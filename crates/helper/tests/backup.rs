//! `Job::Dd` followed by `Job::Backup` round-trip: write a known pattern onto
//! a loop device with DD, snapshot it via Backup, and confirm the snapshot
//! reproduces the source byte-for-byte.

mod common;

use common::{LoopDevice, finished_ok, is_root, run_helper};

#[test]
#[ignore = "needs root (losetup)"]
fn dd_then_backup_reproduces_the_source_image() {
    require_root!();

    // 16 MiB is enough to exercise the streaming loops and small enough to
    // stay under one second of disk activity.
    const SIZE: u64 = 16 * 1024 * 1024;

    // Fabricate a "source ISO" of pseudo-random bytes so an accidental
    // all-zero pass would not look like success.
    let src = std::env::temp_dir().join(format!("usbooty-it-src-{}.img", std::process::id()));
    let pattern: Vec<u8> = (0..SIZE).map(|i| (i * 31 + 7) as u8).collect();
    std::fs::write(&src, &pattern).expect("seeding the source image");

    let loop_dev = LoopDevice::new(SIZE * 2);

    // 1. DD the source onto the loop device.
    let dd = serde_json::json!({
        "kind": "dd",
        "iso_path": src,
        "device_path": loop_dev.path(),
        "opts": { "verify": true }
    });
    let (out, status) = run_helper(&dd.to_string());
    assert!(status.success(), "dd failed. helper stdout:\n{out}");
    assert!(finished_ok(&out), "dd did not emit Done. stdout:\n{out}");

    // 2. Backup the loop device into a fresh image file.
    let snap = std::env::temp_dir().join(format!("usbooty-it-snap-{}.img", std::process::id()));
    let backup = serde_json::json!({
        "kind": "backup",
        "device_path": loop_dev.path(),
        "image_path": snap,
        "opts": { "verify": true }
    });
    let (out, status) = run_helper(&backup.to_string());
    assert!(status.success(), "backup failed. helper stdout:\n{out}");
    assert!(
        finished_ok(&out),
        "backup did not emit Done. stdout:\n{out}"
    );

    // 3. The first `SIZE` bytes of the snapshot must match the original
    //    pattern. (The loop device is larger; only the front section was
    //    written, the rest is whatever the sparse backing held, which is
    //    why we compare only the leading `SIZE` bytes.)
    let snap_bytes = std::fs::read(&snap).expect("reading the snapshot");
    assert!(snap_bytes.len() >= SIZE as usize);
    assert_eq!(
        &snap_bytes[..SIZE as usize],
        &pattern[..],
        "the snapshot does not match the DD source"
    );

    // Clean up artefacts. The loop device dies with `loop_dev` on drop.
    let _ = std::fs::remove_file(&src);
    let _ = std::fs::remove_file(&snap);
}
