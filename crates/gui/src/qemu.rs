//! Boot-test a target device in QEMU.
//!
//! Launches `qemu-system-x86_64` with the selected device attached as a USB
//! stick, so the user can see whether it actually boots, in either BIOS/MBR
//! mode (SeaBIOS, the QEMU default) or UEFI mode (OVMF firmware), without
//! rebooting their real machine.
//!
//! Safety: by default the device is attached with `snapshot=on`, so QEMU
//! diverts every write to a throwaway overlay and the physical device is never
//! modified by the test. The caller can pass `snapshot = false` to let writes
//! persist, needed to run a multi-reboot flow like Windows OOBE to completion
//! (and to inspect the logs it leaves behind), at the cost of mutating the
//! real device.

use std::path::{Path, PathBuf};
use std::process::{Child, Command};

use anyhow::{Context, Result, bail};

/// What the host can offer for a boot test.
pub struct Caps {
    /// `qemu-system-x86_64` is installed.
    pub qemu: bool,
    /// `/dev/kvm` exists, so hardware acceleration is available.
    pub kvm: bool,
    /// OVMF firmware is present, so UEFI boot can be offered.
    pub uefi: bool,
    /// `swtpm` is installed, so a virtual TPM 2.0 can be attached to UEFI
    /// boots; Windows 11 (24H2 especially) wants one or its OOBE loops.
    pub tpm: bool,
}

/// Everything the boot test needs beyond the device path, built by the GUI
/// from the dialog's controls and handed to [`launch`].
pub struct BootConfig {
    /// Guest RAM in MiB.
    pub mem_mb: u32,
    /// Number of vCPUs.
    pub cpus: u32,
    /// 0 = BIOS (SeaBIOS), 1 = UEFI (OVMF), 2 = UEFI + Secure Boot (OVMF).
    pub firmware: u32,
    /// `true` = q35 chipset, `false` = legacy i440fx (QEMU's "pc").
    pub q35: bool,
    /// Attach an Intel HD Audio device routed to the host's audio.
    pub audio: bool,
    /// KVM hardware acceleration.
    pub kvm: bool,
    /// Attach a user-mode NIC (internet) versus no network.
    pub network: bool,
    /// `true` = snapshot mode (discard writes); `false` = persist to the device.
    pub snapshot: bool,
}

/// Probe the host once for boot-test capabilities.
pub fn detect() -> Caps {
    Caps {
        qemu: crate::deps::on_path("qemu-system-x86_64"),
        kvm: Path::new("/dev/kvm").exists(),
        uefi: uefi_available(),
        tpm: swtpm_path().is_some(),
    }
}

/// The host's logical CPU count: the upper bound for the boot test's vCPU
/// slider. At least 1.
pub fn host_cpus() -> u32 {
    std::thread::available_parallelism()
        .map(|n| n.get() as u32)
        .unwrap_or(1)
}

/// The host's total RAM in MiB (from `/proc/meminfo`): the upper bound for the
/// boot test's memory slider. Falls back to 4096 if it can't be read.
pub fn host_ram_mb() -> u32 {
    std::fs::read_to_string("/proc/meminfo")
        .ok()
        .and_then(|s| {
            s.lines().find_map(|l| {
                l.strip_prefix("MemTotal:")?
                    .split_whitespace()
                    .next()?
                    .parse::<u64>()
                    .ok()
            })
        })
        .map(|kb| (kb / 1024) as u32)
        .unwrap_or(4096)
}

/// Resolve the `swtpm` binary. It commonly lives in `/usr/sbin`, which isn't
/// always on a desktop session's PATH, so probe the usual locations directly.
fn swtpm_path() -> Option<PathBuf> {
    [
        "/usr/bin/swtpm",
        "/usr/sbin/swtpm",
        "/bin/swtpm",
        "/sbin/swtpm",
    ]
    .iter()
    .map(PathBuf::from)
    .find(|p| p.is_file())
}

/// Whether OVMF UEFI firmware is installed (so a UEFI boot test is possible).
pub fn uefi_available() -> bool {
    ovmf_paths().is_some()
}

