//! Generate a Windows BCD (Boot Configuration Data) boot store on Linux,
//! replacing the Windows-only `bcdboot.exe` step of a Windows To Go build.
//!
//! ## Approach
//!
//! `bootmgfw.efi` refuses to boot without a valid `\EFI\Microsoft\Boot\BCD`
//! store, and there is no `bcdboot` on Linux. The store is a Windows registry
//! hive, which `hivex` can manipulate. The method here is ported from
//! **BCD-SYS** by Joseph P. Zeller (GPL-3.0, <https://github.com/jpz4085/BCD-SYS>):
//!
//! 1. Start from a blank hive ([`BCD_NEW`]); `libhivex` cannot create a hive
//!    from nothing, so a minimal empty one is vendored.
//! 2. Merge a skeleton ([`WINLOAD_REG`]) with `hivexregedit`. It pre-creates the
//!    Windows Boot Manager object and the inheritable settings objects, with the
//!    boot-manager element keys already present (so the fill step only has to
//!    `cd` into them, never `add` an existing key, the bug an earlier version
//!    hit).
//! 3. Run a `hivexsh` script ([`build_fill_script`]) that adds the Windows OS
//!    loader + resume entries (fresh random GUIDs) and fills the device, path,
//!    default and display-order elements.
//!
//! ## Portability
//!
//! The `device`/`osdevice` elements are **partition** descriptors that identify
//! the partition by its GPT *partition GUID* plus the *disk GUID* ([`device_element`]).
//! Both live in the USB's own GPT, so the references stay valid on whatever
//! machine the stick is plugged into, no disk-signature rewrite needed.

use anyhow::{Context, Result, bail};
use std::collections::BTreeSet;
use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};

use crate::emit;

/// Blank BCD hive (registry) used as the base to merge into. Vendored from
/// BCD-SYS `Templates/BCD-NEW` (libhivex can't create a hive from scratch).
const BCD_NEW: &[u8] = include_bytes!("bcd/BCD-NEW");
/// Skeleton of the boot-manager and settings objects, merged with
/// `hivexregedit --prefix BCD00000001`. Vendored from BCD-SYS `Templates/winload.reg`.
const WINLOAD_REG: &str = include_str!("bcd/winload.reg");
/// Skeleton for the *recovery* store (`\EFI\Microsoft\Recovery\BCD`): a minimal
/// boot-manager object plus the inheritable settings objects. Vendored from
/// BCD-SYS `Templates/recovery.reg`. Windows' boot-file-servicing (BFSVC) runs
/// during OOBE and opens this store; if it is missing it fails with 0xc000000f
/// and OOBE loops on "Why did my PC restart?".
const RECOVERY_REG: &str = include_str!("bcd/recovery.reg");

/// `{bootmgr}`: the Windows Boot Manager object (fixed, well-known).
const BOOTMGR: &str = "{9dea862c-5cdd-4e70-acc1-f32b344d4795}";
/// `{memdiag}`: the Windows Memory Tester object (present in the skeleton).
const MEMDIAG: &str = "{b2721d73-1db4-4c62-bf78-c548a880142d}";
/// Inheritable settings the OS loader / resume loader reference via element 14000006.
const BOOTLOADERSETTINGS: &str = "{6efb52bf-1766-41db-a6b3-0ee5eff72bd7}";
const RESUMELOADERSETTINGS: &str = "{1afa9c49-16ab-4a5c-901b-212802da9460}";

/// GPT identifiers of the freshly-written Windows To Go layout, read straight
/// from the partition table. All three are raw on-disk (mixed-endian) 16-byte
/// GUIDs: exactly the byte order a BCD device element wants.
pub struct DeviceIds {
    /// The whole-disk GUID (GPT header).
    pub disk_guid: [u8; 16],
    /// The EFI System Partition's unique GUID (bootmgr lives here).
    pub esp_guid: [u8; 16],
    /// The NTFS partition's unique GUID (Windows lives here).
    pub ntfs_guid: [u8; 16],
}

