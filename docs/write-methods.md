# Write methods

USBooty offers five ways to write a USB. Pick based on what is going
on the drive and how you intend to boot from it.

## 1. DD image (raw byte-for-byte copy)

A direct copy of the ISO contents to the device, sector by sector.
The drive ends up looking exactly like the source ISO. No
partitioning happens on USBooty's side: the boot loader, partition
table, filesystem, and contents all come from the ISO.

The picker accepts the raw image plus several common compressed
wrappers, transparently decompressed during the write:

* `.iso`, `.img`, `.bin`, `.raw`
* `.xz`, `.gz`, `.bz2`, `.zst`, `.lzma`
* `.zip` (single file inside)
* `.Z` (Unix LZW)
* Fixed `.vhd` (footer-aware)

**Best for**

* Isohybrid Linux ISOs (Arch, Fedora, openSUSE, most modern distros).
* Disk images (`.img` files), VHDs, and compressed images.
* Any ISO where the publisher specifically recommends `dd`.

**Tradeoffs**

* The drive is not writable past the end of the ISO. Free space is
  wasted.
* The drive cannot easily be reformatted later from a file manager:
  you have to re-partition it.
* Verification (read-back hash check) is supported and recommended.

## 2. Partition and copy files

A fresh partition table, a fresh filesystem, and a file-by-file copy
of the ISO contents. The drive is fully writable afterwards: you can
drop additional files on it from your file manager.

You explicitly choose the partition scheme:

| Scheme            | Use when                                                   |
|-------------------|------------------------------------------------------------|
| GPT (UEFI)        | Modern UEFI firmware.                                      |
| MBR (BIOS)        | Pure legacy BIOS.                                          |
| MBR (BIOS+UEFI)   | Older boards in CSM mode that boot MBR partitions on UEFI. |
| Hybrid MBR / GPT  | One stick that boots on both legacy BIOS and UEFI.         |

The filesystem is picked automatically based on the ISO type and the
size of the largest file. The combo only shows filesystems whose
`mkfs.*` tool is installed on the host, so the choice is never a
silent fail:

| ISO class                                | Default filesystem |
|------------------------------------------|--------------------|
| Linux                                    | FAT32              |
| Windows, `install.wim` 4 GiB or smaller  | FAT32              |
| Windows, `install.wim` larger than 4 GiB | Asks: split, or UEFI:NTFS / UEFI:exFAT. See [Windows ISOs](windows-iso.md). |

Filesystems you can pick by hand when the host has the tool installed:

* FAT16, FAT32, NTFS, exFAT, UDF
* ext2, ext3, ext4
* Btrfs, XFS, F2FS, JFS, NILFS2

**Best for**

* Windows installers (any flavour).
* Drives you want to keep writing to after the install.
* Persistent Linux live USBs. See [Linux ISOs](linux-iso.md).

## 3. Format only (no ISO)

Wipe the drive and lay down an empty partition plus filesystem. No
payload, no boot loader. The result is a freshly formatted USB stick,
the same as if you had just run `mkfs.*` by hand.

The combo lists every filesystem in the table above whose tool is
installed on the host.

**Best for**

* Resetting a drive after an experiment.
* Preparing a blank drive of a specific filesystem.

## 4. Ventoy multi-boot

Install (or update) Ventoy on the drive. Ventoy lays out its own
partitions, so USBooty shells out to the bundled Ventoy CLI. After
the install you can drop ISOs straight onto the Ventoy data
partition, and Ventoy's bootloader presents a menu at boot time.

**Best for**

* A single USB that boots many ISOs.
* Test machines where you want to swap ISOs without re-flashing.

**Notes**

* If you pass an ISO with the Ventoy method, USBooty copies it onto
  the Ventoy data partition after the install. The ISO is optional.
* Choose GPT or MBR explicitly. Ventoy supports both via its `-g`
  flag.
* Secure Boot support is a Ventoy install flag (`-s`).
* Updating an existing Ventoy install (`-u`) keeps your ISOs intact.

## 5. FreeDOS bootable USB

Build a self-contained FreeDOS USB without needing a DOS ISO. USBooty
downloads the latest `KERNEL.SYS` and `COMMAND.COM` from the upstream
FreeDOS GitHub releases (cached daily under
`$XDG_CACHE_HOME/usbooty/`), formats the drive as FAT16 or FAT32
(your choice), installs the FreeDOS boot sector via `mformat -B`
plus the Syslinux MBR, and copies the kernel and shell into the
partition with `mcopy`.

**Best for**

* BIOS / UEFI firmware flashing tools that ship as DOS executables.
* Old DOS utilities, DOS-only tooling for legacy hardware.

**Notes**

* No ISO is required. The "Source image" field is ignored.
* Needs `mtools` on the host.
* The drive is fully writable afterwards: drop your own `.exe` /
  `.com` flashing tools onto the partition.

## Read-back verify

When the **Verify** checkbox is enabled, the helper reads the written
bytes back after the job and compares them against a hash captured
live during the write. This catches:

* A bad flash where writes silently fail.
* A USB stick where some sectors map to bad flash and read back as
  zeros.
* A truncated copy.

For DD and Partitioned methods, verify is a full read-back hash
compare. For Format, Ventoy, and FreeDOS there is no verifiable
payload, so the checkbox is disabled.

## Device health: bad-blocks scan and SMART probe

USBooty offers two device-level checks that do not write any user
data:

* **Quick check** (the F3-style fake-capacity probe): writes a
  sparse pattern at strategic offsets and reads it back to detect
  capacity-faked USB sticks (the classic "16 GB stick that is really
  2 GB").
* **Full check** (two-pattern destructive scan): writes a `0x55` /
  `0xAA` pattern across the whole device and reads it back. Catches
  bad blocks that read as zeros or stuck at one.

A background SMART probe runs against the selected device using
`smartctl` if it is installed. Reallocated sectors, high
temperatures, and failing-prediction flags surface as a short
yellow warning under the device picker. No probe means no warning.

## Conflicting-process guard (devlock)

Before touching a device the helper checks whether another process
already holds it open (a file manager preview, an in-flight `dd`,
GNOME Files indexing, etc.). If something else is holding the device,
the write is refused with a clear message naming the offending
process, so you can close it and retry.

## Boot mode reference

| Method                            | UEFI                              | Legacy BIOS                            |
|-----------------------------------|-----------------------------------|----------------------------------------|
| DD                                | If the ISO ships UEFI boot files  | If the ISO ships BIOS boot files       |
| Partitioned, GPT, FAT32           | Yes                               | Possible; depends on firmware          |
| Partitioned, MBR, FAT32           | Yes                               | Yes                                    |
| Partitioned, GPT, UEFI:NTFS       | Yes                               | No                                     |
| Partitioned, GPT, UEFI:exFAT      | Yes                               | No                                     |
| Partitioned, MBR, UEFI:NTFS       | Yes                               | No                                     |
| Partitioned, Hybrid MBR / GPT     | Yes                               | Yes                                    |
| Partitioned, MBR (BIOS+UEFI)      | Yes (in CSM mode)                 | Yes                                    |
| Ventoy                            | Yes                               | Yes                                    |
| FreeDOS                           | Possible (CSM mode)               | Yes                                    |
