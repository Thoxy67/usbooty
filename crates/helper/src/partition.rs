//! Writes a fresh GPT or MBR partition table to the target device.

use anyhow::{Context, Result};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};

use usbooty_core::{FileSystem, PartitionTable};

/// Partition alignment in sectors: 1 MiB, the universal modern default,
/// expressed in the device's logical sector size.
fn align_sectors(sector_size: u64) -> u64 {
    (1024 * 1024 / sector_size).max(1)
}

/// Microsoft Basic Data partition type GUID, in on-disk byte order. Used for
/// *both* the main partition and the tiny UEFI:NTFS partition. UEFI firmware
/// boots `/EFI/BOOT/boot*.efi` from a FAT partition regardless of its type
/// GUID, and, crucially, Rufus found that declaring the UEFI:NTFS partition
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

/// GPT attribute bit 63: "do not assign a drive letter". Set on the tiny
/// UEFI:NTFS partition so Windows never surfaces it to the user.
const GPT_ATTR_NO_DRIVE_LETTER: u64 = 1 << 63;

/// MBR partition type byte for "FAT32 with LBA addressing".
const MBR_TYPE_FAT32_LBA: u8 = 0x0C;
/// MBR partition type byte for NTFS / exFAT (both use `0x07`).
const MBR_TYPE_NTFS: u8 = 0x07;
/// MBR partition type byte for a Linux filesystem.
const MBR_TYPE_LINUX: u8 = 0x83;
/// MBR partition type byte for an EFI System Partition: the type Rufus uses
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
        // NTFS partition. UDF doesn't have its own GPT GUID; Basic Data is
        // what every BD/UDF burner ships with too.
        _ => BASIC_DATA_GUID,
    }
}

/// Last LBA reachable through CHS addressing under the conventional
/// 1024-cylinder / 255-head / 63-sector translated geometry (~7.8 GiB).
const CHS_HORIZON: u32 = 1024 * 255 * 63;

/// The CHS tuple for `lba` under the conventional translated geometry,
/// saturating to the "past the horizon" 1023/254/63 marker every BIOS
/// understands as "use LBA". Old BIOSes (and some buggy 2005-2012 ones)
/// derive the disk geometry by matching the first partition's CHS fields
/// against its LBA; all-zero CHS makes some refuse to boot.
fn chs_for_lba(lba: u32) -> mbrman::CHS {
    mbrman::CHS::from_lba_exact(lba, 1023, 255, 63)
        .unwrap_or_else(|_| mbrman::CHS::new(1023, 254, 63))
}

