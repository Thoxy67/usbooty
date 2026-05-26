//! Writes a fresh GPT or MBR partition table to the target device.

use anyhow::{Context, Result};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};

use usbooty_core::{FileSystem, PartitionTable};

/// Logical sector size assumed throughout (Linux reports 512 for `/sys`-style
/// sizing regardless of the physical sector size).
pub const SECTOR: u64 = 512;
/// Partition alignment: 1 MiB, the universal modern default.
const ALIGN_SECTORS: u64 = 2048;

/// Microsoft Basic Data partition type GUID, in on-disk byte order. Used for
/// *both* the main partition and the tiny UEFI:NTFS partition. UEFI firmware
/// boots `/EFI/BOOT/boot*.efi` from a FAT partition regardless of its type
/// GUID — and, crucially, Rufus found that declaring the UEFI:NTFS partition
/// as an EFI System Partition makes the Windows installer choke ("can't handle
/// two ESPs"), so it must stay Basic Data.
const BASIC_DATA_GUID: [u8; 16] = [
    0xA2, 0xA0, 0xD0, 0xEB, 0xE5, 0xB9, 0x33, 0x44, 0x87, 0xC0, 0x68, 0xB6, 0xB7, 0x26, 0x99, 0xC7,
];

/// Linux filesystem-data partition type GUID, in on-disk byte order. Used for
/// an ext4 partition (`0FC63DAF-8483-4772-8E79-3D69D8477DE4`).
const LINUX_DATA_GUID: [u8; 16] = [
    0xAF, 0x63, 0xC6, 0x0F, 0x83, 0x84, 0x72, 0x47, 0x8E, 0x79, 0x3D, 0x69, 0xD8, 0x47, 0x7D, 0xE4,
];

/// GPT attribute bit 63 — "do not assign a drive letter". Set on the tiny
/// UEFI:NTFS partition so Windows never surfaces it to the user.
const GPT_ATTR_NO_DRIVE_LETTER: u64 = 1 << 63;

/// MBR partition type byte for "FAT32 with LBA addressing".
const MBR_TYPE_FAT32_LBA: u8 = 0x0C;
/// MBR partition type byte for NTFS / exFAT (both use `0x07`).
const MBR_TYPE_NTFS: u8 = 0x07;
/// MBR partition type byte for a Linux filesystem.
const MBR_TYPE_LINUX: u8 = 0x83;
/// MBR partition type byte for an EFI System Partition — the type Rufus uses
/// for the UEFI:NTFS partition so firmware reliably boots it.
const MBR_TYPE_EFI_SYSTEM: u8 = 0xEF;

/// The GPT partition-type GUID for a `filesystem`.
fn gpt_type_guid(filesystem: FileSystem) -> [u8; 16] {
    match filesystem {
        FileSystem::Ext2
        | FileSystem::Ext3
        | FileSystem::Ext4
        | FileSystem::Btrfs
        | FileSystem::Xfs
        | FileSystem::F2fs
        | FileSystem::Jfs
        | FileSystem::Nilfs2 => LINUX_DATA_GUID,
        // FAT16 / FAT32 / NTFS / exFAT / UDF all map to Microsoft Basic Data,
        // which is the GPT type firmware looks for when probing for an FAT or
        // NTFS partition. UDF doesn't have its own GPT GUID — Basic Data is
        // what every BD/UDF burner ships with too.
        _ => BASIC_DATA_GUID,
    }
}

/// The MBR partition-type byte for a `filesystem`.
fn mbr_type_byte(filesystem: FileSystem) -> u8 {
    match filesystem {
        FileSystem::Fat32 => MBR_TYPE_FAT32_LBA,
        // FAT16 partitions ≥ 32 MiB use type 0x06 (FAT16B); we never write
        // sub-32 MiB partitions, so the older 0x04 type is unnecessary.
        FileSystem::Fat16 => 0x06,
        FileSystem::Ntfs | FileSystem::ExFat => MBR_TYPE_NTFS,
        // Linux Data covers every Linux-native filesystem at the MBR level:
        // BIOSes and bootloaders inspect the partition's superblock, not the
        // type byte, so we don't need a one-per-FS code here.
        FileSystem::Ext2
        | FileSystem::Ext3
        | FileSystem::Ext4
        | FileSystem::Btrfs
        | FileSystem::Xfs
        | FileSystem::F2fs
        | FileSystem::Jfs
        | FileSystem::Nilfs2
        | FileSystem::Udf => MBR_TYPE_LINUX,
    }
}

