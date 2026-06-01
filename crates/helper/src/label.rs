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
    or_default(bounded(raw, 16))
}

/// A UDF volume label: up to 30 bytes of UTF-8. UDF natively supports Unicode,
/// but we trim to the bound `mkudffs` actually accepts on the command line.
pub fn udf(raw: &str) -> String {
    or_default(bounded(raw, 30))
}

/// A Btrfs volume label: up to 255 bytes; we cap shorter so the label fits
/// inside any reasonable display.
pub fn btrfs(raw: &str) -> String {
    or_default(bounded(raw, 64))
}

/// An XFS volume label: hard 12-byte limit (mkfs.xfs refuses anything more).
pub fn xfs(raw: &str) -> String {
    or_default(bounded(raw, 12))
}

/// An F2FS volume label: up to 512 bytes; we cap at the same 64 as Btrfs
/// for consistency in the UI.
pub fn f2fs(raw: &str) -> String {
    or_default(bounded(raw, 64))
}

/// A JFS volume label: 16 bytes.
pub fn jfs(raw: &str) -> String {
    or_default(bounded(raw, 16))
}

/// A NILFS2 volume label: 80 bytes; cap at 64 for UI consistency.
pub fn nilfs2(raw: &str) -> String {
    or_default(bounded(raw, 64))
}

/// A GPT partition name: up to 36 UTF-16 code units (36 chars for ASCII labels,
/// which is all an ISO9660 volume identifier can carry).
pub fn partition(raw: &str) -> String {
    or_default(bounded(raw, 36))
}

/// Trim `raw`, drop control characters, and cap at `max` characters.
fn bounded(raw: &str, max: usize) -> String {
    raw.trim()
        .chars()
        .filter(|c| !c.is_control())
        .take(max)
        .collect()
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
}
