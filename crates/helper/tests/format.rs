//! `Job::Format` end-to-end: partition a loop device, format it, and confirm
//! the kernel sees the resulting filesystem.

mod common;

use common::{finished_ok, is_root, run_helper, LoopDevice};

#[test]
#[ignore = "needs root (losetup, mkfs, mount)"]
fn formats_a_loop_device_as_fat32() {
    require_root!();

    // 128 MiB is small enough for the test to finish in <1 s and large enough
    // for mkfs.fat to accept it as FAT32.
    let loop_dev = LoopDevice::new(128 * 1024 * 1024);
    let job = serde_json::json!({
        "kind": "format",
        "device_path": loop_dev.path(),
        "table": "mbr",
        "filesystem": "fat32",
        "opts": { "label": "USBT" }
    });
    let (out, status) = run_helper(&job.to_string());

    assert!(status.success(), "helper exited non-zero. stdout was:\n{out}");
    assert!(finished_ok(&out), "no Done message in helper stdout:\n{out}");

    // Probe with `blkid` to confirm a FAT32 filesystem now lives on the first
    // partition. blkid is part of util-linux and always available where
    // losetup is.
    let partition = format!("{}p1", loop_dev.path().display());
    let blkid = std::process::Command::new("blkid")
        .arg(&partition)
        .output()
        .expect("running blkid");
    let info = String::from_utf8_lossy(&blkid.stdout);
    assert!(
        info.contains("vfat") || info.contains("FAT32"),
        "expected a FAT32 filesystem on {partition}, blkid said: {info}"
    );
    assert!(
        info.contains("USBT"),
        "expected the volume label `USBT` on {partition}, blkid said: {info}"
    );
}