/// Whether a Secure Boot OVMF build is installed (a separate `*.secboot.*`
/// CODE image), so the "UEFI + Secure Boot" firmware option can be offered.
pub fn secureboot_available() -> bool {
    ovmf_secboot_paths().is_some()
}

/// Locate the Secure Boot OVMF firmware (the `*.secboot.*` CODE image plus a
/// VARS template), most-modern-first. Note: paired with a fresh VARS the guest
/// boots in Secure Boot *setup mode* (no keys enrolled): enough to exercise a
/// Secure Boot firmware, short of a pre-enrolled key database.
fn ovmf_secboot_paths() -> Option<(PathBuf, PathBuf)> {
    const PAIRS: &[(&str, &str)] = &[
        (
            "/usr/share/edk2/x64/OVMF_CODE.secboot.4m.fd",
            "/usr/share/edk2/x64/OVMF_VARS.4m.fd",
        ),
        (
            "/usr/share/edk2/x64/OVMF_CODE.secboot.fd",
            "/usr/share/edk2/x64/OVMF_VARS.fd",
        ),
        (
            "/usr/share/OVMF/OVMF_CODE_4M.secboot.fd",
            "/usr/share/OVMF/OVMF_VARS_4M.fd",
        ),
        (
            "/usr/share/OVMF/OVMF_CODE.secboot.fd",
            "/usr/share/OVMF/OVMF_VARS.fd",
        ),
        (
            "/usr/share/edk2/ovmf/OVMF_CODE.secboot.fd",
            "/usr/share/edk2/ovmf/OVMF_VARS.fd",
        ),
    ];
    PAIRS.iter().find_map(|(code, vars)| {
        let (code, vars) = (PathBuf::from(code), PathBuf::from(vars));
        (code.is_file() && vars.is_file()).then_some((code, vars))
    })
}

/// Locate the split OVMF firmware (CODE + VARS) for UEFI boot, trying the
/// common distro layouts most-modern-first. Returns the first pair where both
/// files exist.
fn ovmf_paths() -> Option<(PathBuf, PathBuf)> {
    const PAIRS: &[(&str, &str)] = &[
        // Arch / CachyOS (edk2-ovmf), modern 4 MB split firmware.
        (
            "/usr/share/edk2/x64/OVMF_CODE.4m.fd",
            "/usr/share/edk2/x64/OVMF_VARS.4m.fd",
        ),
        (
            "/usr/share/edk2/x64/OVMF_CODE.fd",
            "/usr/share/edk2/x64/OVMF_VARS.fd",
        ),
        // Debian / Ubuntu (ovmf).
        (
            "/usr/share/OVMF/OVMF_CODE_4M.fd",
            "/usr/share/OVMF/OVMF_VARS_4M.fd",
        ),
        (
            "/usr/share/OVMF/OVMF_CODE.fd",
            "/usr/share/OVMF/OVMF_VARS.fd",
        ),
        // Fedora / openSUSE.
        (
            "/usr/share/edk2/ovmf/OVMF_CODE.fd",
            "/usr/share/edk2/ovmf/OVMF_VARS.fd",
        ),
        (
            "/usr/share/qemu/ovmf-x86_64-code.bin",
            "/usr/share/qemu/ovmf-x86_64-vars.bin",
        ),
    ];
    PAIRS.iter().find_map(|(code, vars)| {
        let (code, vars) = (PathBuf::from(code), PathBuf::from(vars));
        (code.is_file() && vars.is_file()).then_some((code, vars))
    })
}

