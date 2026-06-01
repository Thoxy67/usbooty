//! Ventoy USB creation: drive the Ventoy CLI to install or update Ventoy on a
//! device, then optionally copy an ISO onto its exFAT data partition.
//!
//! Ventoy itself does all the bootloader work; the helper just orchestrates
//! its CLI (`ventoy` / `Ventoy2Disk.sh`) and copies the ISO file afterwards.

use anyhow::{Context, Result, bail};
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use usbooty_core::PartitionTable;

use crate::{blockdev, emit, fsutil};

/// Install or update Ventoy on `device`, then seed it with `iso_path` if given.
pub fn run(
    device: &Path,
    table: PartitionTable,
    secure_boot: bool,
    update: bool,
    iso_path: Option<&Path>,
    abort: &AtomicBool,
) -> Result<()> {
    // Ventoy needs the target's partitions unmounted before it touches them.
    let _ = blockdev::unmount_all(device);

    emit::phase(if update {
        "Updating Ventoy"
    } else {
        "Installing Ventoy"
    });
    run_cli(device, table, secure_boot, update)?;

    if let Some(iso) = iso_path {
        seed_iso(device, iso, abort)?;
    }
    emit::log("Ventoy USB is ready");
    Ok(())
}

/// Run the Ventoy CLI, feeding it confirmation and streaming its output.
///
/// The exFAT data partition keeps Ventoy's default `Ventoy` label; `-L` is
/// not passed, since an image's own label is often too long for exFAT.
fn run_cli(device: &Path, table: PartitionTable, secure_boot: bool, update: bool) -> Result<()> {
    let dev = device.to_string_lossy().into_owned();

    let mut args: Vec<String> = Vec::new();
    if update {
        args.push("-u".into());
    } else {
        // Force install: usbooty already showed an erase-confirm dialog.
        args.push("-I".into());
        if table == PartitionTable::Gpt {
            args.push("-g".into());
        }
        args.push(if secure_boot { "-s" } else { "-S" }.into());
    }
    args.push(dev);

    emit::log(format!("Running ventoy {}", args.join(" ")));
    let mut child = Command::new("ventoy")
        .args(&args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("could not run the Ventoy CLI. Is the `ventoy` package installed?")?;

    // Ventoy2Disk.sh prompts for confirmation (sometimes twice); answer it.
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(b"y\ny\ny\n");
        let _ = stdin.flush();
    }
    if let Some(stdout) = child.stdout.take() {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if !line.trim().is_empty() {
                emit::log(line);
            }
        }
    }

    let status = child.wait().context("waiting for the Ventoy CLI")?;
    if !status.success() {
        let mut stderr = String::new();
        if let Some(mut s) = child.stderr.take() {
            let _ = s.read_to_string(&mut stderr);
        }
        let stderr = stderr.trim();
        if stderr.is_empty() {
            bail!("the Ventoy CLI failed ({status})");
        }
        bail!("the Ventoy CLI failed: {stderr}");
    }
    Ok(())
}

/// Copy `iso` onto Ventoy's exFAT data partition (partition 1).
fn seed_iso(device: &Path, iso: &Path, abort: &AtomicBool) -> Result<()> {
    let part = blockdev::partition_path(device, 1);
    fsutil::wait_for_node(&part)?;

    emit::phase("Copying ISO");
    let mount = fsutil::Mount::new(&part, "exfat")?;
    let name = iso
        .file_name()
        .map(|n| n.to_owned())
        .unwrap_or_else(|| "image.iso".into());
    let dest = mount.path().join(name);

    let mut src = File::open(iso).with_context(|| format!("opening {}", iso.display()))?;
    let total = src.metadata().map(|m| m.len()).unwrap_or(0);
    let mut out = File::create(&dest).with_context(|| format!("creating {}", dest.display()))?;
    emit::log(format!(
        "Copying {} ({}) onto the Ventoy partition",
        dest.file_name().unwrap_or_default().to_string_lossy(),
        usbooty_core::device::format_size(total)
    ));

    let mut buf = vec![0u8; fsutil::COPY_BUF];
    let mut done = 0u64;
    let mut last = Instant::now();
    loop {
        if abort.load(Ordering::SeqCst) {
            bail!("aborted by user");
        }
        let n = src.read(&mut buf).context("reading the ISO")?;
        if n == 0 {
            break;
        }
        out.write_all(&buf[..n])
            .context("writing to the Ventoy partition")?;
        done += n as u64;
        if last.elapsed() >= Duration::from_millis(100) {
            emit::progress("Copying ISO", done, total.max(done));
            last = Instant::now();
        }
    }
    emit::progress("Copying ISO", done, total.max(done));
    emit::phase("Flushing");
    // `mount` drops here: sync + unmount.
    Ok(())
}
