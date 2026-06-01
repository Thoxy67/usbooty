//! Structural validation of the UEFI:NTFS bootloader image.
//!
//! The image is a fixed-size 1 MiB FAT boot image fetched at runtime from the
//! Rufus repository. It deliberately tracks Rufus `master`, so a pinned content
//! hash would go stale; callers validate its *structure* instead: the exact
//! size, a sane jump instruction at offset 0, and the boot-sector signature.
//! This catches a truncated or corrupted download before it is ever written to
//! a USB drive's boot partition.

/// The exact size of the UEFI:NTFS image, in bytes (a 1 MiB FAT image).
pub const UEFI_NTFS_IMG_SIZE: u64 = 1024 * 1024;

/// Validate that `bytes` is a structurally-sound UEFI:NTFS image.
///
/// On failure, returns a human-readable string describing the first failed
/// check, suitable for showing to the user.
pub fn validate_uefi_ntfs(bytes: &[u8]) -> Result<(), String> {
    if bytes.len() as u64 != UEFI_NTFS_IMG_SIZE {
        return Err(format!(
            "expected a {UEFI_NTFS_IMG_SIZE}-byte image, got {} bytes \
             (truncated or corrupt download?)",
            bytes.len()
        ));
    }
    // A FAT boot sector starts with a short or near jump: 0xEB or 0xE9.
    match bytes[0] {
        0xEB | 0xE9 => {}
        other => {
            return Err(format!(
                "not a FAT boot image: byte 0 is {other:#04x}, expected a 0xEB/0xE9 jump"
            ));
        }
    }
    // The 0x55 0xAA boot-sector signature sits at offset 510.
    if bytes[510] != 0x55 || bytes[511] != 0xAA {
        return Err(format!(
            "missing 0x55AA boot-sector signature (found {:#04x} {:#04x})",
            bytes[510], bytes[511]
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal byte vector that passes every structural check.
    fn good_image() -> Vec<u8> {
        let mut img = vec![0u8; UEFI_NTFS_IMG_SIZE as usize];
        img[0] = 0xEB;
        img[510] = 0x55;
        img[511] = 0xAA;
        img
    }

    #[test]
    fn accepts_a_well_formed_image() {
        assert!(validate_uefi_ntfs(&good_image()).is_ok());
    }

    #[test]
    fn rejects_a_wrong_size() {
        let mut img = good_image();
        img.truncate(1024);
        assert!(validate_uefi_ntfs(&img).is_err());
        assert!(validate_uefi_ntfs(&[]).is_err());
    }

    #[test]
    fn rejects_a_missing_boot_signature() {
        let mut img = good_image();
        img[511] = 0x00;
        assert!(validate_uefi_ntfs(&img).is_err());
    }

    #[test]
    fn rejects_a_bad_jump_instruction() {
        let mut img = good_image();
        img[0] = 0x00;
        assert!(validate_uefi_ntfs(&img).is_err());
    }
}
