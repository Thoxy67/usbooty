//! `AppController` methods that build a [`usbooty_core::Job`] and drive the
//! privileged helper: the write/format/check/backup lifecycle and cancellation.

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::QString;
use usbooty_core::{
    CheckMode, DeviceInfo, FileSystem, Job, JobOptions, PartitionTable, Persistence, WimStrategy,
};

use super::helpers::unmount_device_partitions;
use super::{JobHandle, qobject};

/// Unwind the "Running…" UI from a worker thread when a pre-flight check
/// fails before the helper was ever spawned. With `refresh`, the device
/// list is re-scanned too (used when the failure suggests the device set
/// changed under us).
fn abort_start(qt: &cxx_qt::CxxQtThread<qobject::AppController>, message: String, refresh: bool) {
    let _ = qt.queue(
        move |mut ctrl: core::pin::Pin<&mut qobject::AppController>| {
            ctrl.as_mut().set_busy(false);
            ctrl.as_mut().set_phase(QString::default());
            ctrl.as_mut().set_status(QString::from(&message));
            ctrl.as_mut().rust_mut().job = None;
            if refresh {
                ctrl.as_mut().refresh_devices();
            }
        },
    );
}

impl qobject::AppController {
    /// Whether [`start`](Self::start) would currently do anything useful.
    pub fn can_start(&self) -> bool {
        if *self.busy() || *self.selected_device() < 0 {
            return false;
        }
        match *self.method() {
            // Format-only takes no ISO and has nothing to fit-check.
            2 => true,
            // Ventoy: an ISO is optional, but if given it must fit.
            3 => self.fit_warning().to_string().is_empty(),
            // FreeDOS bootable USB needs no ISO at all, just the device.
            4 => true,
            _ => {
                !self.iso_path().to_string().is_empty() && self.fit_warning().to_string().is_empty()
            }
        }
    }

    /// Reset every UI readout that a long-running job will start fresh: enter
    /// the busy state, zero the progress bar, clear the previous run's log,
    /// speed and ETA, and show the caller's `status` banner. Callers vary the
    /// `status` string ("Running…", "Backing up…", …); the rest of the reset
    /// is identical across job kinds, so this helper keeps the four start_*
    /// invokables from drifting apart.
    pub(crate) fn init_job_ui(mut self: core::pin::Pin<&mut Self>, status: &str) {
        self.as_mut().set_busy(true);
        self.as_mut().set_progress(0.0);
        self.as_mut().set_phase(QString::from("Starting"));
        self.as_mut().clear_log();
        self.as_mut().set_speed(QString::default());
        self.as_mut().set_eta(QString::default());
        self.as_mut().set_status(QString::from(status));
    }