/// Whether the hivex command-line tools are available.
pub fn hivex_available() -> bool {
    tool_ok("hivexsh") && tool_ok("hivexregedit")
}

fn tool_ok(tool: &str) -> bool {
    // `hivexsh --help` / `hivexregedit --help` print usage and exit *non-zero*,
    // so success() is the wrong test; what matters is that the binary was found
    // and executed at all. `status()` only errors (ENOENT) when it isn't on PATH.
    Command::new(tool)
        .arg("--help")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok()
}

/// Build the BCD store at `out_bcd` (typically `<esp>/EFI/Microsoft/Boot/BCD`)
/// for a Windows To Go layout described by `ids`. `friendly` is the boot-menu
/// entry title; `locale` is a tag such as `"en-US"`.
pub fn generate(out_bcd: &Path, ids: &DeviceIds, friendly: &str, locale: &str) -> Result<()> {
    if !hivex_available() {
        bail!(
            "hivexsh / hivexregedit are required to build the Windows To Go \
             boot store; install the `hivex` package"
        );
    }

    // 1. Lay down the blank base hive at the destination.
    std::fs::write(out_bcd, BCD_NEW)
        .with_context(|| format!("writing the base BCD hive to {}", out_bcd.display()))?;

    // 2. Merge the boot-manager + settings skeleton.
    run_stdin(
        "hivexregedit",
        &["--merge", "--prefix", "BCD00000001", &out_bcd.to_string_lossy()],
        WINLOAD_REG.as_bytes(),
        "merging the BCD skeleton",
    )?;

    // 3. Fill the dynamic device/path/order elements.
    let loader = random_guid()?;
    let resume = random_guid()?;
    let script = build_fill_script(ids, &loader, &resume, friendly, locale);
    run_stdin(
        "hivexsh",
        &["-w", &out_bcd.to_string_lossy()],
        script.as_bytes(),
        "writing BCD entries",
    )?;

    emit::log("Generated the Windows To Go BCD boot store");
    Ok(())
}

/// Build the *recovery* BCD store at `out_bcd` (typically
/// `<esp>/EFI/Microsoft/Recovery/BCD`). Windows' boot-file servicing (BFSVC),
/// which runs during OOBE, opens this store unconditionally; a missing store
/// makes it fail with 0xc000000f and OOBE reboot-loops on "Why did my PC
/// restart?". Mirrors BCD-SYS `recovery.sh` (UEFI, create-from-scratch): merge
/// the recovery skeleton, then fill the boot manager's device element (the ESP)
/// and locale. The store has no OS-loader entry; its mere presence is what
/// BFSVC needs.
pub fn generate_recovery(out_bcd: &Path, ids: &DeviceIds, locale: &str) -> Result<()> {
    if !hivex_available() {
        bail!(
            "hivexsh / hivexregedit are required to build the Windows To Go \
             recovery store; install the `hivex` package"
        );
    }

    // 1. Lay down the blank base hive at the destination.
    std::fs::write(out_bcd, BCD_NEW)
        .with_context(|| format!("writing the base recovery hive to {}", out_bcd.display()))?;

    // 2. Merge the recovery skeleton (boot manager + settings objects).
    run_stdin(
        "hivexregedit",
        &["--merge", "--prefix", "BCD00000001", &out_bcd.to_string_lossy()],
        RECOVERY_REG.as_bytes(),
        "merging the recovery BCD skeleton",
    )?;

    // 3. Fill the boot manager's device (the ESP) and locale.
    let esp_dev = hex_line(3, &device_element(&ids.esp_guid, &ids.disk_guid));
    let mut script = String::with_capacity(512);
    set_elem(&mut script, BOOTMGR, "11000001", &esp_dev);
    set_elem(&mut script, BOOTMGR, "12000005", &format!("string:{locale}"));
    script.push_str("commit\n");
    run_stdin(
        "hivexsh",
        &["-w", &out_bcd.to_string_lossy()],
        script.as_bytes(),
        "writing recovery BCD entries",
    )?;

    emit::log("Generated the Windows To Go recovery store");
    Ok(())
}

