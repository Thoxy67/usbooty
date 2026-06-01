//! Windows To Go: lay a full, directly-bootable Windows installation onto the
//! device: a portable Windows that boots from the USB itself rather than a
//! Windows *installer* (the [`partitioned`](crate::partitioned) path).
//!
//! UEFI/GPT only. The flow mirrors what Rufus does on Windows, but every
//! Windows-only API is replaced with a Linux equivalent:
//!
//! 1. Partition GPT: an EFI System Partition (FAT32) + a main NTFS partition.
//! 2. `wimlib-imagex apply` the chosen edition out of the ISO's `install.wim`
//!    onto the NTFS partition (replaces wimgapi).
//! 3. Copy the Windows boot files from the applied image onto the ESP
//!    (replaces the file-copy half of `bcdboot`).
//! 4. Generate a portable BCD boot store with `hivex` (replaces the
//!    BCD-generating half of `bcdboot`; see [`crate::bcd`]).
//! 5. Optionally drop a `specialize`+`oobeSystem` `unattend.xml` into
//!    `\Windows\Panther\` for first-boot customization (no `windowsPE` pass;
//!    Setup never runs here).

use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use usbooty_core::{JobOptions, WindowsSetup, device::format_size};

use crate::{bcd, blockdev, emit, fsutil, partition, wimapply};

/// ESP size: 512 MiB. The boot files total ~30 MiB, but 512 MiB matches
/// Microsoft's modern ESP sizing and leaves room for locale resources and a
/// `SkuSiPolicy.p7b` drop-in.
const ESP_BYTES: u64 = 512 * 1024 * 1024;

/// A coarse sanity floor for the device: the ESP plus a minimal Windows
/// footprint. `wimlib-imagex apply` fails cleanly if the real edition does not
/// fit, but this catches an obviously-too-small stick before we wipe it.
const MIN_WINDOWS_BYTES: u64 = 12 * 1024 * 1024 * 1024;

