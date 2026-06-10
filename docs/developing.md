# Developing

## Repo layout

```
crates/
  core/        Shared types and pure planning logic. No Qt, no network, no root.
  gui/         The Qt 6 / QML application. Binary: `usbooty`.
  helper/      The privileged CLI worker. Binary: `usbooty-helper`.
data/          Desktop file, AppStream metadata, icons, polkit policy,
               French translations (`translations/usbooty_fr.ts` /
               `.qm`).
packaging/     PKGBUILD and AUR publish helper.
tests/         loop-test.sh (hardware-free end-to-end driver) plus
               decompress-test.sh.
docs/          This documentation.
install.sh     Top-level installer (assumes `cargo build --release`
               has run).
```

The helper crate also ships static asset directories that are baked
into the binary at compile time via `include_str!`:

```
crates/helper/src/
  debloat.reg              The Group-Policy + Default-user debloat
                           profile imported by the `unattend` module.
  desktop_helpers/         The twenty-six `.bat` post-install helpers
                           plus a README, dropped on the new user's
                           Desktop when the matching checkbox is on.
```

## Build for development

```sh
cargo build
cargo run -p usbooty-gui
```

`cargo run` starts the GUI from the dev tree. The icon will not
appear in the Wayland titlebar from a dev build, because Wayland
looks up icons via installed `.desktop` files. To get the icon
during dev, install the desktop file and icon into your user data
dir once:

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

* Partition table writing (GPT, MBR, hybrid MBR / GPT, BIOS+UEFI
  CSM) against in-memory `Cursor<Vec<u8>>`.
* ISO classification, distro family detection, persistence support
  flags.
* `autounattend.xml` generation: every option block, every
  architecture, the BitLocker guard, the Windows CA 2023 copy
  command, and the desktop-helpers xcopy.
* The desktop-helpers bundle (the list of `.bat` files is checked
  for drift; adding or removing one without updating the list
  fails).
* Decompressor adapters for `.xz`, `.gz`, `.bz2`, `.zst`, `.lzma`,
  `.zip`, `.Z`, and fixed `.vhd`.
* Persistence config rewriting (casper `persistent`, Debian
  `persistence.conf`, Slax `slax/changes/`).
* FAT volume label sanitisation, hostname sanitisation.
* JSON roundtrip of every `Job` variant.

For the full write paths, a loopback driver creates a sparse file,
attaches it as a loop device, and runs the helper end-to-end. It
needs root because the helper opens block devices:

```sh
sudo ./tests/loop-test.sh
```

The loop-test does not touch any real hardware. It always allocates
a fresh sparse image, attaches it as `/dev/loop<N>`, and detaches
it on exit. You cannot accidentally wipe a USB stick with it.

A separate decompressor smoke test lives at
`tests/decompress-test.sh`. It builds a synthetic compressed payload
of each supported format on the fly (no external compressor binaries
required for `.Z`) and round-trips it through the GUI's decompressor
adapters.

## Pre-PR checks

Run the workspace tripwire before pushing:

```sh
./check.sh
```

It runs:

* `cargo clippy --workspace --all-targets --locked -- -D warnings`. Every
  warning fails the gate; this is the single source of truth for "is
  clippy happy?".
* `cargo test --workspace --locked`. Catches regressions in the unit
  and integration test suite.
* A scan of `data/translations/usbooty_fr.ts` for `type="unfinished"`
  entries, a re-run of `lupdate6` into a scratch copy to catch a
  *stale* catalog (a qsTr string added in QML but never run through
  `update-translations.sh`), and a `lrelease6` smoke compile.
* A `grep` for the em-dash character (U+2014) under `docs/`. The
  project rule is that docs contain none.

`cargo fmt` is intentionally NOT enforced; the tree predates a global
rustfmt pass and turning it on as a tripwire would make every PR a
drive-by reformat. Run `cargo fmt --all` manually if you want a clean
diff.

There is no GitHub Actions / Forgejo Workflow file at the moment.
`check.sh` is the single source of truth; reference it from a pre-push
hook if you want it to run automatically.

## Translations

User-visible strings flow through Qt's `qsTr()`. Refresh the catalog
after editing QML:

```sh
./data/translations/update-translations.sh
```

