# Write methods

usbooty offers four ways to write a USB. Pick based on what is going on the
drive and how you intend to boot from it.

## 1. DD image (raw byte-for-byte copy)

A direct copy of the ISO contents to the device, sector by sector. The drive
ends up looking exactly like the source ISO. No partitioning happens on
usbooty's side: the boot loader, partition table, filesystem, and contents
all come from the ISO.

**Best for**

* Isohybrid Linux ISOs (Arch, Fedora, openSUSE, most modern distros).
* Disk images (`.img` files).
* Any ISO where the publisher specifically recommends `dd`.

**Tradeoffs**

* The drive is not writable past the end of the ISO. Free space is wasted.
* The drive cannot easily be reformatted later from a file manager: you have
  to re-partition it.
* Verification (read-back hash check) is supported and recommended.

## 2. Partition and copy files

A fresh partition table, a fresh filesystem, and a file-by-file copy of the
ISO contents. The drive is fully writable afterwards: you can drop additional
files on it from your file manager.

You explicitly choose the partition scheme (GPT for UEFI, MBR for legacy BIOS
or CSM). The filesystem is picked automatically based on the ISO type and
the size of the largest file:

| ISO class                                | Default filesystem |
|------------------------------------------|--------------------|
| Linux                                    | FAT32 |
| Windows, `install.wim` 4 GiB or smaller  | FAT32 |
| Windows, `install.wim` larger than 4 GiB | Asks: split, or UEFI:NTFS. See [Windows ISOs](windows-iso.md). |

**Best for**

* Windows installers.
* Drives you want to keep writing to after the install.
* Persistent Linux live USBs. See [Linux ISOs](linux-iso.md).

## 3. Format only (no ISO)

Wipe the drive and lay down an empty partition plus filesystem. No payload,
no boot loader. The result is a freshly formatted USB stick, the same as if
you had just run `mkfs.*` by hand.

This is the only method where you can pick the filesystem explicitly: FAT32,
NTFS, exFAT, ext4.

**Best for**

* Resetting a drive after an experiment.
* Preparing a blank drive of a specific filesystem.

## 4. Ventoy multi-boot

Install (or update) Ventoy on the drive. Ventoy lays out its own partitions,
so usbooty just shells out to the bundled Ventoy CLI. After the install, you
can drop ISOs straight onto the Ventoy data partition, and Ventoy's
bootloader presents a menu at boot time.

**Best for**

* A single USB that boots many ISOs.
* Test machines where you want to swap ISOs without re-flashing.

**Notes**

* If you pass an ISO with the Ventoy method, usbooty copies it onto the
  Ventoy data partition after the install. The ISO is optional.
* Choose GPT or MBR explicitly. Ventoy supports both via its `-g` flag.
* Secure Boot support is a Ventoy install flag (`-s`).
* Updating an existing Ventoy install (`-u`) keeps your ISOs intact.

## The "Verify" checkbox

When enabled, the helper reads the written bytes back after the job and
compares them against a hash captured live during the write. This catches:

* A bad flash where writes silently fail.
* A USB stick where some sectors map to bad flash and read back as zeros.
* A truncated copy.

For DD and Partitioned methods, verify is a full read-back hash compare. For
Format and Ventoy, there is no verifiable payload, so the checkbox is
disabled.

## Boot mode reference

| Method                       | UEFI  | Legacy BIOS |
|------------------------------|-------|-------------|
| DD                           | If the ISO ships UEFI boot files | If the ISO ships BIOS boot files |
| Partitioned, GPT, FAT32      | Yes   | Possible; depends on firmware |
| Partitioned, MBR, FAT32      | Yes   | Yes |
| Partitioned, GPT, UEFI:NTFS  | Yes   | No |
| Partitioned, MBR, UEFI:NTFS  | Yes   | No |
| Ventoy                       | Yes   | Yes |
