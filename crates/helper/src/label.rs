//! Sanitizing a source image's volume label for each filesystem's rules.
//!
//! The label detected from the ISO is reused to name the USB partition and its
//! filesystem, so the finished drive carries the image's own name. Each target
//! has different constraints; an empty or unusable label falls back to a
//! default.

/// Fallback used when the image carries no usable label.
const DEFAULT: &str = "USBOOTY";

/// A FAT32 volume label: up to 11 characters, upper-case, restricted charset.
pub fn fat(raw: &str) -> String {
    let cleaned: String = raw
        .trim()
        .to_uppercase()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, ' ' | '_' | '-'))
        .take(11)
        .collect();
    or_default(cleaned)
}

/// An NTFS volume label: up to 32 characters, most printable characters allowed.
pub fn ntfs(raw: &str) -> String {
    or_default(bounded(raw, 32))
}

/// An exFAT volume label: up to 15 characters.
pub fn exfat(raw: &str) -> String {
    or_default(bounded(raw, 15))
}

/// An ext4 volume label: up to 16 bytes. Reused for ext2 and ext3, which
/// share the e2fsprogs label limit.
pub fn ext4(raw: &str) -> String {
    or_default(bounded_bytes(raw, 16))
}

/// A UDF volume label: up to 30 bytes of UTF-8. UDF natively supports Unicode,
/// but we trim to the bound `mkudffs` actually accepts on the command line.
pub fn udf(raw: &str) -> String {
    or_default(bounded_bytes(raw, 30))
}

/// A Btrfs volume label: up to 255 bytes; we cap shorter so the label fits
/// inside any reasonable display.
pub fn btrfs(raw: &str) -> String {
    or_default(bounded_bytes(raw, 64))
}

/// An XFS volume label: hard 12-byte limit (mkfs.xfs refuses anything more).
pub fn xfs(raw: &str) -> String {
    or_default(bounded_bytes(raw, 12))
}

/// An F2FS volume label: up to 512 bytes; we cap at the same 64 as Btrfs
/// for consistency in the UI.
pub fn f2fs(raw: &str) -> String {
    or_default(bounded_bytes(raw, 64))
}

/// A JFS volume label: 16 bytes.
pub fn jfs(raw: &str) -> String {
    or_default(bounded_bytes(raw, 16))
}

/// A NILFS2 volume label: 80 bytes; cap at 64 for UI consistency.
pub fn nilfs2(raw: &str) -> String {
    or_default(bounded_bytes(raw, 64))
}

/// A GPT partition name: up to 36 UTF-16 code units (36 chars for ASCII labels,
/// which is all an ISO9660 volume identifier can carry).
pub fn partition(raw: &str) -> String {
    or_default(bounded(raw, 36))
}

/// Trim `raw`, drop control characters, and cap at `max` characters. For
/// filesystems whose limit counts UTF-16 code units (NTFS, exFAT, GPT
/// names); labels with non-BMP characters are vanishingly rare, so
/// chars approximate code units well there.
fn bounded(raw: &str, max: usize) -> String {
    raw.trim()
        .chars()
        .filter(|c| !c.is_control())
        .take(max)
        .collect()
}

/// Like [`bounded`] but capped at `max` *bytes* of UTF-8, truncating on a
/// character boundary. For mkfs tools that enforce byte limits (e2fsprogs,
/// mkfs.xfs, mkudffs, ...): a multi-byte label capped by characters could
/// exceed the byte limit and fail the format mid-job.
fn bounded_bytes(raw: &str, max: usize) -> String {
    let mut out = String::with_capacity(max);
    for c in raw.trim().chars().filter(|c| !c.is_control()) {
        if out.len() + c.len_utf8() > max {
            break;
        }
        out.push(c);
    }
    out
}

/// Trim `value` and substitute the default when it is empty.
fn or_default(value: String) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        DEFAULT.to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fat_label_is_short_and_upper_case() {
        // The dots are dropped, not in FAT32's safe label charset.
        assert_eq!(fat("Ubuntu 24.04.1 LTS amd64"), "UBUNTU 2404");
        assert_eq!(fat("CCCOMA_X64FRE_EN-US_DV9"), "CCCOMA_X64F");
        assert_eq!(fat(""), "USBOOTY");
        assert_eq!(fat("  ***  "), "USBOOTY");
    }

    #[test]
    fn longer_labels_are_capped() {
        assert_eq!(ntfs("Win11_25H2_English_x64").len(), 22);
        assert!(partition(&"x".repeat(80)).len() <= 36);
        assert_eq!(partition(""), "USBOOTY");
    }

    #[test]
    fn byte_limited_labels_count_bytes_not_chars() {
        // "é" is 2 bytes in UTF-8: 12 of them would be 12 chars but 24 bytes,
        // which mkfs.xfs (hard 12-byte limit) rejects.
        let accented = "é".repeat(12);
        assert!(xfs(&accented).len() <= 12);
        assert_eq!(xfs(&accented), "é".repeat(6));
        // Truncation never splits a character.
        assert!(ext4(&"日".repeat(10)).is_char_boundary(16.min(ext4(&"日".repeat(10)).len())));
        assert_eq!(ext4(&"日".repeat(10)).len(), 15); // 5 chars * 3 bytes
    }
}