That script runs `lupdate6` (which extracts strings from
`crates/gui/qml/`) and then `lrelease6` (which compiles the resulting
`.ts` to `.qm`). The compiled `.qm` is embedded via
`crates/gui/qrc/translations.qrc` and picked up at runtime by the
`QTranslator` install in `crates/gui/include/translator_bridge.cpp`.

Force English (from the `?` menu) swaps the translator at runtime
without restarting.

Watch out for one trap: `lupdate6`'s QML lexer treats `\U` as a
Unicode escape, so a literal Windows path like `C:\Users\` in a
`qsTr()` string corrupts the extracted catalog. Use the `&#x5C;`
HTML entity instead when writing a literal backslash inside `qsTr()`.

## Code style

* No unwraps in non-test code. Use `anyhow::Result` and
  `with_context` for context-rich errors. The helper's bail
  behaviour is what the user sees in the GUI log, so context matters.
* No comments that restate what the code does. Comments explain
  why, especially when a choice is non-obvious (a workaround, a
  spec quirk, an incident postmortem).
* The privilege boundary is sacred: nothing in `usbooty-helper`
  reaches outwards to the network or to a GUI library; nothing in
  `usbooty-gui` opens a block device.

## Adding a new write method

1. Add a variant to `Job` in `crates/core/src/job.rs` with the fully
   resolved parameters it needs.
2. Add a JSON roundtrip test in the same file.
3. In `crates/helper/src/main.rs`, dispatch on the new variant and
   call into a new module that does the work. Reuse `blockdev`,
   `partition`, `fsutil`, and `emit` rather than rolling your own.
4. In `crates/gui/src/bridge/jobs.rs`, build the new `Job` variant
   from the QML state. Add any new properties in
   `crates/gui/src/bridge/mod.rs` and their state in
   `crates/gui/src/bridge/state.rs`.
5. In `crates/gui/qml/main.qml`, surface the new option in the
   Options card.

## Adding a new Windows-setup option

1. Add a field to `WindowsSetup` in `crates/core/src/job.rs` and
   add it to the `is_active()` predicate so an empty struct still
   emits no settings.
2. In the matching pass module under `crates/helper/src/unattend/`
   (`windows_pe.rs`, `specialize.rs`, or `oobe.rs`), emit the right
   XML for the pass it belongs to (`windowsPE`, `specialize`, or
   `oobeSystem`). Add a unit test under `unattend::mod::tests` that
   asserts the expected XML fragment.
3. In `crates/gui/src/bridge/mod.rs`, add a `#[qproperty(bool, ...)]`
   line, the field in `AppControllerRust`, the default, and wire
   it into the `WindowsSetup { ... }` builder in `start()`.
4. In `crates/gui/qml/dialogs/WindowsSetupDialog.qml`, add a
   `WrapCheckBox` (or matching control). Give it a tooltip
   that explains what the user actually gets.
5. Refresh translations
   (`./data/translations/update-translations.sh`) and translate
   any new strings.

## Adding a new post-install desktop helper

1. Drop a new `.bat` file under `crates/helper/src/desktop_helpers/`.
   Use CRLF line endings, finish with `pause`, and prefer a
   `title` directive at the top so the user knows what they
   launched.
2. Add it to the `DESKTOP_HELPERS` `&[(name, body)]` constant in
   `crates/helper/src/unattend/assets.rs`.
3. Update the bundle expectation in the
   `desktop_helpers_bundle_lists_every_shipped_script` unit test.
4. Mention it in
   `crates/helper/src/desktop_helpers/README.txt`.
5. Add a row to the expander label and bump the count in the
   tooltip in `crates/gui/qml/main.qml`. Update
   [`docs/windows-iso.md`](windows-iso.md) too.

## Packaging

The AUR PKGBUILD lives at `packaging/PKGBUILD`. To publish:

* Push your source changes to the upstream git repo.
* In your local AUR clone (a separate git repo), update the
  PKGBUILD if any packaging-relevant lines changed (deps, options,
  install layout).
* `makepkg -Cfsi` to test locally.
* `makepkg --printsrcinfo > .SRCINFO`, then commit and push to AUR.

`packaging/publish-aur.sh` automates the AUR side if you have set it
up.