    /// Validate inputs, build a [`Job`], and spawn the privileged helper.
    pub fn start(mut self: core::pin::Pin<&mut Self>) {
        if !self.can_start() {
            self.as_mut()
                .set_status(QString::from("Select an ISO and a target device first"));
            return;
        }

        let Some(selected) = self.selected_info().cloned() else {
            self.as_mut()
                .set_status(QString::from("Select a target device first"));
            return;
        };

        let iso = self.iso_path().to_string();
        let device = selected.path.clone();

        // Keep this in sync with the QML combo's `model` order below.
        let table = match *self.table() {
            1 => PartitionTable::Mbr,
            2 => PartitionTable::MbrBiosUefi,
            3 => PartitionTable::HybridMbrGpt,
            _ => PartitionTable::Gpt,
        };
        let label = self.label().to_string();
        let full_format = *self.full_format();
        let verify = *self.verify();
        let log_all_files = *self.log_all_files();

        let job = match *self.method() {
            0 => Job::Dd {
                iso_path: iso.into(),
                device_path: device.into(),
                opts: JobOptions::default(),
            },
            2 => Job::Format {
                device_path: device.into(),
                table,
                filesystem: self.filesystem_kind_from_index(*self.filesystem()),
                opts: JobOptions {
                    label,
                    full_format,
                    verify,
                    log_all_files,
                },
            },
            3 => Job::Ventoy {
                device_path: device.into(),
                table,
                secure_boot: *self.ventoy_secure_boot(),
                update: *self.ventoy_update(),
                // Seed the Ventoy partition with the loaded ISO, if any.
                iso_path: (!iso.is_empty()).then(|| iso.into()),
            },
            4 => {
                // FreeDOS bootable USB. The user can pick FAT16 or FAT32
                // (anything else is rejected by the helper). The cached
                // FreeDOS files get filled in by the runner; see
                // `crates/gui/src/runner.rs::run_job`.
                let filesystem = match self.filesystem_kind_from_index(*self.filesystem()) {
                    fs @ (usbooty_core::FileSystem::Fat16 | usbooty_core::FileSystem::Fat32) => fs,
                    _ => usbooty_core::FileSystem::Fat32,
                };
                Job::Freedos {
                    device_path: device.into(),
                    table,
                    filesystem,
                    // Runner replaces these placeholders with real cache paths
                    // after `resources::ensure_freedos` returns.
                    kernel_sys: std::path::PathBuf::new(),
                    command_com: std::path::PathBuf::new(),
                    boot_bin: std::path::PathBuf::new(),
                    opts: JobOptions {
                        label,
                        full_format,
                        verify,
                        log_all_files,
                    },
                }
            }
            _ => {
                // Filesystem and large-`install.wim` handling are decided
                // automatically from the ISO analysis: NTFS + UEFI:NTFS for a
                // Windows ISO with an oversized install.wim, FAT32 otherwise.
                // When the user chose `Split`, override the layout to FAT32
                // and let `wimsplit` chunk install.wim after the copy.
                let (mut filesystem, mut wim) = self
                    .rust()
                    .iso_report
                    .as_ref()
                    .map(usbooty_core::auto_filesystem)
                    .unwrap_or((FileSystem::Fat32, WimStrategy::None));
                if *self.split_wim() && wim == WimStrategy::UefiNtfs {
                    filesystem = FileSystem::Fat32;
                    wim = WimStrategy::Split;
                }
                // A persistent overlay, when the ISO supports it and the user
                // asked for it. Partition-based schemes need a non-zero slider
                // value (the partition size); inline-directory schemes
                // (currently Slax) ignore the slider; the data partition
                // itself absorbs writes, no separate partition to size.
                let persistence = self
                    .rust()
                    .iso_report
                    .as_ref()
                    .and_then(|r| r.persistence)
                    .filter(|kind| !kind.needs_partition() || *self.persistence_size() > 0)
                    .map(|kind| Persistence {
                        kind,
                        size_bytes: if kind.needs_partition() {
                            *self.persistence_size() as u64 * 1024 * 1024
                        } else {
                            0
                        },
                    });
                // Windows-installer customization, when the source is Windows.
                let windows_setup = if *self.windows_iso() {
                    let setup = self.collect_windows_setup();
                    setup.is_active().then_some(setup)
                } else {
                    None
                };
                // Offer the legacy-BIOS Syslinux/extlinux installer for Linux
                // ISOs that ship an isolinux config; Windows ISOs already
                // come with their own boot loader, so skip them.
                let install_bootloader = *self.linux_iso()
                    && self
                        .rust()
                        .iso_report
                        .as_ref()
                        .is_some_and(|r| r.has_isolinux);
                Job::Partitioned {
                    iso_path: iso.into(),
                    device_path: device.into(),
                    table,
                    filesystem,
                    wim,
                    // The runner downloads and fills this in when needed.
                    uefi_ntfs_img: None,
                    persistence,
                    windows_setup,
                    install_bootloader,
                    // Forward the detected distribution so the helper can
                    // run the matching post-copy quirk fixes.
                    distro: self
                        .rust()
                        .iso_report
                        .as_ref()
                        .map(|r| r.distro)
                        .unwrap_or_default(),
                    opts: JobOptions {
                        label,
                        full_format,
                        verify,
                        log_all_files,
                    },
                }
            }
        };

        self.as_mut().init_job_ui("Running…");
        self.spawn_helper_job(job, selected);
    }

    /// Park a [`JobHandle`] and run the shared destructive-job pre-flight on
    /// a worker thread: re-enumerate and re-verify the chosen device, ask the
    /// desktop session to unmount its partitions, then hand off to the
    /// runner. Shared by `start`, `start_check`, and `start_backup` so every
    /// device-touching job gets the same safety re-scan.
    fn spawn_helper_job(mut self: core::pin::Pin<&mut Self>, job: Job, selected: DeviceInfo) {
        let handle = JobHandle::new();
        let stdin_slot = handle.stdin.clone();
        let cancel = handle.cancel.clone();
        let qt_thread = self.qt_thread();
        let show_fixed = *self.show_fixed_disks();

        // The safety re-scan and the udisksctl unmount below both block (the
        // unmount syncs dirty pages, routinely for seconds on a part-written
        // stick), so they run on the worker, before pkexec is spawned.
        std::thread::spawn(move || {
            // Confirm the chosen device still exists exactly as it was
            // enumerated. A USB drive swapped into this slot since the user
            // picked it would reuse the same `/dev` node; writing to it
            // would destroy the wrong disk. Any mismatch aborts and forces
            // a fresh scan.
            let current = crate::devices::enumerate(show_fixed);
            if !current.contains(&selected) {
                abort_start(
                    &qt_thread,
                    "The selected device changed since it was chosen. \
                     The device list has been refreshed; check the target and start again."
                        .into(),
                    true,
                );
                return;
            }
            // Pre-flight: ask the desktop session (via udisksctl) to release
            // any partition of the target it still has mounted. udisksctl
            // runs as the user, notifies file managers, and triggers the
            // polkit prompt when needed, which is friendlier than letting
            // the helper's kernel-level unmount fight through a still-open
            // mount. Anything left mounted afterwards is reported and the
            // job aborts.
            if let Err(err) = unmount_device_partitions(&selected.path) {
                abort_start(
                    &qt_thread,
                    format!(
                        "Could not unmount {}: {err} \
                         Close any file manager that has it open and try again.",
                        selected.path,
                    ),
                    false,
                );
                return;
            }
            if !std::path::Path::new(&selected.path).exists() {
                abort_start(
                    &qt_thread,
                    format!("{} no longer exists. Was the drive removed?", selected.path),
                    true,
                );
                return;
            }
            // A cancel clicked during the (multi-second) pre-flight above has
            // no helper stdin to land on; honour the flag before any further
            // work. The runner re-checks it around the helper spawn too.
            if cancel.load(std::sync::atomic::Ordering::SeqCst) {
                abort_start(&qt_thread, "Cancelled.".into(), false);
                return;
            }
            crate::runner::run_job(job, qt_thread, stdin_slot, cancel);
        });

        self.as_mut().rust_mut().job = Some(handle);
    }