/// Set `SanPolicy = 4` (keep the host machine's internal disks offline) in the
/// applied image's SYSTEM hive at `system_hive`
/// (`<ntfs>\Windows\System32\config\SYSTEM`). This is the Windows To Go
/// standard, and the Linux-side equivalent of Rufus's
/// `dism /Image:X /Apply-Unattend` step (whose `offlineServicing`
/// `Microsoft-Windows-PartitionManager\SanPolicy=4` ultimately just writes this
/// `partmgr` registry value); usbooty writes it directly since DISM is not
/// available on Linux. Without it the portable OS brings the host's internal
/// Windows disk online during OOBE (the recurring `[setup.exe] System disks
/// found`), which on Windows 11 24H2/25H2 drives `CloudExperienceHost` to reseal
/// OOBE (`IMAGE_STATE_COMPLETE` -> `SPECIALIZE_RESEAL_TO_OOBE`) and reboot-loop.
/// It also protects the host: the running WTG OS will not auto-mount or modify
/// the host's own volumes. `4` = `VDS_SP_OFFLINE_INTERNAL`.
pub fn set_internal_disks_offline(system_hive: &Path) -> Result<()> {
    if !hivex_available() {
        bail!(
            "hivexregedit is required to configure the Windows To Go SYSTEM \
             hive; install the `hivex` package"
        );
    }
    // `hivexregedit --merge` strips `--prefix` from the .reg keys to get
    // hive-relative paths and creates any missing intermediate keys (here
    // `partmgr\Parameters`). `SYSTEM` is a stand-in for the hive root, the same
    // technique as the BCD `--prefix BCD00000001` merges above.
    let reg = "Windows Registry Editor Version 5.00\n\n\
        [SYSTEM\\ControlSet001\\Services\\partmgr\\Parameters]\n\
        \"SanPolicy\"=dword:00000004\n";
    run_stdin(
        "hivexregedit",
        &["--merge", "--prefix", "SYSTEM", &system_hive.to_string_lossy()],
        reg.as_bytes(),
        "setting SanPolicy=4 (offline internal disks) in the SYSTEM hive",
    )?;
    emit::log("Set SanPolicy=4 (host internal disks stay offline) in the image");
    Ok(())
}

/// Merge the subtree of `reg_text` rooted at `root_prefix` into the offline
/// hive `hive`. `reg_text` is standard `.reg` (regedit) content that may mix
/// several roots; only blocks whose key equals or descends from `root_prefix`
/// are applied, with `root_prefix` mapped onto the hive root (the
/// `hivexregedit --prefix` mechanism).
///
/// `hivexregedit --merge` refuses to create a key whose immediate parent is
/// missing, so we first emit an empty block for every *ancestor* key (a
/// `BTreeSet` sorts them lexicographically, which always places a parent before
/// its children) and only then the original value-bearing blocks. Re-declaring
/// a key that already exists merges idempotently, so emitting ancestors that
/// the real hive already has is harmless. This lets a flat `.reg` of deep leaf
/// keys apply cleanly to a fresh image hive.
pub fn merge_reg_subtree(hive: &Path, reg_text: &str, root_prefix: &str) -> Result<()> {
    if !hivex_available() {
        bail!("hivexregedit is required to apply registry policies; install the `hivex` package");
    }
    let prefix_slash = format!("{root_prefix}\\");
    let mut ancestors: BTreeSet<String> = BTreeSet::new();
    let mut body = String::new();
    let mut in_match = false;
    for line in reg_text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            let key = &trimmed[1..trimmed.len() - 1];
            in_match = key == root_prefix || key.starts_with(&prefix_slash);
            if in_match {
                // Every ancestor strictly between root_prefix and this key.
                let mut acc = root_prefix.to_string();
                for seg in key[root_prefix.len()..].split('\\').filter(|s| !s.is_empty()) {
                    acc.push('\\');
                    acc.push_str(seg);
                    if acc != key {
                        ancestors.insert(acc.clone());
                    }
                }
                body.push_str(line);
                body.push('\n');
            }
        } else if in_match {
            body.push_str(line);
            body.push('\n');
        }
    }
    if body.trim().is_empty() {
        return Ok(()); // No keys for this hive in the .reg.
    }
    let mut out = String::from("Windows Registry Editor Version 5.00\n\n");
    for anc in &ancestors {
        out.push_str(&format!("[{anc}]\n\n"));
    }
    out.push_str(&body);
    run_stdin(
        "hivexregedit",
        &["--merge", "--prefix", root_prefix, &hive.to_string_lossy()],
        out.as_bytes(),
        "merging registry policies into the offline hive",
    )
}

