//! Round-trip test for the decompression cache.
//!
//! Driven by `tests/decompress-test.sh`, which generates a known ISO and
//! compresses it with every supported algorithm before invoking
//! `cargo test --include-ignored --test decompress_smoke`. The shell wrapper
//! exists so we don't add `xorriso`/`xz`/`zstd`/etc as test-time Rust deps —
//! shelling out is the natural way to get golden fixtures here.
//!
//! Each subtest:
//!   1. invokes `decompress_to_cache` on the compressed fixture,
//!   2. reads the resulting cache file,
//!   3. confirms it is byte-identical to the original ISO.

use std::path::PathBuf;

fn fixture_dir() -> Option<PathBuf> {
    std::env::var_os("USBOOTY_DECOMPRESS_FIXTURE_DIR").map(PathBuf::from)
}

fn fixture(name: &str) -> Option<PathBuf> {
    let dir = fixture_dir()?;
    let path = dir.join(name);
    path.is_file().then_some(path)
}

fn expected() -> Vec<u8> {
    let path = fixture_dir()
        .expect("USBOOTY_DECOMPRESS_FIXTURE_DIR must be set by tests/decompress-test.sh")
        .join("test.iso");
    std::fs::read(&path)
        .unwrap_or_else(|e| panic!("could not read reference ISO {}: {e}", path.display()))
}

fn assert_roundtrip(compressed_name: &str) {
    let Some(src) = fixture(compressed_name) else {
        eprintln!("skipping {compressed_name}: fixture missing (compressor not installed?)");
        return;
    };
    // Re-export the GUI crate's modules — these tests live in the crate's
    // own `tests/` so they only see the public surface.
    let out = usbooty_gui::decompress::decompress_to_cache(&src, |_, _| {})
        .unwrap_or_else(|e| panic!("decompress {compressed_name}: {e:#}"));
    let actual = std::fs::read(&out).expect("reading cached output");
    assert_eq!(
        actual,
        expected(),
        "decompressed {compressed_name} does not match the reference ISO"
    );
}

#[test]
#[ignore = "needs compressed fixtures from tests/decompress-test.sh"]
fn xz_roundtrip() {
    assert_roundtrip("test.iso.xz");
}

#[test]
#[ignore = "needs compressed fixtures from tests/decompress-test.sh"]
fn gz_roundtrip() {
    assert_roundtrip("test.iso.gz");
}

#[test]
#[ignore = "needs compressed fixtures from tests/decompress-test.sh"]
fn bz2_roundtrip() {
    assert_roundtrip("test.iso.bz2");
}

#[test]
#[ignore = "needs compressed fixtures from tests/decompress-test.sh"]
fn zst_roundtrip() {
    assert_roundtrip("test.iso.zst");
}

#[test]
#[ignore = "needs compressed fixtures from tests/decompress-test.sh"]
fn lzma_roundtrip() {
    assert_roundtrip("test.iso.lzma");
}

#[test]
#[ignore = "needs compressed fixtures from tests/decompress-test.sh"]
fn zip_roundtrip() {
    assert_roundtrip("test.iso.zip");
}

#[test]
#[ignore = "needs compressed fixtures from tests/decompress-test.sh"]
fn z_roundtrip() {
    assert_roundtrip("test.iso.Z");
}

/// Self-contained `.Z` round-trip — encode a small payload via `weezl` with
/// the same parameters our `DotZAdapter` expects (Unix-compress framing,
/// LSB-first packing, 8-bit alphabet), then decompress it through the
/// real public API. Runs without needing `ncompress` installed.
#[test]
fn z_roundtrip_synthetic() {
    let dir = tempfile::tempdir().expect("tempdir");
    let payload: Vec<u8> = (0..8192u32).map(|i| (i & 0xff) as u8).collect();

    // Build a minimal `.Z` stream: 3-byte header (1F 9D + flag) plus a raw
    // LSB-Lsb LZW stream over an 8-bit alphabet.
    let mut bytes = vec![0x1F, 0x9D, 0x90]; // flag: max-bits=16, block-mode set
    let mut encoder = weezl::encode::Encoder::new(weezl::BitOrder::Lsb, 8);
    let mut encoded = encoder.encode(&payload).expect("weezl encode");
    bytes.append(&mut encoded);

    let dot_z = dir.path().join("synthetic.Z");
    std::fs::write(&dot_z, &bytes).unwrap();

    let out = usbooty_gui::decompress::decompress_to_cache(&dot_z, |_, _| {})
        .expect("decompress_to_cache");
    let recovered = std::fs::read(&out).unwrap();
    assert_eq!(recovered, payload, "synthetic .Z round-trip lost bytes");
}

#[test]
fn detects_compression_by_magic() {
    // Make sure a renamed plain file isn't blindly trusted.
    let dir = tempfile::tempdir().expect("tempdir");
    let plain = dir.path().join("not-really.iso.xz");
    std::fs::write(&plain, b"this is not an xz stream").unwrap();
    assert_eq!(
        usbooty_gui::decompress::detect(&plain),
        usbooty_gui::decompress::Compression::None,
    );
}

#[test]
fn fixed_vhd_strips_to_original_bytes() {
    // Build a 4 KiB raw image, append a synthetic fixed-VHD footer, then
    // confirm `strip_footer_to_cache` recovers the original payload byte-
    // for-byte. Footer layout: 8-byte `conectix` cookie + 52 zero bytes +
    // 4 BE bytes of `disk_type = 2` + 448 padding bytes.
    let dir = tempfile::tempdir().expect("tempdir");
    let raw: Vec<u8> = (0..4096u32).map(|i| (i & 0xff) as u8).collect();
    let mut bytes = raw.clone();
    bytes.extend_from_slice(b"conectix");
    bytes.extend(std::iter::repeat_n(0u8, 52));
    bytes.extend_from_slice(&2u32.to_be_bytes()); // disk_type = fixed
    bytes.extend(std::iter::repeat_n(0u8, 512 - 8 - 52 - 4));
    assert_eq!(bytes.len(), raw.len() + 512);

    let vhd = dir.path().join("smoke.vhd");
    std::fs::write(&vhd, &bytes).unwrap();
    let out = usbooty_gui::vhd::strip_footer_to_cache(&vhd).expect("strip_footer_to_cache");
    let recovered = std::fs::read(&out).unwrap();
    assert_eq!(recovered, raw, "stripped VHD payload differs from input");
}

#[test]
fn dynamic_vhd_is_rejected() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut bytes = vec![0u8; 4096];
    bytes.extend_from_slice(b"conectix");
    bytes.extend(std::iter::repeat_n(0u8, 52));
    bytes.extend_from_slice(&3u32.to_be_bytes()); // disk_type = dynamic
    bytes.extend(std::iter::repeat_n(0u8, 512 - 8 - 52 - 4));
    let vhd = dir.path().join("dyn.vhd");
    std::fs::write(&vhd, &bytes).unwrap();
    let err = usbooty_gui::vhd::strip_footer_to_cache(&vhd).unwrap_err();
    assert!(format!("{err:#}").contains("dynamic"));
}
