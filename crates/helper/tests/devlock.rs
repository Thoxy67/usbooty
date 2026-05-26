//! Two concurrent helpers targeting the same device must not both run: the
//! second one should bail with the lockfile error.

mod common;

use common::{LoopDevice, is_root, run_helper};
use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

#[test]
#[ignore = "needs root (losetup)"]
fn second_helper_on_the_same_device_is_rejected() {
    require_root!();

    let loop_dev = LoopDevice::new(128 * 1024 * 1024);
    // A Format job that is slow enough to overlap with a second helper but
    // does not need a real ISO. `full_format` zeroes the whole device first,
    // which buys us a 128 MiB window during which the lock is held.
    let job = serde_json::json!({
        "kind": "format",
        "device_path": loop_dev.path(),
        "table": "mbr",
        "filesystem": "fat32",
        "opts": { "label": "LOCKED", "full_format": true }
    });
    let job_str = job.to_string();

    // Launch the first helper in the background and let it start its work.
    let first_helper = common::helper_binary();
    let mut first = Command::new(&first_helper)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawning the first helper");
    writeln!(first.stdin.as_mut().unwrap(), "{job_str}").expect("feeding job 1");
    // Give the first helper a moment to acquire the lockfile.
    std::thread::sleep(std::time::Duration::from_millis(250));

    // The second helper must fail fast with the lockfile message.
    let captured = Arc::new(Mutex::new(String::new()));
    let captured_for_run = Arc::clone(&captured);
    let job_for_run = job_str.clone();
    let handle = std::thread::spawn(move || {
        let (out, status) = run_helper(&job_for_run);
        *captured_for_run.lock().unwrap() = out;
        status
    });
    let second_status = handle.join().expect("joining second helper");
    let second_out = captured.lock().unwrap().clone();

    assert!(
        !second_status.success(),
        "second helper unexpectedly succeeded against a locked device: {second_out}"
    );
    assert!(
        second_out.contains("already running"),
        "expected the lockfile error in stderr or stdout, got:\n{second_out}"
    );

    // Let the first helper finish, otherwise the loop device drop races it.
    let _ = first.wait();
}
