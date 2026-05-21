//! Shared test plumbing: a loop device wrapped in RAII, plus a helper for
//! spawning `usbooty-helper` against it and collecting its progress stream.

#![allow(dead_code)]

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Skip the calling test unless this process is running as root. Losetup,
/// mount, mkfs, and the helper itself all need privileges; running these
/// without root is meaningless rather than failing fast.
#[macro_export]
macro_rules! require_root {
    () => {
        if !is_root() {
            eprintln!("skipping: this integration test needs root");
            return;
        }
    };
}

/// Whether the current process can perform privileged actions (uid 0). Hidden
/// behind a function so each test file can `use` it without unsafe blocks.
pub fn is_root() -> bool {
    nix::unistd::Uid::effective().is_root()
}

/// A sparse-backed loop device, cleaned up on drop.
pub struct LoopDevice {
    backing: PathBuf,
    device: PathBuf,
}

impl LoopDevice {
    /// Create a sparse file of `size_bytes` and attach a `/dev/loopN` to it.
    pub fn new(size_bytes: u64) -> Self {
        let pid = std::process::id();
        let cnt = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let backing = std::env::temp_dir().join(format!("usbooty-it-{pid}-{cnt}.img"));
        // Sparse file: seek to size-1 and write one byte.
        let f = std::fs::File::create(&backing).expect("create backing file");
        f.set_len(size_bytes).expect("set sparse size");
        drop(f);

        let out = Command::new("losetup")
            .args(["--find", "--show"])
            .arg(&backing)
            .output()
            .expect("losetup invocation");
        if !out.status.success() {
            let _ = std::fs::remove_file(&backing);
            panic!(
                "losetup failed: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }
        let device = PathBuf::from(
            String::from_utf8_lossy(&out.stdout).trim().to_string(),
        );
        LoopDevice { backing, device }
    }

    pub fn path(&self) -> &Path {
        &self.device
    }

    pub fn backing(&self) -> &Path {
        &self.backing
    }
}

impl Drop for LoopDevice {
    fn drop(&mut self) {
        // Best effort: detach the loop, then unlink the sparse file.
        let _ = Command::new("losetup")
            .arg("-d")
            .arg(&self.device)
            .status();
        let _ = std::fs::remove_file(&self.backing);
    }
}

static COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// The compiled `usbooty-helper` binary, located via the cargo env var so the
/// tests follow the same target dir as the rest of the build.
pub fn helper_binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_usbooty-helper"))
}

/// Run the helper with `job_json` on stdin. Returns the captured stdout (each
/// line is one ProgressMsg) and the exit status. Stderr is inherited so any
/// helper-internal panic surfaces in the test output.
pub fn run_helper(job_json: &str) -> (String, std::process::ExitStatus) {
    let mut child = Command::new(helper_binary())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("spawning the helper");
    {
        let stdin = child.stdin.as_mut().expect("helper stdin");
        writeln!(stdin, "{job_json}").expect("writing job to helper stdin");
    }
    // Drain stdout off-thread, so a chatty helper cannot deadlock by filling
    // its pipe before the test reads it.
    let stdout = child.stdout.take().expect("helper stdout");
    let reader = std::thread::spawn(move || {
        let mut out = String::new();
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            out.push_str(&line);
            out.push('\n');
        }
        out
    });
    let status = child.wait().expect("waiting for helper");
    let out = reader.join().expect("joining reader thread");
    (out, status)
}

/// Was a `ProgressMsg::Done` line present in the helper's output?
pub fn finished_ok(stdout: &str) -> bool {
    stdout
        .lines()
        .any(|line| line.trim() == "{\"type\":\"done\"}")
}
