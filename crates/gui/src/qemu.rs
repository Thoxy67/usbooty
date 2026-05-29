//! Boot-test a target device in QEMU.
//!
//! Launches `qemu-system-x86_64` with the selected device attached as a USB
//! stick, so the user can see whether it actually boots, in either BIOS/MBR
//! mode (SeaBIOS, the QEMU default) or UEFI mode (OVMF firmware), without
//! rebooting their real machine.
//!
//! Safety: the device is always attached with `snapshot=on`, so QEMU diverts
//! every write to a throwaway overlay and the physical device is never
//! modified by the test.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

/// What the host can offer for a boot test.
pub struct Caps {
    /// `qemu-system-x86_64` is installed.
    pub qemu: bool,
    /// `/dev/kvm` exists, so hardware acceleration is available.
    pub kvm: bool,
    /// OVMF firmware is present, so UEFI boot can be offered.
    pub uefi: bool,
}

/// Probe the host once for boot-test capabilities.
pub fn detect() -> Caps {
    Caps {
        qemu: crate::deps::on_path("qemu-system-x86_64"),
        kvm: Path::new("/dev/kvm").exists(),
        uefi: uefi_available(),
    }
}

/// Whether OVMF UEFI firmware is installed (so a UEFI boot test is possible).
pub fn uefi_available() -> bool {
    ovmf_paths().is_some()
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

/// Launch QEMU to boot `device`. Non-blocking: spawns the process and returns
/// immediately (the QEMU window then runs independently).
///
/// `uefi` selects OVMF (UEFI) firmware versus the default SeaBIOS (BIOS/MBR);
/// `kvm` enables hardware acceleration. Runs under `pkexec` because reading a
/// raw block device needs root, the user's desktop-session environment is
/// forwarded so the root-owned QEMU window appears on their display.
pub fn launch(device: &str, mem_mb: u32, uefi: bool, kvm: bool) -> Result<()> {
    if device.is_empty() {
        bail!("no device selected");
    }
    let mem_mb = mem_mb.clamp(256, 65_536);

    let mut qemu: Vec<String> = vec![
        "qemu-system-x86_64".into(),
        "-name".into(),
        format!("usbooty boot test: {device}"),
        "-m".into(),
        mem_mb.to_string(),
        // Attach the device as a USB stick. snapshot=on means every write
        // goes to a temporary overlay, the real device is never touched.
        "-device".into(),
        "qemu-xhci,id=xhci".into(),
        "-drive".into(),
        format!("if=none,id=usbstick,format=raw,snapshot=on,file={device}"),
        "-device".into(),
        "usb-storage,bus=xhci.0,drive=usbstick,bootindex=0".into(),
        "-boot".into(),
        "menu=on".into(),
    ];
    if kvm {
        qemu.push("-enable-kvm".into());
        qemu.push("-cpu".into());
        qemu.push("host".into());
    }
    if uefi {
        let (code, vars) =
            ovmf_paths().context("UEFI firmware (OVMF) not found; install edk2-ovmf / ovmf")?;
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
    ] {
        if let Ok(val) = std::env::var(var)
            && !val.is_empty()
        {
            envs.push(format!("{var}={val}"));
        }
    }

    let mut cmd = Command::new("pkexec");
    cmd.arg("/usr/bin/env");
    cmd.args(&envs);
    cmd.args(&qemu);
    cmd.spawn().context("launching pkexec qemu-system-x86_64")?;
    Ok(())
}