/// The MBR partition-type byte for a `filesystem` whose last sector is
/// `end_lba`.
fn mbr_type_byte(filesystem: FileSystem, end_lba: u32) -> u8 {
    match filesystem {
        FileSystem::Fat32 => MBR_TYPE_FAT32_LBA,
        // FAT16 partitions ≥ 32 MiB use type 0x06 (FAT16B) while they stay
        // CHS-addressable, which is what genuinely old BIOSes expect; past
        // the CHS horizon the LBA variant 0x0E is required. We never write
        // sub-32 MiB partitions, so the older 0x04 type is unnecessary.
        FileSystem::Fat16 => {
            if end_lba < CHS_HORIZON {
                0x06
            } else {
                0x0E
            }
        }
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
/// Fails loudly rather than silently returning zeros; an all-zero GPT disk
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
    sector_size: u64,
) -> Result<()> {
    match table {
        PartitionTable::Gpt => write_gpt(device, filesystem, name, sector_size),
        // Plain BIOS-only MBR and the BIOS+UEFI variant share the same
        // on-disk layout: one bootable partition spanning the device. The
        // BIOS+UEFI flavour just commits the user to a FAT-family FS so
        // UEFI fallback (`/EFI/BOOT/BOOT*.EFI` on the partition) works.
        PartitionTable::Mbr | PartitionTable::MbrBiosUefi => {
            write_mbr(device, filesystem, sector_size)
        }
        PartitionTable::HybridMbrGpt => write_hybrid_mbr_gpt(device, filesystem, name, sector_size),
    }
}

fn write_gpt<D: Read + Write + Seek>(
    device: &mut D,
    filesystem: FileSystem,
    name: &str,
    sector_size: u64,
) -> Result<()> {
    let mut gpt = gptman::GPT::new_from(device, sector_size, random_bytes::<16>()?)
        .context("creating GPT")?;
    // Recompute usable LBAs from the device's real size.
    gpt.header
        .update_from(device, sector_size)
        .context("sizing GPT to the device")?;

    gpt[1] = gptman::GPTPartitionEntry {
        partition_type_guid: gpt_type_guid(filesystem),
        unique_partition_guid: random_bytes::<16>()?,
        starting_lba: gpt.header.first_usable_lba.max(align_sectors(sector_size)),
        ending_lba: gpt.header.last_usable_lba,
        attribute_bits: 0,
        partition_name: crate::label::partition(name).as_str().into(),
    };

    gpt.write_into(device).context("writing GPT")?;
    gptman::GPT::write_protective_mbr_into(device, sector_size).context("writing protective MBR")?;
    Ok(())
}

fn write_mbr<D: Read + Write + Seek>(
    device: &mut D,
    filesystem: FileSystem,
    sector_size: u64,
) -> Result<()> {
    let mut mbr = mbrman::MBR::new_from(device, sector_size as u32, random_bytes::<4>()?)
        .context("creating MBR")?;
    let start = align_sectors(sector_size) as u32;
    let sectors = mbr.disk_size.saturating_sub(start);
    let end = start + sectors - 1;

    mbr[1] = mbrman::MBRPartitionEntry {
        boot: mbrman::BOOT_ACTIVE,
        first_chs: chs_for_lba(start),
        sys: mbr_type_byte(filesystem, end),
        last_chs: chs_for_lba(end),
        starting_lba: start,
        sectors,
    };

    mbr.write_into(device).context("writing MBR")?;
    Ok(())
}

/// Write a GPT + one-partition layout, then synthesise a hybrid MBR pointing
/// the legacy slot at the same data partition.
fn write_hybrid_mbr_gpt<D: Read + Write + Seek>(
    device: &mut D,
    filesystem: FileSystem,
    name: &str,
    sector_size: u64,
) -> Result<()> {
    write_gpt(device, filesystem, name, sector_size)?;
    synthesize_hybrid_mbr(device, filesystem, 1, sector_size)
}

/// Replace the protective MBR that `gptman` writes with a *hybrid* MBR:
/// slot 1 mirrors GPT partition entry `gpt_index` as a real, bootable
/// partition that legacy BIOSes will pick up; slot 2 keeps the protective
/// `0xEE` entry covering the GPT primary header so partitioning tools
/// recognise the disk as GPT-aware.
///
/// Compatible with most modern BIOSes (and required for Apple Macs that
/// support legacy boot from a GPT disk). A handful of buggy firmwares
/// dislike any hybrid layout; that's the trade-off of the option.
fn synthesize_hybrid_mbr<D: Read + Write + Seek>(
    device: &mut D,
    filesystem: FileSystem,
    gpt_index: u32,
    sector_size: u64,
) -> Result<()> {
    // Re-parse the GPT just to learn the data partition's LBA range; we
    // could plumb the values through arguments, but reading them straight
    // from disk keeps the call sites tiny and self-consistent.
    let gpt = gptman::GPT::find_from(device).context("re-reading GPT for hybrid MBR")?;
    let entry = gpt
        .iter()
        .find(|(i, e)| *i == gpt_index && e.is_used())
        .map(|(_, e)| e)
        .context("GPT data partition missing for hybrid MBR")?;
    let start = u32::try_from(entry.starting_lba)
        .context("hybrid MBR cannot address a partition past 2 TiB")?;
    let end = u32::try_from(entry.ending_lba)
        .context("hybrid MBR cannot address a partition past 2 TiB")?;
    let sectors = end - start + 1;

    let mut mbr = mbrman::MBR::read_from(device, sector_size as u32)
        .context("re-reading protective MBR")?;

    // Slot 1: real bootable mirror entry for legacy BIOS.
    mbr[1] = mbrman::MBRPartitionEntry {
        boot: mbrman::BOOT_ACTIVE,
        first_chs: chs_for_lba(start),
        sys: mbr_type_byte(filesystem, end),
        last_chs: chs_for_lba(end),
        starting_lba: start,
        sectors,
    };
    // Slot 2: protective EFI-GPT entry, covering the area before the data
    // partition (which holds the GPT primary header + entries). Keeps
    // partitioning tools from treating the disk as legacy-only.
    mbr[2] = mbrman::MBRPartitionEntry {
        boot: mbrman::BOOT_INACTIVE,
        first_chs: chs_for_lba(1),
        sys: 0xEE,
        last_chs: chs_for_lba(start.saturating_sub(1)),
        starting_lba: 1,
        sectors: start.saturating_sub(1).max(1),
    };
    mbr[3] = mbrman::MBRPartitionEntry::empty();
    mbr[4] = mbrman::MBRPartitionEntry::empty();

    mbr.write_into(device).context("writing hybrid MBR")?;
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
    sector_size: u64,
) -> Result<()> {
    let fat_sectors = fat_bytes.div_ceil(sector_size);
    match table {
        // GPT-based variants; the hybrid one then synthesises an MBR mirror.
        PartitionTable::Gpt => write_gpt_uefi_ntfs(device, fat_sectors, main_name, sector_size),
        PartitionTable::HybridMbrGpt => {
            write_gpt_uefi_ntfs(device, fat_sectors, main_name, sector_size)?;
            // Build a hybrid MBR over the GPT layout: slot 1 mirrors the
            // *main* (NTFS) partition as bootable so legacy BIOSes can find
            // it. Slot 2 is the protective entry for the GPT areas. The
            // tiny FAT bootloader partition is reachable via UEFI through
            // the GPT; BIOSes don't need it.
            synthesize_hybrid_mbr(device, FileSystem::Ntfs, 1, sector_size)
        }
        // Pure-MBR variants: same on-disk layout.
        PartitionTable::Mbr | PartitionTable::MbrBiosUefi => {
            write_mbr_uefi_ntfs(device, fat_sectors, sector_size)
        }
    }
}

fn write_gpt_uefi_ntfs<D: Read + Write + Seek>(
    device: &mut D,
    fat_sectors: u64,
    main_name: &str,
    sector_size: u64,
) -> Result<()> {
    let mut gpt = gptman::GPT::new_from(device, sector_size, random_bytes::<16>()?)
        .context("creating GPT")?;
    gpt.header
        .update_from(device, sector_size)
        .context("sizing GPT to the device")?;

    let first = gpt.header.first_usable_lba.max(align_sectors(sector_size));
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
    // Typed Basic Data (not ESP, see `BASIC_DATA_GUID`) and hidden from
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
    gptman::GPT::write_protective_mbr_into(device, sector_size).context("writing protective MBR")?;
    Ok(())
}

fn write_mbr_uefi_ntfs<D: Read + Write + Seek>(
    device: &mut D,
    fat_sectors: u64,
    sector_size: u64,
) -> Result<()> {
    let mut mbr = mbrman::MBR::new_from(device, sector_size as u32, random_bytes::<4>()?)
        .context("creating MBR")?;

    let total = mbr.disk_size;
    let fat_sectors =
        u32::try_from(fat_sectors).context("FAT partition exceeds the MBR 2 TiB sector limit")?;
    let p1_start = align_sectors(sector_size) as u32;
    if total <= p1_start + fat_sectors {
        anyhow::bail!("device is too small for the UEFI:NTFS layout");
    }
    let fat_start = total - fat_sectors;

    // Partition 1: NTFS.
    mbr[1] = mbrman::MBRPartitionEntry {
        boot: mbrman::BOOT_INACTIVE,
        first_chs: chs_for_lba(p1_start),
        sys: MBR_TYPE_NTFS,
        last_chs: chs_for_lba(fat_start - 1),
        starting_lba: p1_start,
        sectors: fat_start - p1_start,
    };
    // Partition 2: tiny EFI System partition carrying the UEFI:NTFS bootloader,
    // marked active.
    mbr[2] = mbrman::MBRPartitionEntry {
        boot: mbrman::BOOT_ACTIVE,
        first_chs: chs_for_lba(fat_start),
        sys: MBR_TYPE_EFI_SYSTEM,
        last_chs: chs_for_lba(total - 1),
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
    sector_size: u64,
) -> Result<()> {
    let pers_sectors = persistence_bytes.div_ceil(sector_size);
    match table {
        PartitionTable::Gpt => {
            write_gpt_persistence(device, filesystem, pers_sectors, main_name, sector_size)
        }
        PartitionTable::HybridMbrGpt => {
            write_gpt_persistence(device, filesystem, pers_sectors, main_name, sector_size)?;
            // Mirror the data partition (slot 1, GPT entry 1) into the MBR
            // so a legacy BIOS can boot the live system; the persistence
            // overlay isn't bootable and stays GPT-only.
            synthesize_hybrid_mbr(device, filesystem, 1, sector_size)
        }
        PartitionTable::Mbr | PartitionTable::MbrBiosUefi => {
            write_mbr_persistence(device, filesystem, pers_sectors, sector_size)
        }
    }
}

fn write_gpt_persistence<D: Read + Write + Seek>(
    device: &mut D,
    filesystem: FileSystem,
    pers_sectors: u64,
    main_name: &str,
    sector_size: u64,
) -> Result<()> {
    let mut gpt = gptman::GPT::new_from(device, sector_size, random_bytes::<16>()?)
        .context("creating GPT")?;
    gpt.header
        .update_from(device, sector_size)
        .context("sizing GPT to the device")?;

    let first = gpt.header.first_usable_lba.max(align_sectors(sector_size));
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
    gptman::GPT::write_protective_mbr_into(device, sector_size).context("writing protective MBR")?;
    Ok(())
}

fn write_mbr_persistence<D: Read + Write + Seek>(
    device: &mut D,
    filesystem: FileSystem,
    pers_sectors: u64,
    sector_size: u64,
) -> Result<()> {
    let mut mbr = mbrman::MBR::new_from(device, sector_size as u32, random_bytes::<4>()?)
        .context("creating MBR")?;

    let total = mbr.disk_size;
    let pers_sectors = u32::try_from(pers_sectors)
        .context("persistence partition exceeds the MBR 2 TiB sector limit")?;
    let p1_start = align_sectors(sector_size) as u32;
    if total <= p1_start + pers_sectors {
        anyhow::bail!("device is too small for the persistence layout");
    }
    let pers_start = total - pers_sectors;

    mbr[1] = mbrman::MBRPartitionEntry {
        boot: mbrman::BOOT_ACTIVE,
        first_chs: chs_for_lba(p1_start),
        sys: mbr_type_byte(filesystem, pers_start - 1),
        last_chs: chs_for_lba(pers_start - 1),
        starting_lba: p1_start,
        sectors: pers_start - p1_start,
    };
    mbr[2] = mbrman::MBRPartitionEntry {
        boot: mbrman::BOOT_INACTIVE,
        first_chs: chs_for_lba(pers_start),
        sys: MBR_TYPE_LINUX,
        last_chs: chs_for_lba(total - 1),
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

    /// Default logical sector size: what the in-memory test targets use. Real
    /// devices are probed with `BLKSSZGET` (`blockdev::logical_sector_size`).
    const SECTOR: u64 = 512;

    /// gptman/mbrman operate on any `Read + Write + Seek`, so an in-memory
    /// buffer stands in for a device: no root, no real disk.
    fn disk(size: usize) -> Cursor<Vec<u8>> {
        Cursor::new(vec![0u8; size])
    }

    #[test]
    fn writes_a_gpt_with_one_basic_data_partition() {
        let mut disk = disk(64 * 1024 * 1024);
        write_single_partition(
            &mut disk,
            PartitionTable::Gpt,
            FileSystem::Fat32,
            "USBOOTY",
            SECTOR,
        )
        .unwrap();

        disk.set_position(0);
        let gpt = gptman::GPT::read_from(&mut disk, SECTOR).unwrap();
        assert_eq!(gpt[1].partition_type_guid, BASIC_DATA_GUID);
        assert!(gpt[1].starting_lba >= align_sectors(SECTOR));
        assert!(gpt[1].ending_lba > gpt[1].starting_lba);
        assert_eq!(gpt[2].partition_type_guid, [0u8; 16]); // only one partition
    }

    #[test]
    fn writes_an_mbr_with_one_active_fat32_partition() {
        let mut disk = disk(64 * 1024 * 1024);
        write_single_partition(
            &mut disk,
            PartitionTable::Mbr,
            FileSystem::Fat32,
            "USBOOTY",
            SECTOR,
        )
        .unwrap();

        disk.set_position(0);
        let mbr = mbrman::MBR::read_from(&mut disk, SECTOR as u32).unwrap();
        assert!(mbr[1].is_used());
        assert_eq!(mbr[1].sys, MBR_TYPE_FAT32_LBA);
        assert_eq!(mbr[1].boot, mbrman::BOOT_ACTIVE);
        assert_eq!(mbr[1].starting_lba, align_sectors(SECTOR) as u32);
    }

    #[test]
    fn gpt_on_4kn_device_uses_4096_byte_lbas_and_1mib_alignment() {
        let mut disk = disk(64 * 1024 * 1024);
        write_single_partition(
            &mut disk,
            PartitionTable::Gpt,
            FileSystem::Fat32,
            "USBOOTY",
            4096,
        )
        .unwrap();

        disk.set_position(0);
        // Readable only at the 4096-byte sector size: proves every LBA is in
        // 4Kn units, the thing a 4Kn enclosure's firmware actually parses.
        let gpt = gptman::GPT::read_from(&mut disk, 4096).unwrap();
        assert_eq!(gpt[1].partition_type_guid, BASIC_DATA_GUID);
        // 1 MiB alignment is 256 sectors of 4096 bytes.
        assert_eq!(align_sectors(4096), 256);
        assert!(gpt[1].starting_lba >= align_sectors(4096));
    }

    #[test]
    fn chs_values_are_real_within_horizon_and_saturated_past_it() {
        // LBA 2048 under 255/63 translated geometry: cylinder 0, head 32,
        // sector 33 (2048 = 32*63 + 32, sector is 1-based).
        let chs = chs_for_lba(2048);
        assert_eq!((chs.cylinder, chs.head, chs.sector), (0, 32, 33));
        // Past the 8 GB horizon: the saturated all-ones tuple.
        let sat = chs_for_lba(CHS_HORIZON + 1);
        assert_eq!((sat.cylinder, sat.head, sat.sector), (1023, 254, 63));
    }

    #[test]
    fn mbr_partition_carries_real_chs_and_fat16_lba_type_past_horizon() {
        let mut disk = disk(64 * 1024 * 1024);
        write_single_partition(&mut disk, PartitionTable::Mbr, FileSystem::Fat32, "X", SECTOR)
            .unwrap();
        disk.set_position(0);
        let mbr = mbrman::MBR::read_from(&mut disk, SECTOR as u32).unwrap();
        // A 64 MiB disk is entirely CHS-addressable: real values, not zeros.
        assert_ne!(mbr[1].first_chs, mbrman::CHS::empty());
        assert_ne!(mbr[1].last_chs, mbrman::CHS::empty());

        // FAT16 type byte: CHS variant inside the horizon, LBA variant past it.
        assert_eq!(mbr_type_byte(FileSystem::Fat16, CHS_HORIZON - 1), 0x06);
        assert_eq!(mbr_type_byte(FileSystem::Fat16, CHS_HORIZON), 0x0E);
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