/// Build the `hivexsh` command script. Mirrors BCD-SYS `winload.sh` for the
/// UEFI, physical-disk, create-from-scratch case.
fn build_fill_script(
    ids: &DeviceIds,
    loader: &str,
    resume: &str,
    friendly: &str,
    locale: &str,
) -> String {
    let esp_dev = hex_line(3, &device_element(&ids.esp_guid, &ids.disk_guid));
    let ntfs_dev = hex_line(3, &device_element(&ids.ntfs_guid, &ids.disk_guid));
    let flags = hex_line(3, &[0x75, 0x00, 0x00, 0x15, 0x00, 0x00, 0x00, 0x00]);

    let mut s = String::with_capacity(8192);

    // ---- Windows Resume Loader (new object) ------------------------------
    new_object(&mut s, resume, 0x1020_0004);
    add_elem(&mut s, "11000001", &ntfs_dev);
    add_elem(&mut s, "12000002", "string:\\Windows\\system32\\winresume.efi");
    add_elem(&mut s, "12000004", "string:Windows Resume Application");
    add_elem(&mut s, "12000005", &format!("string:{locale}"));
    add_elem(&mut s, "14000006", &hex_line(7, &multi_sz(&[RESUMELOADERSETTINGS])));
    add_elem(&mut s, "16000060", &hex_line(3, &[0x01]));
    add_elem(&mut s, "17000077", &flags);
    add_elem(&mut s, "21000001", &ntfs_dev);
    add_elem(&mut s, "22000002", "string:\\hiberfil.sys");
    add_elem(&mut s, "25000008", &hex_line(3, &[0x01, 0, 0, 0, 0, 0, 0, 0]));

    // ---- Windows Boot Loader (new object) --------------------------------
    new_object(&mut s, loader, 0x1020_0003);
    add_elem(&mut s, "11000001", &ntfs_dev);
    add_elem(&mut s, "12000002", "string:\\Windows\\system32\\winload.efi");
    add_elem(&mut s, "12000004", &format!("string:{friendly}"));
    add_elem(&mut s, "12000005", &format!("string:{locale}"));
    add_elem(&mut s, "14000006", &hex_line(7, &multi_sz(&[BOOTLOADERSETTINGS])));
    add_elem(&mut s, "16000060", &hex_line(3, &[0x01]));
    // recoveryenabled = No. Without it, BFSVC (boot-file servicing, run during
    // OOBE) tries to open the recovery store on the ESP, which we never create,
    // fails with 0x2 (FILE_NOT_FOUND), and OOBE loops on "Why did my PC restart?".
    // Mirrors Rufus's `bcdedit /set {default} recoveryenabled no` for WTG.
    add_elem(&mut s, "16000054", &hex_line(3, &[0x00]));
    add_elem(&mut s, "17000077", &flags);
    add_elem(&mut s, "21000001", &ntfs_dev);
    add_elem(&mut s, "22000002", "string:\\Windows");
    add_elem(&mut s, "23000003", &format!("string:{resume}"));
    add_elem(&mut s, "25000020", &hex_line(3, &[0, 0, 0, 0, 0, 0, 0, 0]));
    add_elem(&mut s, "250000c2", &hex_line(3, &[0x01, 0, 0, 0, 0, 0, 0, 0]));

    // ---- Windows Boot Manager (pre-existing keys from the skeleton) ------
    set_elem(&mut s, BOOTMGR, "11000001", &esp_dev);
    set_elem(&mut s, BOOTMGR, "12000002", "string:\\EFI\\Microsoft\\Boot\\bootmgfw.efi");
    set_elem(&mut s, BOOTMGR, "12000005", &format!("string:{locale}"));
    set_elem(&mut s, BOOTMGR, "23000003", &format!("string:{loader}"));
    set_elem(&mut s, BOOTMGR, "23000006", &format!("string:{resume}"));
    set_elem(&mut s, BOOTMGR, "24000001", &hex_line(7, &multi_sz(&[loader])));

    // ---- Windows Memory Tester (pre-existing keys) -----------------------
    set_elem(&mut s, MEMDIAG, "11000001", &esp_dev);
    set_elem(&mut s, MEMDIAG, "12000002", "string:\\EFI\\Microsoft\\Boot\\memtest.efi");
    set_elem(&mut s, MEMDIAG, "12000005", &format!("string:{locale}"));

    // ---- Mark the hive as a system store ---------------------------------
    s.push_str("cd \\Description\n");
    s.push_str("setval 2\n");
    s.push_str("KeyName\n");
    s.push_str("string:BCD00000000\n");
    s.push_str("System\n");
    s.push_str("dword:1\n");

    s.push_str("commit\n");
    s
}

