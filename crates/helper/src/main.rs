//! `usbooty-helper` — the privileged worker.
//!
//! Invoked via `pkexec` by the unprivileged GUI. It reads a single
//! [`Job`](usbooty_core::Job) as a JSON line on stdin, performs every
//! privileged operation, and streams [`ProgressMsg`](usbooty_core::ProgressMsg)
//! JSON lines on stdout. Exit code 0 means success; 1 means failure (with a
//! preceding `Error` message).
//!
//! ## Cancellation
//!
//! stdin stays open for the lifetime of the job. A watcher thread keeps reading
//! it: the line `cancel` — or EOF, which happens when the GUI process dies —
//! sets the global [`ABORT`] flag. This is the only cross-process control
//! channel that works, since the unprivileged GUI cannot signal this root
//! process directly.
//!
//! This binary contains no GUI and no networking — it is small and auditable
//! on purpose, since it is the only component that runs as root.

mod blockdev;
mod dd;
mod emit;
mod format;
mod fsutil;
mod isocopy;
mod label;
mod partition;
mod partitioned;
mod persistence;
mod uefi_ntfs;
mod unattend;
mod ventoy;

use std::fs::File;
use std::io::{BufRead, BufReader};
use std::os::fd::FromRawFd;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};

use anyhow::{Context, Result};
use nix::sys::signal::{self, SaFlags, SigAction, SigHandler, SigSet, Signal};
use usbooty_core::{Job, ProgressMsg};

/// Set when the job should abort: by a SIGTERM/SIGINT, a `cancel` line on
/// stdin, or stdin reaching EOF (the GUI went away).
static ABORT: AtomicBool = AtomicBool::new(false);

/// Async-signal-safe handler: just records that an abort was requested.
extern "C" fn on_terminate(_signum: i32) {
    ABORT.store(true, Ordering::SeqCst);
}

/// Install a clean-abort handler for SIGTERM and SIGINT.
fn install_signal_handlers() {
    let action = SigAction::new(
        SigHandler::Handler(on_terminate),
        SaFlags::empty(),
        SigSet::empty(),
    );
    // SAFETY: the handler performs only an atomic store, which is
    // async-signal-safe.
    unsafe {
        let _ = signal::sigaction(Signal::SIGTERM, &action);
        let _ = signal::sigaction(Signal::SIGINT, &action);
    }
}

/// Read stdin for the lifetime of the process: first line is the job, any
/// later line (or EOF) requests cancellation.
fn stdin_watcher(tx: Sender<Job>) {
    // SAFETY: fd 0 is this process's stdin; we own it for the process lifetime.
    let stdin = unsafe { File::from_raw_fd(0) };
    let reader = BufReader::new(stdin);
    let mut got_job = false;
    for line in reader.lines() {
        let Ok(line) = line else { break };
        if !got_job {
            match serde_json::from_str::<Job>(line.trim()) {
                Ok(job) => {
                    got_job = true;
                    let _ = tx.send(job);
                }
                Err(_) => break, // malformed job — drop tx, main thread errors out
            }
        } else if line.trim() == "cancel" {
            ABORT.store(true, Ordering::SeqCst);
        }
    }
    // stdin closed or errored — treat as an abort request.
    ABORT.store(true, Ordering::SeqCst);
}

fn main() {
    install_signal_handlers();
    let code = match run() {
        Ok(()) => {
            emit::emit(&ProgressMsg::Done);
            0
        }
        Err(e) => {
            emit::error(format!("{e:#}"));
            1
        }
    };
    std::process::exit(code);
}

/// Receive the job from the stdin watcher and dispatch it.
fn run() -> Result<()> {
    let (tx, rx) = mpsc::channel::<Job>();
    std::thread::spawn(move || stdin_watcher(tx));

    let job = rx
        .recv()
        .context("no valid job description received on stdin")?;

    emit::log(format!(
        "usbooty-helper {} starting",
        env!("CARGO_PKG_VERSION")
    ));
    emit::log(describe_job(&job));

    match job {
        Job::Dd {
            iso_path,
            device_path,
            opts,
        } => dd::run(&iso_path, &device_path, &opts, &ABORT),
        Job::Partitioned {
            iso_path,
            device_path,
            table,
            filesystem,
            wim,
            uefi_ntfs_img,
            persistence,
            windows_setup,
            opts,
        } => partitioned::run(
            &iso_path,
            &device_path,
            table,
            filesystem,
            wim,
            uefi_ntfs_img.as_deref(),
            persistence,
            windows_setup.as_ref(),
            &opts,
            &ABORT,
        ),
        Job::Format {
            device_path,
            table,
            filesystem,
            opts,
        } => format::run(&device_path, table, filesystem, &opts, &ABORT),
        Job::Ventoy {
            device_path,
            table,
            secure_boot,
            update,
            iso_path,
        } => ventoy::run(
            &device_path,
            table,
            secure_boot,
            update,
            iso_path.as_deref(),
            &ABORT,
        ),
    }
}

/// A one-line summary of a job, logged when the helper starts.
fn describe_job(job: &Job) -> String {
    match job {
        Job::Dd { device_path, .. } => {
            format!("Job: raw image write → {}", device_path.display())
        }
        Job::Partitioned {
            device_path,
            filesystem,
            table,
            ..
        } => format!(
            "Job: partition & copy → {} ({}, {})",
            device_path.display(),
            filesystem.label(),
            table.label()
        ),
        Job::Format {
            device_path,
            filesystem,
            ..
        } => format!(
            "Job: format {} as {}",
            device_path.display(),
            filesystem.label()
        ),
        Job::Ventoy {
            device_path, update, ..
        } => format!(
            "Job: {} Ventoy → {}",
            if *update { "update" } else { "install" },
            device_path.display()
        ),
    }
}