pub fn run(
    iso: &Path,
    device: &Path,
    image_index: u32,
    windows_setup: Option<&WindowsSetup>,
    opts: &JobOptions,
    abort: &AtomicBool,
) -> Result<()> {
    // ---- Preconditions (fail before touching the device) ------------------
    if !fsutil::wimlib_available() {
        bail!(
            "wimlib-imagex is required for Windows To Go; install the \
             `wimlib` / `wimtools` package"
        );
    }
    if !bcd::hivex_available() {
        bail!(
            "hivexsh is required for Windows To Go (to build the boot store); \
             install the `hivex` package"
        );
    }

    // ---- Resolve the source WIM out of the ISO ----------------------------
    let iso_mount = fsutil::LoopMount::open_iso(iso, "wim")?;
    let wim = resolve_install_image(iso_mount.path())
        .context("the ISO has no sources/install.wim or install.esd to apply")?;

    // ---- Prepare + partition (GPT: ESP + NTFS) ----------------------------
    let unmounted = blockdev::unmount_all(device)?;
    if unmounted > 0 {
        emit::log(format!("Unmounted {unmounted} filesystem(s) from the target"));
    }
    if opts.full_format {
        blockdev::zero_device(device, abort)?;
    }

    emit::phase("Partitioning");
    let ids = {
        let mut dev = blockdev::open_exclusive(device)?;
        let dev_size = blockdev::device_size(&dev)?;
        if dev_size < ESP_BYTES + MIN_WINDOWS_BYTES {
            bail!(
                "the target device ({}) is too small for Windows To Go; \
                 use a drive of at least {}",
                format_size(dev_size),
                format_size(ESP_BYTES + MIN_WINDOWS_BYTES)
            );
        }
        emit::log(format!("Target {}, {}", device.display(), format_size(dev_size)));
        emit::log("Wiping old partition tables and filesystem signatures");
        partition::wipe_signatures(&mut dev, dev_size)?;
        emit::log("Writing a GPT layout: an EFI System Partition plus a main NTFS partition");
        let ids = partition::write_gpt_esp_ntfs(&mut dev, ESP_BYTES, &opts.label)?;
        let _ = nix::unistd::fsync(&dev);
        blockdev::reread_partition_table(&dev);
        ids
    };

    let esp_part = blockdev::partition_path(device, 1);
    let main_part = blockdev::partition_path(device, 2);
    fsutil::wait_for_node(&esp_part)?;
    fsutil::wait_for_node(&main_part)?;

    emit::phase("Formatting");
    fsutil::mkfs_vfat(&esp_part, "ESP")?;
    fsutil::mkfs_ntfs(&main_part, &opts.label)?;

    // ---- Apply the image, then lay out boot files + BCD off the same mount
    {
        let ntfs = fsutil::Mount::new_ntfs(&main_part)?;
        wimapply::apply_image(&wim, image_index, ntfs.path(), abort)?;

        let boot_efi = fsutil::ci_path(ntfs.path(), &["Windows", "Boot", "EFI"])
            .context("the applied image has no Windows\\Boot\\EFI")?;
        let fonts = fsutil::ci_path(ntfs.path(), &["Windows", "Boot", "Fonts"]).ok();
        let bootmgfw = fsutil::ci_path(&boot_efi, &["bootmgfw.efi"])
            .context("the applied image has no bootmgfw.efi")?;

        emit::phase("Installing boot files");
        {
            let esp = fsutil::Mount::new(&esp_part, "vfat")?;
            let ms_boot = esp.path().join("EFI/Microsoft/Boot");
            std::fs::create_dir_all(&ms_boot)
                .with_context(|| format!("creating {}", ms_boot.display()))?;

            // Mirror `bcdboot /f UEFI`: copy the whole Boot\EFI tree (bootmgfw,
            // .mui locale resources, boot.stl) and the Fonts directory.
            copy_tree(&boot_efi, &ms_boot)?;
            if let Some(fonts) = &fonts {
                copy_tree(fonts, &ms_boot.join("Fonts"))?;
            }
            // Removable-media fallback path the firmware always tries.
            let efi_boot = esp.path().join("EFI/Boot");
            std::fs::create_dir_all(&efi_boot)
                .with_context(|| format!("creating {}", efi_boot.display()))?;
            std::fs::copy(&bootmgfw, efi_boot.join("bootx64.efi"))
                .context("copying bootmgfw.efi to the removable fallback path")?;

            emit::phase("Writing boot configuration");
            let bcd_ids = bcd::DeviceIds {
                disk_guid: ids.disk_guid,
                esp_guid: ids.esp_guid,
                ntfs_guid: ids.ntfs_guid,
            };
            bcd::generate(&ms_boot.join("BCD"), &bcd_ids, "Windows To Go", "en-US")?;

            // The recovery store at \EFI\Microsoft\Recovery\BCD. Windows' boot-
            // file servicing (BFSVC) opens it during OOBE; without it, OOBE
            // fails (0xc000000f) and loops on "Why did my PC restart?".
            let recovery = esp.path().join("EFI/Microsoft/Recovery");
            std::fs::create_dir_all(&recovery)
                .with_context(|| format!("creating {}", recovery.display()))?;
            bcd::generate_recovery(&recovery.join("BCD"), &bcd_ids, "en-US")?;

            // `esp` drops here: sync + unmount.
        }

        // Resolve the customization up front (the disk-policy step below needs
        // it). With no setup supplied, fall back to a default that still keeps
        // internal disks offline, the Rufus default and the safe choice for WTG.
        let default_setup = usbooty_core::WindowsSetup {
            wtg_offline_internal_disks: true,
            ..usbooty_core::WindowsSetup::default()
        };
        let setup = windows_setup.unwrap_or(&default_setup);

        // Keep the host machine's internal disks offline (Windows To Go
        // standard: partmgr SanPolicy=4). Stops the portable OS from mounting
        // or altering the host's own volumes, and prevents the host's internal
        // Windows disk being seen during OOBE (the "[setup.exe] System disks
        // found" line), which on 24H2/25H2 makes CloudExperienceHost reseal
        // OOBE and reboot-loop. Mirrors Rufus's offlineServicing SanPolicy step
        // ("Prevent Windows To Go from accessing internal disks"), and like
        // Rufus it is an opt-out toggle (on by default).
        if setup.wtg_offline_internal_disks {
            emit::phase("Configuring disk policy");
            let system_hive = fsutil::ci_path(
                ntfs.path(),
                &["Windows", "System32", "config", "SYSTEM"],
            )
            .context("the applied image has no SYSTEM registry hive")?;
            bcd::set_internal_disks_offline(&system_hive)?;
        }

        // ---- First-boot customization ------------------------------------
        // Always write the offline unattend so the specialize machine settings
        // (BypassNRO, computer name, time zone) apply; first boot then runs the
        // normal Windows OOBE account-creation flow, matching Rufus. The
        // 24H2/25H2 reseal loop ("Why did my PC restart?") is headed off at the
        // BCD/registry layer instead (recovery store + recoveryenabled=No +
        // SanPolicy=4 above), not by an Audit-mode redirect. Any user
        // customization rides along.
        emit::phase("Applying customization");
        let windows_root = fsutil::ci_path(ntfs.path(), &["Windows"])
            .context("the applied image has no Windows directory")?;
        crate::unattend::write_offline(&windows_root, setup)?;

        // Debloat + desktop helpers can't ride the unattend's
        // import-from-USB-media path on a running WTG system, so apply them
        // straight to the applied image (offline registry hives + the
        // Default user's Desktop) instead.
        if setup.apply_debloat {
            emit::phase("Applying debloat");
            crate::unattend::apply_offline_debloat(ntfs.path())?;
        }
        if setup.desktop_helpers {
            emit::phase("Installing desktop helpers");
            crate::unattend::write_desktop_helpers_offline(ntfs.path())?;
        }

        emit::phase("Flushing");
        // `ntfs` drops here: sync + unmount.
    }

    if opts.verify {
        verify_boot_files(&esp_part)?;
    }

    emit::log("Windows To Go drive created");
    Ok(())
}