    /// Build a [`Job::Check`] for the currently-selected device and run it.
    pub fn start_check(mut self: core::pin::Pin<&mut Self>, mode_index: i32) {
        if *self.busy() {
            return;
        }
        let Some(selected) = self.selected_info().cloned() else {
            self.as_mut()
                .set_status(QString::from("Select a target device first"));
            return;
        };
        let mode = if mode_index == 1 {
            CheckMode::Full
        } else {
            CheckMode::Quick
        };

        let job = Job::Check {
            device_path: selected.path.clone().into(),
            mode,
        };

        self.as_mut().init_job_ui("Checking device…");
        // The pattern check overwrites the device; give it the same
        // pre-flight (identity re-verify + unmount) as start().
        self.spawn_helper_job(job, selected);
    }

    /// Build a [`Job::Backup`] for the currently-selected device and run it.
    pub fn start_backup(mut self: core::pin::Pin<&mut Self>, image_path: &QString) {
        if *self.busy() {
            return;
        }
        let Some(selected) = self.selected_info().cloned() else {
            self.as_mut()
                .set_status(QString::from("Select a target device first"));
            return;
        };
        let path = super::helpers::local_path_from_url(&image_path.to_string());
        if path.is_empty() {
            self.as_mut()
                .set_status(QString::from("Pick an output file for the backup"));
            return;
        }

        let job = Job::Backup {
            device_path: selected.path.clone().into(),
            image_path: path.into(),
            opts: JobOptions {
                label: String::new(),
                full_format: false,
                verify: *self.verify(),
                log_all_files: false,
            },
        };

        self.as_mut().init_job_ui("Backing up…");
        // Reading the device is non-destructive, but the unmount pre-flight
        // still matters: a backup taken while dirty pages are unflushed
        // snapshots an inconsistent filesystem.
        self.spawn_helper_job(job, selected);
    }

    /// Ask the running job to abort. A running helper hears about it through
    /// a `cancel` line on its stdin; everything that happens before or
    /// without a helper (pre-flight, resource downloads, decompression,
    /// the Windows-ISO download) polls the handle's atomic flag, so flip
    /// both unconditionally; a cancel must never be lost just because the
    /// helper isn't up yet.
    pub fn cancel(mut self: core::pin::Pin<&mut Self>) {
        if let Some(job) = &self.rust().job {
            job.cancel.store(true, std::sync::atomic::Ordering::SeqCst);
            if let Ok(mut guard) = job.stdin.lock()
                && let Some(stdin) = guard.as_mut()
            {
                use std::io::Write;
                let _ = writeln!(stdin, "cancel");
                let _ = stdin.flush();
            }
        }
        self.as_mut().set_status(QString::from("Cancelling…"));
    }

    /// Resolve a filesystem-combo index against the list of filesystems
    /// whose mkfs tool is actually installed. Falls back to the first entry
    /// (or FAT32) if the index is out of range; the QML side is responsible
    /// for keeping `filesystem` in `[0, available_filesystem_kinds.len())`,
    /// but a stale binding shouldn't crash the app.
    fn filesystem_kind_from_index(&self, index: i32) -> FileSystem {
        let kinds = &self.rust().available_filesystem_kinds;
        if index >= 0
            && let Some(fs) = kinds.get(index as usize)
        {
            return *fs;
        }
        kinds.first().copied().unwrap_or(FileSystem::Fat32)
    }
}