/// Read `N` random bytes from `/dev/urandom`, for GUIDs and disk signatures.
/// Fails loudly rather than silently returning zeros — an all-zero GPT disk
/// GUID or MBR disk signature violates the spec and has been known to confuse
/// Windows and firmware (two zero-signature disks look identical to the boot
/// manager), so silently degrading here would defeat the point of the tool.
fn random_bytes<const N: usize>() -> Result<[u8; N]> {
    let mut buf = [0u8; N];
    let mut f = File::open("/dev/urandom").context("opening /dev/urandom for GUIDs/signatures")?;
    f.read_exact(&mut buf)
        .context("reading /dev/urandom for GUIDs/signatures")?;
    Ok(buf)
}

/// Zero the first and last mebibyte of the device, erasing stale partition
/// tables and filesystem superblocks that could otherwise confuse the kernel.
pub fn wipe_signatures<D: Read + Write + Seek>(device: &mut D, device_size: u64) -> Result<()> {
    let zeros = vec![0u8; 1024 * 1024];
    device.seek(SeekFrom::Start(0))?;
    device.write_all(&zeros).context("wiping start of device")?;
    if device_size > 2 * 1024 * 1024 {
        device.seek(SeekFrom::Start(device_size - 1024 * 1024))?;
        device.write_all(&zeros).context("wiping end of device")?;
    }
    device.flush()?;
    device.seek(SeekFrom::Start(0))?;
    Ok(())
}

/// Write a single data partition spanning the whole device, typed for
/// `filesystem`. For MBR the partition is marked active; `name` becomes the
/// GPT partition name (MBR has no partition names).
pub fn write_single_partition<D: Read + Write + Seek>(
    device: &mut D,
    table: PartitionTable,
    filesystem: FileSystem,
    name: &str,
) -> Result<()> {
    match table {
        PartitionTable::Gpt => write_gpt(device, filesystem, name),
        PartitionTable::Mbr => write_mbr(device, filesystem),
    }
}

fn write_gpt<D: Read + Write + Seek>(
    device: &mut D,
    filesystem: FileSystem,
    name: &str,
) -> Result<()> {
    let mut gpt =
        gptman::GPT::new_from(device, SECTOR, random_bytes::<16>()?).context("creating GPT")?;
    // Recompute usable LBAs from the device's real size.
    gpt.header
        .update_from(device, SECTOR)
        .context("sizing GPT to the device")?;

    gpt[1] = gptman::GPTPartitionEntry {
        partition_type_guid: gpt_type_guid(filesystem),
        unique_partition_guid: random_bytes::<16>()?,
        starting_lba: gpt.header.first_usable_lba.max(ALIGN_SECTORS),
        ending_lba: gpt.header.last_usable_lba,
        attribute_bits: 0,
        partition_name: crate::label::partition(name).as_str().into(),
    };

    gpt.write_into(device).context("writing GPT")?;
    gptman::GPT::write_protective_mbr_into(device, SECTOR).context("writing protective MBR")?;
    Ok(())
}

fn write_mbr<D: Read + Write + Seek>(device: &mut D, filesystem: FileSystem) -> Result<()> {
    let mut mbr = mbrman::MBR::new_from(device, SECTOR as u32, random_bytes::<4>()?)
        .context("creating MBR")?;
    let sectors = mbr.disk_size.saturating_sub(ALIGN_SECTORS as u32);

    mbr[1] = mbrman::MBRPartitionEntry {
        boot: mbrman::BOOT_ACTIVE,
        first_chs: mbrman::CHS::empty(),
        sys: mbr_type_byte(filesystem),
        last_chs: mbrman::CHS::empty(),
        starting_lba: ALIGN_SECTORS as u32,
        sectors,
    };

    mbr.write_into(device).context("writing MBR")?;
    Ok(())
}

/// Write the UEFI:NTFS two-partition layout: a large NTFS partition holding the
/// Windows files, plus a tiny FAT32 partition at the end of the disk carrying
/// the UEFI:NTFS bootloader image. `fat_bytes` is the exact size of that image.
pub fn write_uefi_ntfs_layout<D: Read + Write + Seek>(
    device: &mut D,
    table: PartitionTable,
    fat_bytes: u64,
    main_name: &str,
) -> Result<()> {
    let fat_sectors = fat_bytes.div_ceil(SECTOR);
    match table {
        PartitionTable::Gpt => write_gpt_uefi_ntfs(device, fat_sectors, main_name),
        PartitionTable::Mbr => write_mbr_uefi_ntfs(device, fat_sectors),
    }
}