/// Launch QEMU to boot `device`. Spawns `pkexec qemu-system-x86_64 …`, waits a
/// short grace period to catch immediate startup failures, and returns the
/// still-running child.
///
/// The caller MUST `wait()` on the returned child from the same thread that
/// called `launch`, and that thread must stay alive until QEMU exits: pkexec
/// arms a parent-death watch on the exact *task* (thread) that spawned it, and
/// terminates itself with SIGTERM the moment that thread exits — which cancels
/// the polkit password prompt mid-authentication and kills the running VM.
/// (Waiting also reaps the process, so no zombie is left behind.)
///
/// All the knobs come from [`BootConfig`] (the dialog's controls): vCPUs,
/// memory, firmware (BIOS / UEFI / UEFI+Secure Boot), chipset (q35 / i440fx),
/// audio, KVM, networking, and snapshot mode. For UEFI boots, if `swtpm` is
/// installed a virtual TPM 2.0 is attached automatically; Windows 11 (24H2 in
/// particular) loops in OOBE without one. Runs under `pkexec` because reading a
/// raw block device needs root; the user's desktop-session environment is
/// forwarded so the root-owned QEMU window appears on their display.
pub fn launch(device: &str, cfg: &BootConfig, log: &mut dyn FnMut(&str)) -> Result<Child> {
    if device.is_empty() {
        bail!("no device selected");
    }
    let &BootConfig {
        firmware,
        q35,
        audio,
        kvm,
        network,
        snapshot,
        ..
    } = cfg;
    let mem_mb = cfg.mem_mb.clamp(256, 65_536);
    // vCPUs: caller-chosen (the dialog's slider), clamped to a sane range. More
    // than QEMU's default of 1 makes every guest OS far more responsive.
    let smp = cfg.cpus.clamp(1, host_cpus().max(1));
    // Firmware: 0 = BIOS (SeaBIOS), 1 = UEFI (OVMF), 2 = UEFI + Secure Boot.
    let uefi = firmware != 0;

    let mut qemu: Vec<String> = vec![
        "qemu-system-x86_64".into(),
        "-name".into(),
        format!("usbooty boot test: {device}"),
        // Chipset: q35 (modern PCIe/AHCI, better for Windows 11) or the legacy
        // i440fx ("pc"). Caller's choice via the dialog combobox.
        "-machine".into(),
        if q35 { "q35".into() } else { "pc".into() },
        "-m".into(),
        mem_mb.to_string(),
        "-smp".into(),
        format!("cores={smp},sockets=1"),
        // Resizable window that scales the guest framebuffer to fit, instead of
        // a fixed small box. GTK display, OS-agnostic.
        "-display".into(),
        "gtk,zoom-to-fit=on".into(),
        // Attach the device as a USB stick. snapshot=on (the default) sends
        // every write to a temporary overlay so the real device is untouched;
        // snapshot=off persists writes so an OOBE run can complete across
        // reboots (and leave readable logs behind).
        "-device".into(),
        "qemu-xhci,id=xhci".into(),
        "-drive".into(),
        format!(
            "if=none,id=usbstick,format=raw,snapshot={},file={device}",
            if snapshot { "on" } else { "off" }
        ),
        "-device".into(),
        "usb-storage,bus=xhci.0,drive=usbstick,bootindex=0".into(),
        // A USB tablet is an *absolute* pointing device, so the guest cursor
        // tracks the host cursor 1:1. Without it QEMU's default PS/2 mouse sends
        // relative motion that desyncs in a windowed GUI; the mouse appears not
        // to work while the keyboard is fine.
        "-device".into(),
        "usb-tablet,bus=xhci.0".into(),
        "-boot".into(),
        "menu=on".into(),
        // Networking is explicit so the dialog's checkbox has real effect:
        // user-mode (SLIRP) needs no root/bridge and gives the guest internet;
        // the e1000 model has built-in Windows drivers.
        "-nic".into(),
        if network {
            "user,model=e1000".into()
        } else {
            "none".into()
        },
    ];
    if kvm {
        qemu.push("-enable-kvm".into());
        qemu.push("-cpu".into());
        // Hyper-V enlightenments: paravirtual clock/timers/APIC/spinlocks that a
        // Windows guest uses for much smoother, lower-CPU operation. They are
        // advertised via CPUID and simply ignored by guests that don't use them
        // (Linux, BSD, …), so they're harmless for the non-Windows boot tests.
        qemu.push(
            "host,hv_relaxed,hv_vapic,hv_time,hv_spinlocks=0x1fff,hv_vpindex,\
             hv_runtime,hv_synic,hv_stimer,hv_reset,hv_frequencies"
                .into(),
        );
    }
    if audio {
        // Intel HD Audio with a duplex codec, routed to the host's PipeWire/
        // PulseAudio. QEMU runs under pkexec (root), which has no audio session
        // of its own, so point the PA backend straight at the user's socket
        // (PULSE_SERVER/PULSE_COOKIE are also forwarded below).
        let mut adev = String::from("pa,id=snd0");
        if let Ok(rt) = std::env::var("XDG_RUNTIME_DIR")
            && !rt.is_empty()
        {
            adev.push_str(&format!(",server=unix:{rt}/pulse/native"));
        }
        qemu.push("-audiodev".into());
        qemu.push(adev);
        qemu.push("-device".into());
        qemu.push("intel-hda".into());
        qemu.push("-device".into());
        qemu.push("hda-duplex,audiodev=snd0".into());
    }
    if uefi {
        let (code, vars) = if firmware == 2 {
            ovmf_secboot_paths()
                .context("Secure Boot OVMF firmware not found; install edk2-ovmf / ovmf")?
        } else {
            ovmf_paths().context("UEFI firmware (OVMF) not found; install edk2-ovmf / ovmf")?
        };
        // CODE is the read-only firmware; VARS is the NVRAM template, opened
        // with snapshot=on so the writable overlay lives in a temp file
        // instead of modifying the shared system template.
        qemu.push("-drive".into());
        qemu.push(format!(
            "if=pflash,format=raw,unit=0,readonly=on,file={}",
            code.display()
        ));
        qemu.push("-drive".into());
        qemu.push(format!(
            "if=pflash,format=raw,unit=1,snapshot=on,file={}",
            vars.display()
        ));
    }

    // pkexec scrubs the environment, so forward the desktop-session variables
    // QEMU's GUI needs to reach the user's display, via `env`.
    let mut envs: Vec<String> = Vec::new();
    for var in [
        "WAYLAND_DISPLAY",
        "DISPLAY",
        "XDG_RUNTIME_DIR",
        "XAUTHORITY",
        "XDG_SESSION_TYPE",
        // So `-audiodev pa` can reach the user's PipeWire/PulseAudio from root.
        "PULSE_SERVER",
        "PULSE_COOKIE",
    ] {
        if let Ok(val) = std::env::var(var)
            && !val.is_empty()
        {
            envs.push(format!("{var}={val}"));
        }
    }
    // Root has no ~/.config, so if the host's PA server wants cookie auth and
    // PULSE_COOKIE isn't already set, point it at the user's cookie file.
    if audio
        && !envs.iter().any(|e| e.starts_with("PULSE_COOKIE="))
        && let Ok(home) = std::env::var("HOME")
    {
        let cookie = format!("{home}/.config/pulse/cookie");
        if std::path::Path::new(&cookie).is_file() {
            envs.push(format!("PULSE_COOKIE={cookie}"));
        }
    }

    // A virtual TPM 2.0 for UEFI boots (Windows 11 OOBE needs it). swtpm runs
    // as a separate process exposing a socket QEMU connects to; it's started
    // and torn down inside the same pkexec shell as QEMU.
    let swtpm = if uefi { swtpm_path() } else { None };

    // We need a shell wrapper whenever there's setup/teardown to do around
    // QEMU: unmounting the device (snapshot off) and/or running swtpm (TPM).
    let mut cmd = Command::new("pkexec");
    if snapshot && swtpm.is_none() {
        cmd.arg("/usr/bin/env");
        cmd.args(&envs);
        cmd.args(&qemu);
    } else {
        // The shell gets: $0=name, $1=device, then `/usr/bin/env <envs>
        // <qemu...>` as "$@". It drops the device arg, does any unmount/TPM
        // setup, then runs QEMU (with TPM args appended).
        let mut script = String::from("dev=\"$1\"; shift\n");
        if !snapshot {
            // Snapshot off needs exclusive write access to the raw device, so
            // unmount any partitions the desktop auto-mounted (common right
            // after a replug), otherwise QEMU's image lock fails with "Failed
            // to get write lock".
            script.push_str(
                "for p in \"$dev\"*; do [ \"$p\" != \"$dev\" ] && umount \"$p\" 2>/dev/null; done\n",
            );
        }
        if let Some(sw) = &swtpm {
            // Start swtpm on a unix socket, wait (briefly) for it to appear,
            // run QEMU wired to it, then tear swtpm down and clean up state.
            // --terminate makes swtpm also exit when QEMU disconnects.
            script.push_str(&format!(
                "tpmdir=$(mktemp -d); sock=\"$tpmdir/swtpm.sock\"\n\
                 {sw} socket --tpmstate \"dir=$tpmdir\" --ctrl \"type=unixio,path=$sock\" --tpm2 --terminate >/dev/null 2>&1 &\n\
                 swp=$!\n\
                 n=0; while [ ! -S \"$sock\" ] && [ $n -lt 50 ]; do n=$((n+1)); sleep 0.1; done\n\
                 \"$@\" -chardev \"socket,id=chrtpm,path=$sock\" -tpmdev emulator,id=tpm0,chardev=chrtpm -device tpm-tis,tpmdev=tpm0\n\
                 kill \"$swp\" 2>/dev/null; rm -rf \"$tpmdir\"\n",
                sw = sw.display(),
            ));
        } else {
            script.push_str("exec \"$@\"\n");
        }
        cmd.arg("sh").arg("-c").arg(script);
        cmd.arg("usbooty-boot-test"); // $0
        cmd.arg(device); // $1
        cmd.arg("/usr/bin/env");
        cmd.args(&envs);
        cmd.args(&qemu);
    }

    // Log the full invocation to the GUI's activity log so the boot test is
    // transparent and diagnosable.
    log(&format!(
        "Boot test {device}: {mem_mb} MiB, {smp} vCPU, firmware={fw}, machine={mach}, \
         kvm={kvm}, network={network}, snapshot={snapshot}, audio={audio}, tpm={tpm}",
        fw = match firmware {
            0 => "BIOS",
            1 => "UEFI",
            _ => "UEFI+SecureBoot",
        },
        mach = if q35 { "q35" } else { "i440fx" },
        tpm = swtpm.is_some(),
    ));
    if !snapshot {
        log(
            "snapshot off → unmounting the device's partitions first; writes persist to the device",
        );
    }
    if let Some(sw) = &swtpm {
        log(&format!("virtual TPM: {} (tpm-tis, TPM 2.0)", sw.display()));
    }
    if !envs.is_empty() {
        log(&format!("forwarded env: {}", envs.join(" ")));
    }
    log(&format!("qemu: {}", qemu.join(" ")));

    // Capture stderr so an immediate failure (image-lock conflict, bad args,
    // audio backend that can't connect) surfaces in the app instead of only the
    // terminal. QEMU fails fast at startup; if it's still alive after a short
    // grace period we treat the launch as a success and hand the child back to
    // the caller to wait on. (When pkexec has to prompt for a password the
    // grace window just sees pkexec still waiting, so this is best-effort; it
    // reliably catches failures once polkit auth is cached, i.e. while
    // iterating on boot tests.)
    let mut errfile = tempfile::tempfile().context("creating a temp file for QEMU stderr")?;
    cmd.stderr(errfile.try_clone().context("cloning the stderr handle")?);
    let mut child = cmd.spawn().context("launching pkexec qemu-system-x86_64")?;

    std::thread::sleep(std::time::Duration::from_millis(700));
    if let Some(status) = child.try_wait().context("polling QEMU")?
        && !status.success()
    {
        use std::io::{Read, Seek};
        let mut out = String::new();
        errfile.rewind().ok();
        errfile.read_to_string(&mut out).ok();
        log(&format!(
            "QEMU exited immediately (status {:?})",
            status.code()
        ));
        for l in out.lines().map(str::trim).filter(|l| !l.is_empty()) {
            log(&format!("qemu: {l}"));
        }
        let detail = out
            .lines()
            .rev()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("")
            .trim();
        let lower = out.to_lowercase();
        let hint = if lower.contains("lock") {
            ": the device is held by another process; close any other QEMU window using it"
        } else if lower.contains("audio") || lower.contains("pulse") || lower.contains("pipewire") {
            ": audio backend failed to start; try unchecking Guest audio"
        } else {
            ""
        };
        bail!(
            "QEMU exited immediately{}{}",
            if detail.is_empty() {
                String::new()
            } else {
                format!(": {detail}")
            },
            hint
        );
    }
    Ok(child)
}
