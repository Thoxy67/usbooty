# Developing

## Repo layout

```
crates/
  core/        Shared types and pure planning logic. No Qt, no network, no root.
  gui/         The Qt 6 / QML application. Binary: `usbooty`.
  helper/      The privileged CLI worker. Binary: `usbooty-helper`.
data/          Desktop file, AppStream metadata, icons, polkit policy.
packaging/     PKGBUILD and AUR publish helper.
tests/         loop-test.sh (hardware-free end-to-end driver).
docs/          This documentation.
install.sh     Top-level installer (assumes `cargo build --release` has run).
```

## Build for development

```sh
cargo build
cargo run -p usbooty-gui
```

`cargo run` starts the GUI from the dev tree. The icon will not appear in
the Wayland titlebar from a dev build, because Wayland looks up icons via
installed `.desktop` files. To get the icon during dev, install the desktop
file and icon into your user data dir once:

```sh
install -Dm644 data/org.usbooty.Usbooty.desktop \
    ~/.local/share/applications/org.usbooty.Usbooty.desktop
install -Dm644 data/icons/org.usbooty.Usbooty.svg \
    ~/.local/share/icons/hicolor/scalable/apps/org.usbooty.Usbooty.svg
update-desktop-database ~/.local/share/applications 2>/dev/null || true
```

## Tests

The hardware-free suite runs entirely on in-memory buffers:

```sh
cargo test
```

It exercises:

* Partition table writing (GPT and MBR) against in-memory `Cursor<Vec<u8>>`.
* ISO classification.
* `autounattend.xml` generation (every option block, every architecture).
* Persistence config rewriting (casper and Debian).
* FAT volume label sanitisation.
* JSON roundtrip of every `Job` variant.

For the full write paths, a loopback driver creates a sparse file, attaches
it as a loop device, and runs the helper end-to-end. It needs root because
the helper opens block devices:

```sh
sudo ./tests/loop-test.sh
```

The loop-test does not touch any real hardware. It always allocates a fresh
sparse image, attaches it as `/dev/loop<N>`, and detaches it on exit. You
cannot accidentally wipe a USB stick with it.

## Code style

* No unwraps in non-test code. Use `anyhow::Result` and `with_context` for
  context-rich errors. The helper's bail behaviour is what the user sees in
  the GUI log, so context matters.
* No comments that restate what the code does. Comments explain why,
  especially when a choice is non-obvious (a workaround, a spec quirk, an
  incident postmortem).
* The privilege boundary is sacred: nothing in `usbooty-helper` reaches
  outwards to the network or to a GUI library; nothing in `usbooty-gui`
  opens a block device.

## Adding a new write method

1. Add a variant to `Job` in `crates/core/src/job.rs` with the fully
   resolved parameters it needs.
2. Add a JSON roundtrip test in the same file.
3. In `crates/helper/src/main.rs`, dispatch on the new variant and call into
   a new module that does the work. Reuse `blockdev`, `partition`, `fsutil`,
   and `emit` rather than rolling your own.
4. In `crates/gui/src/runner.rs`, build the new `Job` variant from the QML
   state. Add any new properties to `crates/gui/src/bridge.rs`.
5. In `crates/gui/qml/main.qml`, surface the new option in the Options card.

## Packaging

The AUR PKGBUILD lives at `packaging/PKGBUILD`. To publish:

* Push your source changes to the upstream git repo.
* In your local AUR clone (a separate git repo), update the PKGBUILD if any
  packaging-relevant lines changed (deps, options, install layout).
* `makepkg -Cfsi` to test locally.
* `makepkg --printsrcinfo > .SRCINFO`, then commit and push to AUR.

`packaging/publish-aur.sh` automates the AUR side if you have set it up.