fn write_gpt_uefi_ntfs<D: Read + Write + Seek>(
    device: &mut D,
    fat_sectors: u64,
    main_name: &str,
) -> Result<()> {
    let mut gpt =
        gptman::GPT::new_from(device, SECTOR, random_bytes::<16>()?).context("creating GPT")?;
    gpt.header
        .update_from(device, SECTOR)
        .context("sizing GPT to the device")?;

    let first = gpt.header.first_usable_lba.max(ALIGN_SECTORS);
    let last = gpt.header.last_usable_lba;
    if last <= first + fat_sectors {
        anyhow::bail!("device is too small for the UEFI:NTFS layout");
    }
    let fat_start = last + 1 - fat_sectors;

    // Partition 1: NTFS, holds the Windows files.
    gpt[1] = gptman::GPTPartitionEntry {
        partition_type_guid: BASIC_DATA_GUID,
        unique_partition_guid: random_bytes::<16>()?,
        starting_lba: first,
        ending_lba: fat_start - 1,
        attribute_bits: 0,
        partition_name: crate::label::partition(main_name).as_str().into(),
    };
    // Partition 2: tiny FAT32 partition holding the UEFI:NTFS bootloader.
    // Typed Basic Data (not ESP — see `BASIC_DATA_GUID`) and hidden from
    // Windows via the no-drive-letter attribute.
    gpt[2] = gptman::GPTPartitionEntry {
        partition_type_guid: BASIC_DATA_GUID,
        unique_partition_guid: random_bytes::<16>()?,
        starting_lba: fat_start,
        ending_lba: last,
        attribute_bits: GPT_ATTR_NO_DRIVE_LETTER,
        partition_name: "UEFI_NTFS".into(),
    };

    gpt.write_into(device).context("writing GPT")?;
    gptman::GPT::write_protective_mbr_into(device, SECTOR).context("writing protective MBR")?;
    Ok(())
}

fn write_mbr_uefi_ntfs<D: Read + Write + Seek>(device: &mut D, fat_sectors: u64) -> Result<()> {
    let mut mbr = mbrman::MBR::new_from(device, SECTOR as u32, random_bytes::<4>()?)
        .context("creating MBR")?;

    let total = mbr.disk_size;
    let fat_sectors = fat_sectors as u32;
    let p1_start = ALIGN_SECTORS as u32;
    if total <= p1_start + fat_sectors {
        anyhow::bail!("device is too small for the UEFI:NTFS layout");
    }
    let fat_start = total - fat_sectors;

    // Partition 1: NTFS.
    mbr[1] = mbrman::MBRPartitionEntry {
        boot: mbrman::BOOT_INACTIVE,
        first_chs: mbrman::CHS::empty(),
        sys: MBR_TYPE_NTFS,
        last_chs: mbrman::CHS::empty(),
        starting_lba: p1_start,
        sectors: fat_start - p1_start,
    };
    // Partition 2: tiny EFI System partition carrying the UEFI:NTFS bootloader,
    // marked active.
    mbr[2] = mbrman::MBRPartitionEntry {
        boot: mbrman::BOOT_ACTIVE,
        first_chs: mbrman::CHS::empty(),
        sys: MBR_TYPE_EFI_SYSTEM,
        last_chs: mbrman::CHS::empty(),
        starting_lba: fat_start,
        sectors: fat_sectors,
    };

    mbr.write_into(device).context("writing MBR")?;
    Ok(())
}

/// Write a two-partition layout for a Linux live USB with persistence: a main
/// `filesystem` partition for the live system, plus a trailing ext4 partition
/// of `persistence_bytes` for the writable overlay.
pub fn write_persistence_layout<D: Read + Write + Seek>(
    device: &mut D,
    table: PartitionTable,
    filesystem: FileSystem,
    persistence_bytes: u64,
    main_name: &str,
) -> Result<()> {
    let pers_sectors = persistence_bytes.div_ceil(SECTOR);
    match table {
        PartitionTable::Gpt => write_gpt_persistence(device, filesystem, pers_sectors, main_name),
        PartitionTable::Mbr => write_mbr_persistence(device, filesystem, pers_sectors),
    }
}