/// Create a new object `{guid}` under `\Objects` with the given `Description\Type`,
/// leaving the script positioned at its `Elements` key for [`add_elem`].
fn new_object(s: &mut String, guid: &str, type_dword: u32) {
    s.push_str("cd \\Objects\n");
    s.push_str(&format!("add {guid}\n"));
    s.push_str(&format!("cd {guid}\n"));
    s.push_str("add Description\n");
    s.push_str("cd Description\n");
    s.push_str("setval 1\n");
    s.push_str("Type\n");
    s.push_str(&format!("dword:0x{type_dword:08x}\n"));
    s.push_str("cd ..\n");
    s.push_str("add Elements\n");
    s.push_str("cd Elements\n");
}

/// Add a new element key under the current (newly-created) object's `Elements`,
/// set its `Element` value, and return to `Elements`.
fn add_elem(s: &mut String, element: &str, value_line: &str) {
    s.push_str(&format!("add {element}\n"));
    s.push_str(&format!("cd {element}\n"));
    s.push_str("setval 1\n");
    s.push_str("Element\n");
    s.push_str(value_line);
    s.push('\n');
    s.push_str("cd ..\n");
}

/// Set the `Element` value of a pre-existing element key (absolute path), used
/// for the boot-manager / memory-tester keys the skeleton already created.
fn set_elem(s: &mut String, guid: &str, element: &str, value_line: &str) {
    s.push_str(&format!("cd \\Objects\\{guid}\\Elements\\{element}\n"));
    s.push_str("setval 1\n");
    s.push_str("Element\n");
    s.push_str(value_line);
    s.push('\n');
}

/// Build a BCD "qualified partition" device element (GPT): 88 bytes that
/// identify a partition by its partition GUID plus the disk GUID. Byte layout
/// reproduced from BCD-SYS `update_device.sh` (GPT branch).
fn device_element(part_guid: &[u8; 16], disk_guid: &[u8; 16]) -> Vec<u8> {
    let mut v = Vec::with_capacity(88);
    v.extend_from_slice(&[0u8; 16]); // 0x00: reserved
    v.extend_from_slice(&6u32.to_le_bytes()); // 0x10: type
    v.extend_from_slice(&0u32.to_le_bytes()); // 0x14
    v.extend_from_slice(&0x48u64.to_le_bytes()); // 0x18: sub-element size
    v.extend_from_slice(part_guid); // 0x20: partition GUID
    v.extend_from_slice(&0u32.to_le_bytes()); // 0x30
    v.extend_from_slice(&0u32.to_le_bytes()); // 0x34: scheme (0 = GPT)
    v.extend_from_slice(disk_guid); // 0x38: disk GUID
    v.extend_from_slice(&[0u8; 16]); // 0x48: trailing reserved
    v
}