/// Find `sources/install.wim` or `sources/install.esd` under the mounted ISO.
fn resolve_install_image(iso_root: &Path) -> Result<PathBuf> {
    fsutil::ci_path(iso_root, &["sources", "install.wim"])
        .or_else(|_| fsutil::ci_path(iso_root, &["sources", "install.esd"]))
}

/// Recursively copy a directory tree from `src` into `dst` (created if needed).
fn copy_tree(src: &Path, dst: &Path) -> Result<()> {
    std::fs::create_dir_all(dst).with_context(|| format!("creating {}", dst.display()))?;
    for entry in std::fs::read_dir(src).with_context(|| format!("reading {}", src.display()))? {
        let entry = entry?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_tree(&from, &to)?;
        } else {
            std::fs::copy(&from, &to)
                .with_context(|| format!("copying {} to {}", from.display(), to.display()))?;
        }
    }
    Ok(())
}

/// Light post-write verify: re-mount the ESP read-only and confirm the boot
/// manager and BCD store are present and non-empty. A byte-for-byte verify is
/// meaningless here; the payload is an applied image, not a verbatim copy.
fn verify_boot_files(esp_part: &str) -> Result<()> {
    emit::phase("Verifying");
    let esp = fsutil::Mount::new(esp_part, "vfat")?;
    for rel in ["EFI/Microsoft/Boot/bootmgfw.efi", "EFI/Microsoft/Boot/BCD"] {
        let p = esp.path().join(rel);
        let md = std::fs::metadata(&p).with_context(|| format!("{rel} is missing on the ESP"))?;
        if md.len() == 0 {
            bail!("{rel} on the ESP is empty");
        }
    }
    emit::log("Boot files verified on the ESP");
    Ok(())
}