fn write_gpt_persistence<D: Read + Write + Seek>(
    device: &mut D,
    filesystem: FileSystem,
    pers_sectors: u64,
    main_name: &str,
) -> Result<()> {
    let mut gpt =
        gptman::GPT::new_from(device, SECTOR, random_bytes::<16>()?).context("creating GPT")?;
    gpt.header
        .update_from(device, SECTOR)
        .context("sizing GPT to the device")?;

    let first = gpt.header.first_usable_lba.max(ALIGN_SECTORS);
    let last = gpt.header.last_usable_lba;
    if last <= first + pers_sectors {
        anyhow::bail!("device is too small for the persistence layout");
    }
    let pers_start = last + 1 - pers_sectors;

    gpt[1] = gptman::GPTPartitionEntry {
        partition_type_guid: gpt_type_guid(filesystem),
        unique_partition_guid: random_bytes::<16>()?,
        starting_lba: first,
        ending_lba: pers_start - 1,
        attribute_bits: 0,
        partition_name: crate::label::partition(main_name).as_str().into(),
    };
    gpt[2] = gptman::GPTPartitionEntry {
        partition_type_guid: LINUX_DATA_GUID,
        unique_partition_guid: random_bytes::<16>()?,
        starting_lba: pers_start,
        ending_lba: last,
        attribute_bits: 0,
        partition_name: "persistence".into(),
    };

    gpt.write_into(device).context("writing GPT")?;
    gptman::GPT::write_protective_mbr_into(device, SECTOR).context("writing protective MBR")?;
    Ok(())
}

fn write_mbr_persistence<D: Read + Write + Seek>(
    device: &mut D,
    filesystem: FileSystem,
    pers_sectors: u64,
) -> Result<()> {
    let mut mbr = mbrman::MBR::new_from(device, SECTOR as u32, random_bytes::<4>()?)
        .context("creating MBR")?;

    let total = mbr.disk_size;
    let pers_sectors = pers_sectors as u32;
    let p1_start = ALIGN_SECTORS as u32;
    if total <= p1_start + pers_sectors {
        anyhow::bail!("device is too small for the persistence layout");
    }
    let pers_start = total - pers_sectors;

    mbr[1] = mbrman::MBRPartitionEntry {
        boot: mbrman::BOOT_ACTIVE,
        first_chs: mbrman::CHS::empty(),
        sys: mbr_type_byte(filesystem),
        last_chs: mbrman::CHS::empty(),
        starting_lba: p1_start,
        sectors: pers_start - p1_start,
    };
    mbr[2] = mbrman::MBRPartitionEntry {
        boot: mbrman::BOOT_INACTIVE,
        first_chs: mbrman::CHS::empty(),
        sys: MBR_TYPE_LINUX,
        last_chs: mbrman::CHS::empty(),
        starting_lba: pers_start,
        sectors: pers_sectors,
    };

    mbr.write_into(device).context("writing MBR")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    /// gptman/mbrman operate on any `Read + Write + Seek`, so an in-memory
    /// buffer stands in for a device — no root, no real disk.
    fn disk(size: usize) -> Cursor<Vec<u8>> {
        Cursor::new(vec![0u8; size])
    }

    #[test]
    fn writes_a_gpt_with_one_basic_data_partition() {
        let mut disk = disk(64 * 1024 * 1024);
        write_single_partition(&mut disk, PartitionTable::Gpt, FileSystem::Fat32, "USBOOTY")
            .unwrap();

        disk.set_position(0);
        let gpt = gptman::GPT::read_from(&mut disk, SECTOR).unwrap();
        assert_eq!(gpt[1].partition_type_guid, BASIC_DATA_GUID);
        assert!(gpt[1].starting_lba >= ALIGN_SECTORS);
        assert!(gpt[1].ending_lba > gpt[1].starting_lba);
        assert_eq!(gpt[2].partition_type_guid, [0u8; 16]); // only one partition
    }

    #[test]
    fn writes_an_mbr_with_one_active_fat32_partition() {
        let mut disk = disk(64 * 1024 * 1024);
        write_single_partition(&mut disk, PartitionTable::Mbr, FileSystem::Fat32, "USBOOTY")
            .unwrap();

        disk.set_position(0);
        let mbr = mbrman::MBR::read_from(&mut disk, SECTOR as u32).unwrap();
        assert!(mbr[1].is_used());
        assert_eq!(mbr[1].sys, MBR_TYPE_FAT32_LBA);
        assert_eq!(mbr[1].boot, mbrman::BOOT_ACTIVE);
        assert_eq!(mbr[1].starting_lba, ALIGN_SECTORS as u32);
    }

    #[test]
    fn wipe_clears_both_ends() {
        let mut disk = disk(8 * 1024 * 1024);
        for b in disk.get_mut().iter_mut() {
            *b = 0xAA;
        }
        wipe_signatures(&mut disk, 8 * 1024 * 1024).unwrap();
        let data = disk.into_inner();
        assert!(data[..1024 * 1024].iter().all(|&b| b == 0));
        assert!(data[7 * 1024 * 1024..].iter().all(|&b| b == 0));
        assert_eq!(data[3 * 1024 * 1024], 0xAA); // middle untouched
    }
}