/// A `hivexsh` value line: `hex:<reg_type>:aa,bb,cc`.
fn hex_line(reg_type: u8, bytes: &[u8]) -> String {
    let body = bytes
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(",");
    format!("hex:{reg_type}:{body}")
}

/// REG_MULTI_SZ bytes: each GUID string as UTF-16LE + NUL, then a final NUL.
fn multi_sz(items: &[&str]) -> Vec<u8> {
    let mut v = Vec::new();
    for item in items {
        v.extend(item.encode_utf16().flat_map(u16::to_le_bytes));
        v.extend_from_slice(&[0, 0]);
    }
    v.extend_from_slice(&[0, 0]);
    v
}

/// A fresh lowercase GUID string in braces, from 16 random bytes (RFC-4122 v4).
fn random_guid() -> Result<String> {
    let mut b = [0u8; 16];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut b))
        .context("reading /dev/urandom for a BCD GUID")?;
    b[6] = (b[6] & 0x0f) | 0x40; // version 4
    b[8] = (b[8] & 0x3f) | 0x80; // variant
    Ok(format!(
        "{{{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}}}",
        b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7], b[8], b[9], b[10], b[11], b[12], b[13],
        b[14], b[15]
    ))
}

/// Run `tool args...` feeding `input` on stdin, surfacing stderr on failure.
fn run_stdin(tool: &str, args: &[&str], input: &[u8], doing: &str) -> Result<()> {
    let mut child = Command::new(tool)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("spawning {tool}; is the `hivex` package installed?"))?;
    child
        .stdin
        .take()
        .context("stdin pipe")?
        .write_all(input)
        .with_context(|| format!("feeding {tool}"))?;
    let out = child
        .wait_with_output()
        .with_context(|| format!("waiting for {tool}"))?;
    if !out.status.success() {
        bail!("{doing} failed: {}", String::from_utf8_lossy(&out.stderr).trim());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_element_is_88_bytes_with_guids_placed() {
        let part = [0x11u8; 16];
        let disk = [0x22u8; 16];
        let d = device_element(&part, &disk);
        assert_eq!(d.len(), 88);
        assert_eq!(&d[0x10..0x14], &6u32.to_le_bytes()); // type
        assert_eq!(d[0x18], 0x48); // sub-element size
        assert_eq!(&d[0x20..0x30], &part); // partition GUID
        assert_eq!(d[0x34], 0x00); // GPT scheme
        assert_eq!(&d[0x38..0x48], &disk); // disk GUID
    }

    #[test]
    fn multi_sz_double_null_terminated() {
        let m = multi_sz(&["{abc}"]);
        assert_eq!(&m[m.len() - 4..], &[0, 0, 0, 0]);
    }

    #[test]
    fn hex_line_format() {
        assert_eq!(hex_line(3, &[0x1e, 0x00]), "hex:3:1e,00");
        assert_eq!(hex_line(7, &[]), "hex:7:");
    }

    #[test]
    fn random_guid_shape() {
        let g = random_guid().unwrap();
        assert_eq!(g.len(), 38); // {8-4-4-4-12}
        assert!(g.starts_with('{') && g.ends_with('}'));
        assert_eq!(g.chars().filter(|&c| c == '-').count(), 4);
        assert_eq!(&g[15..16], "4"); // version nibble
    }

    #[test]
    fn generate_runs_against_real_hivex() {
        if !hivex_available() {
            eprintln!("skipping: hivex tools not installed");
            return;
        }
        let dir = std::env::temp_dir().join(format!("usbooty-bcdtest-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let out = dir.join("BCD");
        let ids = DeviceIds {
            disk_guid: [0xAA; 16],
            esp_guid: [0xBB; 16],
            ntfs_guid: [0xCC; 16],
        };
        generate(&out, &ids, "Windows To Go", "en-US").expect("BCD generate should succeed");

        // bootmgr default (23000003) should now name a loader GUID.
        let got = std::process::Command::new("hivexget")
            .arg(&out)
            .arg("\\Objects\\{9dea862c-5cdd-4e70-acc1-f32b344d4795}\\Elements\\23000003")
            .arg("Element")
            .output()
            .unwrap();
        assert!(got.status.success(), "hivexget failed");
        assert!(String::from_utf8_lossy(&got.stdout).contains('{'));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn generate_recovery_runs_against_real_hivex() {
        if !hivex_available() {
            eprintln!("skipping: hivex tools not installed");
            return;
        }
        let dir = std::env::temp_dir().join(format!("usbooty-rectest-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let out = dir.join("BCD");
        let ids = DeviceIds {
            disk_guid: [0xAA; 16],
            esp_guid: [0xBB; 16],
            ntfs_guid: [0xCC; 16],
        };
        generate_recovery(&out, &ids, "en-US").expect("recovery store should generate");

        // The boot-manager device element (11000001) should hold the 88-byte
        // ESP descriptor we filled in.
        let got = std::process::Command::new("hivexget")
            .arg(&out)
            .arg(format!("\\Objects\\{BOOTMGR}\\Elements\\11000001"))
            .arg("Element")
            .output()
            .unwrap();
        assert!(got.status.success(), "hivexget failed on recovery store");
        assert_eq!(got.stdout.len(), 88, "recovery device element should be 88 bytes");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn fill_script_creates_loader_and_fills_bootmgr() {
        let ids = DeviceIds {
            disk_guid: [1; 16],
            esp_guid: [2; 16],
            ntfs_guid: [3; 16],
        };
        let s = build_fill_script(&ids, "{ldr}", "{res}", "Windows To Go", "en-US");
        assert!(s.contains("add {ldr}"));
        assert!(s.contains("add {res}"));
        assert!(s.contains(&format!("cd \\Objects\\{BOOTMGR}\\Elements\\23000003")));
        assert!(s.contains("string:{ldr}")); // bootmgr default → loader
        assert!(s.contains("\\Windows\\system32\\winload.efi"));
        // recoveryenabled = No, so OOBE's BFSVC pass doesn't fail on the
        // missing recovery store (the "Why did my PC restart?" loop).
        assert!(s.contains("add 16000054"));
        assert!(s.trim_end().ends_with("commit"));
    }

    #[test]
    fn set_internal_disks_offline_writes_sanpolicy_4() {
        if !hivex_available() {
            eprintln!("skipping: hivex tools not installed");
            return;
        }
        let dir = std::env::temp_dir().join(format!("usbooty-santest-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let hive = dir.join("SYSTEM");
        std::fs::write(&hive, BCD_NEW).unwrap();

        // A blank hive lacks the Services tree; recreate the `partmgr\Parameters`
        // chain that exists in every real SYSTEM hive so the merge has a parent
        // (hivexregedit only creates a key whose immediate parent exists).
        let seed = "add ControlSet001\ncd ControlSet001\nadd Services\ncd Services\n\
                    add partmgr\ncd partmgr\nadd Parameters\ncommit\n";
        run_stdin(
            "hivexsh",
            &["-w", &hive.to_string_lossy()],
            seed.as_bytes(),
            "seeding partmgr key",
        )
        .expect("seeding the key chain should succeed");

        set_internal_disks_offline(&hive).expect("SanPolicy merge should succeed");

        let got = std::process::Command::new("hivexget")
            .arg(&hive)
            .arg("\\ControlSet001\\Services\\partmgr\\Parameters")
            .arg("SanPolicy")
            .output()
            .unwrap();
        assert!(got.status.success(), "hivexget failed reading SanPolicy");
        // hivexget prints a dword as `0x4`.
        assert!(
            String::from_utf8_lossy(&got.stdout).trim().contains('4'),
            "SanPolicy should be 4"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
