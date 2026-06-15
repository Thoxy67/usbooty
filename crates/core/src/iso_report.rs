//! The result of analyzing an ISO image.
//!
//! Produced by the GUI's `iso` module (which reads the ISO9660 filesystem
//! directly), consumed by [`crate::plan`] to decide a partition layout.

use serde::{Deserialize, Serialize};

/// The live-USB persistence scheme an ISO supports, if any.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PersistenceKind {
    /// Ubuntu / casper-based live systems: a `casper-rw` (or `writable`) partition.
    CasperRw,
    /// Debian-live: a `persistence` partition carrying a `persistence.conf` file.
    DebianLive,
    /// RHEL-family live (Alma/Rocky/CentOS): an ext4 partition with the fixed
    /// label `OVERLAY` holding a sparse `overlay.img` COW file. dracut's
    /// dmsquash-live loop-mounts that file as a dm-snapshot when
    /// `rd.live.overlay=LABEL=OVERLAY:/overlay.img` is on the kernel command
    /// line (which we add when patching configs).
    FedoraOverlay,
    /// Fedora 40+ (and Fedora-derived) live: same `OVERLAY` partition but
    /// used *directly* as an overlayfs upper dir via
    /// `rd.live.overlay=LABEL=OVERLAY rd.live.overlay.overlayfs=1`. No COW
    /// file, so none of dm-snapshot's silent-corruption-when-full failure
    /// mode. Kept separate from [`FedoraOverlay`] because RHEL-era dracut
    /// lacks overlayfs support.
    FedoraOverlayFs,
    /// openSUSE live (kiwi-live): an ext4 partition labelled `cow`, picked up
    /// automatically by the live system; no kernel parameter required.
    OpenSuseCow,
    /// archiso-based live systems (Arch Linux, CachyOS, etc.): an ext4 partition
    /// labelled `PERSISTENCE`. archiso turns it into the persistent cowspace
    /// when `cow_label=PERSISTENCE` is on the kernel command line (see the
    /// archiso `README.bootparams` `cow_label` / `cow_device` options).
    ArchOverlay,
    /// Slax: *no separate partition*. Slax 9+ persists into `/slax/changes/`
    /// at the data partition's root; the helper creates the directory and
    /// Slax picks it up automatically on writable media, no kernel parameter
    /// required (`perchsize=` exists only to raise the FAT 16 GiB cap).
    /// Unlike every other variant the size slider is ignored; Slax just
    /// keeps writing into the folder until the partition fills.
    SlaxChanges,
    /// Knoppix: an ext4 partition labelled `KNOPPIX-DATA`. The Knoppix
    /// initrd scans every partition for that label at boot and adopts it as
    /// the persistent overlay automatically; no kernel parameter is needed.
    KnoppixData,
    /// Alpine "diskless" mode. No overlay partition: Alpine runs from RAM and
    /// persists config via `lbu` (an apkovl tarball) on the writable boot
    /// media. usbooty just prepares a local apk cache directory; the user runs
    /// `lbu commit` to save. Inline, like Slax (the writable boot partition
    /// itself holds the state).
    AlpineLbu,
}

impl PersistenceKind {
    /// Whether this scheme needs its own dedicated partition. False for
    /// inline variants (Slax, Alpine), where the size slider is moot because
    /// persistence lives inside the main (writable) data partition.
    pub fn needs_partition(self) -> bool {
        !matches!(
            self,
            PersistenceKind::SlaxChanges | PersistenceKind::AlpineLbu
        )
    }
}

/// The Linux distribution (or family) usbooty has recognised inside an ISO.
///
/// Used for two things: routing persistence to the right scheme (so a
/// Mint ISO uses CasperRw, an LMDE ISO uses DebianLive, etc.) and applying
/// per-distro post-copy fixes that mirror Rufus's `iso.c` quirk table.
/// `Unknown` is the polite default; usbooty still writes the ISO with the
/// generic flow, it just doesn't add distro-specific patches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DistroFamily {
    /// No family recognised. Generic Linux handling.
    #[default]
    Unknown,
    /// Ubuntu (Casper-based).
    Ubuntu,
    /// Linux Mint (Casper-based, Ubuntu-derived).
    Mint,
    /// Linux Mint Debian Edition (Debian Live family).
    Lmde,
    /// Debian Live.
    Debian,
    /// Fedora Workstation / Spins (LiveOS overlay).
    Fedora,
    /// Bazzite (Fedora-derived, Universal Blue / OSTree image).
    Bazzite,
    /// Nobara (Fedora-derived).
    Nobara,
    /// AlmaLinux (RHEL rebuild; same dracut dmsquash-live live media as Fedora).
    AlmaLinux,
    /// Rocky Linux (RHEL rebuild; same dracut dmsquash-live live media).
    Rocky,
    /// CentOS Stream (RHEL upstream; same dracut dmsquash-live live media).
    CentOs,
    /// openSUSE (kiwi-live).
    OpenSuse,
    /// GeckoLinux (openSUSE-derived).
    GeckoLinux,
    /// Arch Linux (archiso).
    Arch,
    /// Manjaro (archiso-derived).
    Manjaro,
    /// EndeavourOS (archiso-derived).
    EndeavourOs,
    /// CachyOS (archiso-derived).
    CachyOs,
    /// Alpine Linux (diskless mode). Persists via `lbu` (an apkovl tarball on
    /// writable media), not an overlay partition, so no persistence partition
    /// is offered.
    Alpine,
    /// Slax: own `/slax/changes/` scheme.
    Slax,
    /// Knoppix: `KNOPPIX-DATA` auto-adopted overlay partition.
    Knoppix,
    /// Kali Linux (Debian Live family; its boot menu ships a native
    /// "Live USB Persistence" entry using the standard Debian scheme).
    Kali,
    /// Pop!_OS (Casper-based, Ubuntu-derived).
    PopOs,
    /// Zorin OS (Casper-based, Ubuntu-derived).
    Zorin,
    /// elementary OS (Casper-based, Ubuntu-derived).
    Elementary,
    /// KDE neon (Casper-based, Ubuntu-derived).
    KdeNeon,
    /// Linux Lite (Casper-based, Ubuntu-derived).
    LinuxLite,
    /// Garuda Linux (miso/buildiso, Arch-derived).
    Garuda,
    /// Artix Linux (archiso-derived, no systemd).
    Artix,
    /// Tails. Persistence is its own LUKS2 "Persistent Storage" created from
    /// inside Tails; usbooty must not (and cannot) pre-create it.
    Tails,
    /// Puppy Linux family (FossaPup, BookwormPup, ...). Persists via a save
    /// file/folder Puppy itself offers to create on first shutdown.
    Puppy,
    /// antiX / MX Linux. Persistence (rootfs/homefs files) is configured
    /// from their own live boot menu; pre-creating the files is fragile.
    Antix,
}

impl DistroFamily {
    /// Recognise the distribution from the ISO's volume label plus the names
    /// of files and directories at its root.
    ///
    /// Detection follows a most-specific-first cascade so a derivative
    /// (Bazzite, Nobara, Mint, LMDE, GeckoLinux) always wins over its parent
    /// (Fedora, Ubuntu, Debian, openSUSE). Root markers cover ISOs whose
    /// labels were customised away from the upstream default, e.g. a `slax/`
    /// directory or a `knoppix*` directory pin the family even on a renamed
    /// ISO.
    ///
    /// `root_entries` is whatever `iso.rs` already lists at the root (a `Vec`
    /// of `(name, is_dir, size)`); passing the existing slice keeps the GUI
    /// from re-walking the ISO just to classify the distro.
    pub fn detect(label: &str, root_entries: &[(String, bool, u64)]) -> DistroFamily {
        let label_low = label.to_ascii_lowercase();
        let has_dir = |needle: &str| {
            root_entries
                .iter()
                .any(|(name, is_dir, _)| *is_dir && name == needle)
        };
        let dir_starts_with = |prefix: &str| {
            root_entries
                .iter()
                .any(|(name, is_dir, _)| *is_dir && name.starts_with(prefix))
        };

        // Root-marker overrides; these beat the label entirely, because
        // these distros put a hard-named directory at the ISO root.
        if has_dir("slax") || dir_starts_with("slax-") {
            return DistroFamily::Slax;
        }
        if dir_starts_with("knoppix") || has_dir("knoppix") {
            return DistroFamily::Knoppix;
        }
        // antiX/MX ship a hard-named `antiX/` directory at the root.
        if root_entries
            .iter()
            .any(|(name, is_dir, _)| *is_dir && name.eq_ignore_ascii_case("antix"))
        {
            return DistroFamily::Antix;
        }
        // Puppy variants carry their squashfs modules as `puppy_*.sfs` (or
        // `*pup*.sfs`) files at the root; labels vary wildly per puplet.
        if root_entries.iter().any(|(name, is_dir, _)| {
            !*is_dir
                && name.ends_with(".sfs")
                && (name.starts_with("puppy_") || name.contains("pup"))
        }) {
            return DistroFamily::Puppy;
        }

        // Label-based detection, most specific first. Needles are pre-lowered
        // so each iteration is just a substring scan over the already-lowered
        // label, with no per-needle allocation.
        for (needle, family) in [
            ("bazzite", DistroFamily::Bazzite),
            ("nobara", DistroFamily::Nobara),
            ("lmde", DistroFamily::Lmde),
            ("linuxmint", DistroFamily::Mint),
            ("linux mint", DistroFamily::Mint),
            ("geckolinux", DistroFamily::GeckoLinux),
            ("manjaro", DistroFamily::Manjaro),
            ("endeavour", DistroFamily::EndeavourOs),
            ("cachyos", DistroFamily::CachyOs),
            ("cos_", DistroFamily::CachyOs),
            ("garuda", DistroFamily::Garuda),
            ("artix", DistroFamily::Artix),
            ("almalinux", DistroFamily::AlmaLinux),
            ("rockylinux", DistroFamily::Rocky),
            ("rocky", DistroFamily::Rocky),
            ("centos", DistroFamily::CentOs),
            ("alpine", DistroFamily::Alpine),
            // Debian derivatives with their own schemes/labels, before the
            // `debian` needle.
            ("kali", DistroFamily::Kali),
            ("tails", DistroFamily::Tails),
            // Ubuntu derivatives, before the `ubuntu` needle.
            ("pop_os", DistroFamily::PopOs),
            ("pop-os", DistroFamily::PopOs),
            ("zorin", DistroFamily::Zorin),
            ("elementary", DistroFamily::Elementary),
            ("kde neon", DistroFamily::KdeNeon),
            ("neon-", DistroFamily::KdeNeon),
            ("linux lite", DistroFamily::LinuxLite),
            ("linuxlite", DistroFamily::LinuxLite),
            ("puppy", DistroFamily::Puppy),
            ("fossapup", DistroFamily::Puppy),
            ("bookwormpup", DistroFamily::Puppy),
            ("ubuntu", DistroFamily::Ubuntu),
            ("kubuntu", DistroFamily::Ubuntu),
            ("xubuntu", DistroFamily::Ubuntu),
            ("lubuntu", DistroFamily::Ubuntu),
            ("debian", DistroFamily::Debian),
            ("fedora", DistroFamily::Fedora),
            ("fed_", DistroFamily::Fedora),
            ("opensuse", DistroFamily::OpenSuse),
            ("suse-", DistroFamily::OpenSuse),
            ("archlinux", DistroFamily::Arch),
            ("arch_", DistroFamily::Arch),
            ("arch-", DistroFamily::Arch),
        ] {
            // The loosest needles are common English substrings; require
            // token boundaries for them so "...details..." never reads as
            // Tails and "rockyou" never reads as Rocky. The rest stay plain
            // substring scans (they are distinctive enough, and several are
            // deliberate infixes like "kubuntu" ⊃ "ubuntu").
            let hit = match needle {
                "tails" | "rocky" => contains_token(&label_low, needle),
                _ => label_low.contains(needle),
            };
            if hit {
                return family;
            }
        }

        // Structural fallback: if the ISO has a `casper/` directory but the
        // label didn't name an Ubuntu derivative, call it generic Ubuntu so
        // CasperRw persistence is still offered.
        // Fedora / RHEL-family live media mark themselves with a `LiveOS/`
        // directory (holding squashfs.img). Treat an unlabelled one as the
        // generic dracut dmsquash-live family so the overlay scheme applies.
        if root_entries
            .iter()
            .any(|(name, is_dir, _)| *is_dir && name.eq_ignore_ascii_case("liveos"))
        {
            return DistroFamily::Fedora;
        }
        if has_dir("casper") {
            return DistroFamily::Ubuntu;
        }
        if has_dir("live") {
            return DistroFamily::Debian;
        }
        if has_dir("arch") {
            return DistroFamily::Arch;
        }
        DistroFamily::Unknown
    }

    /// Human-readable name, used by the UI hint that explains which scheme
    /// usbooty selected ("Detected Linux Mint → using casper persistence").
    pub fn display(self) -> &'static str {
        match self {
            DistroFamily::Unknown => "Unknown",
            DistroFamily::Ubuntu => "Ubuntu",
            DistroFamily::Mint => "Linux Mint",
            DistroFamily::Lmde => "LMDE",
            DistroFamily::Debian => "Debian",
            DistroFamily::Fedora => "Fedora",
            DistroFamily::Bazzite => "Bazzite",
            DistroFamily::Nobara => "Nobara",
            DistroFamily::AlmaLinux => "AlmaLinux",
            DistroFamily::Rocky => "Rocky Linux",
            DistroFamily::CentOs => "CentOS Stream",
            DistroFamily::OpenSuse => "openSUSE",
            DistroFamily::GeckoLinux => "GeckoLinux",
            DistroFamily::Arch => "Arch Linux",
            DistroFamily::Manjaro => "Manjaro",
            DistroFamily::EndeavourOs => "EndeavourOS",
            DistroFamily::CachyOs => "CachyOS",
            DistroFamily::Alpine => "Alpine Linux",
            DistroFamily::Slax => "Slax",
            DistroFamily::Knoppix => "Knoppix",
            DistroFamily::Kali => "Kali Linux",
            DistroFamily::PopOs => "Pop!_OS",
            DistroFamily::Zorin => "Zorin OS",
            DistroFamily::Elementary => "elementary OS",
            DistroFamily::KdeNeon => "KDE neon",
            DistroFamily::LinuxLite => "Linux Lite",
            DistroFamily::Garuda => "Garuda Linux",
            DistroFamily::Artix => "Artix Linux",
            DistroFamily::Tails => "Tails",
            DistroFamily::Puppy => "Puppy Linux",
            DistroFamily::Antix => "antiX / MX Linux",
        }
    }

    /// The persistence scheme this family uses, if any. Returning `None`
    /// means usbooty doesn't yet know how to set up a writable overlay for
    /// this distro; the user is offered the DD method and no slider.
    pub fn persistence(self) -> Option<PersistenceKind> {
        match self {
            DistroFamily::Ubuntu
            | DistroFamily::Mint
            | DistroFamily::PopOs
            | DistroFamily::Zorin
            | DistroFamily::Elementary
            | DistroFamily::KdeNeon
            | DistroFamily::LinuxLite => Some(PersistenceKind::CasperRw),
            DistroFamily::Debian | DistroFamily::Lmde | DistroFamily::Kali => {
                Some(PersistenceKind::DebianLive)
            }
            // Fedora-current dracut supports overlayfs persistence (no COW
            // file to exhaust); the RHEL rebuilds ship older dracut and keep
            // the dm-snapshot overlay.img scheme.
            DistroFamily::Fedora | DistroFamily::Bazzite | DistroFamily::Nobara => {
                Some(PersistenceKind::FedoraOverlayFs)
            }
            DistroFamily::AlmaLinux | DistroFamily::Rocky | DistroFamily::CentOs => {
                Some(PersistenceKind::FedoraOverlay)
            }
            DistroFamily::OpenSuse | DistroFamily::GeckoLinux => Some(PersistenceKind::OpenSuseCow),
            DistroFamily::Arch
            | DistroFamily::Manjaro
            | DistroFamily::EndeavourOs
            | DistroFamily::CachyOs
            | DistroFamily::Garuda
            | DistroFamily::Artix => Some(PersistenceKind::ArchOverlay),
            DistroFamily::Slax => Some(PersistenceKind::SlaxChanges),
            // Alpine "diskless" persists via lbu (an apkovl tarball on writable
            // media), an inline scheme rather than an overlay partition.
            DistroFamily::Alpine => Some(PersistenceKind::AlpineLbu),
            DistroFamily::Knoppix => Some(PersistenceKind::KnoppixData),
            // These manage persistence themselves; see `persistence_note_key`.
            DistroFamily::Tails | DistroFamily::Puppy | DistroFamily::Antix => None,
            DistroFamily::Unknown => None,
        }
    }

    /// A stable key naming the "why is there no persistence slider" note for
    /// families that manage persistence themselves. The GUI maps the key to
    /// a translated message (keys, not sentences, so the catalog owns the
    /// wording).
    pub fn persistence_note_key(self) -> Option<&'static str> {
        match self {
            DistroFamily::Tails => Some("tails"),
            DistroFamily::Puppy => Some("puppy"),
            DistroFamily::Antix => Some("antix"),
            _ => None,
        }
    }
}

/// Whether `haystack` contains `needle` as a whole token: neither neighbour
/// of the match may be ASCII alphanumeric. Used for the label needles that
/// are common English substrings (see [`DistroFamily::detect`]).
fn contains_token(haystack: &str, needle: &str) -> bool {
    let mut from = 0;
    while let Some(pos) = haystack[from..].find(needle) {
        let start = from + pos;
        let end = start + needle.len();
        let before_ok = !haystack[..start]
            .chars()
            .next_back()
            .is_some_and(|c| c.is_ascii_alphanumeric());
        let after_ok = !haystack[end..]
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphanumeric());
        if before_ok && after_ok {
            return true;
        }
        from = end;
    }
    false
}

/// The kind of operating system an ISO contains.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OsKind {
    /// A Windows installation ISO (`sources/install.wim` + `bootmgr`/`setup.exe`).
    Windows,
    /// A Linux ISO (isolinux or GRUB present).
    Linux,
    /// A BSD ISO (FreeBSD / OpenBSD / NetBSD / …), written with the DD method.
    Bsd,
    /// Something else, still writable with the DD method.
    Other,
}

/// A summary of everything the planner needs to know about an ISO.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IsoReport {
    /// Detected operating-system family.
    pub os_kind: OsKind,
    /// Whether the ISO carries an isohybrid MBR (directly DD-bootable).
    pub is_isohybrid: bool,
    /// `sources/install.wim` (or `.esd`/`.swm`) is present.
    pub has_install_wim: bool,
    /// Size of the install image in bytes, if present.
    pub install_wim_size: Option<u64>,
    /// The install image is `install.esd` (already-compressed) rather than `.wim`.
    pub install_wim_is_esd: bool,
    /// A `bootmgr` file is present in the root.
    pub has_bootmgr: bool,
    /// A `setup.exe` file is present in the root.
    pub has_setup_exe: bool,
    /// An `/EFI/BOOT/boot*.efi` UEFI bootloader directory is present.
    pub has_efi_boot_dir: bool,
    /// `isolinux.bin` / `isolinux.cfg` is present.
    pub has_isolinux: bool,
    /// A GRUB directory (`/boot/grub` or `/boot/grub2`) is present.
    pub has_grub: bool,
    /// Any single file is >= 4 GiB (cannot live on plain FAT32).
    pub has_4gb_file: bool,
    /// The ISO volume label.
    pub label: String,
    /// Total size of the ISO file on disk, in bytes.
    pub total_size: u64,
    /// Live-USB persistence support, if the ISO is a Linux live system.
    pub persistence: Option<PersistenceKind>,
    /// The Linux distribution family this ISO belongs to, when usbooty can
    /// recognise it. Drives persistence routing and post-copy quirk fixes;
    /// `Unknown` (the default) keeps the generic flow.
    #[serde(default)]
    pub distro: DistroFamily,
    /// Warnings raised by scanning the ISO's signed EFI binaries against the
    /// current UEFI revocation database (SBAT levels, DBX hashes). Empty when
    /// no concern was found or the ISO carries no signed EFI binaries.
    #[serde(default)]
    pub revocation_warnings: Vec<String>,
    /// Key for the GUI's "this distro manages persistence itself" note
    /// (see [`DistroFamily::persistence_note_key`]). Empty when there is
    /// nothing to explain.
    #[serde(default)]
    pub persistence_note_key: String,
}

impl IsoReport {
    /// A minimal report for an ISO we could not analyze (still DD-writable).
    pub fn unknown(total_size: u64) -> Self {
        IsoReport {
            os_kind: OsKind::Other,
            is_isohybrid: false,
            has_install_wim: false,
            install_wim_size: None,
            install_wim_is_esd: false,
            has_bootmgr: false,
            has_setup_exe: false,
            has_efi_boot_dir: false,
            has_isolinux: false,
            has_grub: false,
            has_4gb_file: false,
            label: String::new(),
            total_size,
            persistence: None,
            distro: DistroFamily::Unknown,
            revocation_warnings: Vec::new(),
            persistence_note_key: String::new(),
        }
    }

    /// A short human-readable summary line for the UI.
    pub fn summary(&self) -> String {
        let os = match self.os_kind {
            OsKind::Windows => "Windows",
            OsKind::Linux => "Linux",
            OsKind::Bsd => "BSD",
            OsKind::Other => "Generic image",
        };
        format!("{os} · {}", crate::device::format_size(self.total_size))
    }
}

#[cfg(test)]
mod distro_family_tests {
    use super::*;

    /// Helper: build a `(name, is_dir, size)` triple for `DistroFamily::detect`.
    fn dir(name: &str) -> (String, bool, u64) {
        (name.to_string(), true, 0)
    }

    #[test]
    fn label_pins_derivatives_over_their_parents() {
        // Each derivative must beat its parent. Listed parent-first so a
        // regression that swallows the derivative case is obvious.
        for (label, expected) in [
            ("BAZZITE_GNOME_42_x86_64", DistroFamily::Bazzite),
            ("NOBARA-OFFICIAL", DistroFamily::Nobara),
            ("Fedora-Workstation-Live-x86_64-40", DistroFamily::Fedora),
            ("LMDE-6", DistroFamily::Lmde),
            ("Linux Mint 21.3 Cinnamon", DistroFamily::Mint),
            ("debian-12.5.0-amd64-DVD-1", DistroFamily::Debian),
            ("GeckoLinux_Static_Cinnamon", DistroFamily::GeckoLinux),
            ("openSUSE-Leap-15.6-DVD-x86_64", DistroFamily::OpenSuse),
            ("Manjaro KDE 23.1.4", DistroFamily::Manjaro),
            ("EndeavourOS-2026.05", DistroFamily::EndeavourOs),
            ("CachyOS-Live-2026", DistroFamily::CachyOs),
            ("archlinux-2026.05.01-x86_64", DistroFamily::Arch),
            ("ubuntu-24.04-desktop-amd64", DistroFamily::Ubuntu),
            ("AlmaLinux-9-7-x86_64-Live-GNOME", DistroFamily::AlmaLinux),
            ("Rocky-9-KDE-x86_64-latest", DistroFamily::Rocky),
            ("CentOS-Stream-9-latest-x86_64-Live", DistroFamily::CentOs),
            ("alpine-standard-3.20.0-x86_64", DistroFamily::Alpine),
            ("kali-linux-2026.1-live-amd64", DistroFamily::Kali),
            ("TAILS 6.5 - 20260601", DistroFamily::Tails),
            ("Pop_OS 22.04 amd64 Intel", DistroFamily::PopOs),
            ("Zorin-OS-17.1-Core-64-bit", DistroFamily::Zorin),
            ("elementary OS 8.0", DistroFamily::Elementary),
            ("neon-user-20260601-0716", DistroFamily::KdeNeon),
            ("Linux Lite 7.0", DistroFamily::LinuxLite),
            ("GARUDA_DR460NIZED_RAPTOR", DistroFamily::Garuda),
            ("artix-plasma-openrc-20260501", DistroFamily::Artix),
            ("Puppy BookwormPup64 10.0", DistroFamily::Puppy),
        ] {
            assert_eq!(
                DistroFamily::detect(label, &[]),
                expected,
                "label `{label}` should detect as {expected:?}"
            );
        }
    }

    #[test]
    fn derivative_persistence_routing() {
        // The new families must route to the parent family's scheme.
        assert_eq!(
            DistroFamily::Kali.persistence(),
            Some(PersistenceKind::DebianLive)
        );
        for fam in [
            DistroFamily::PopOs,
            DistroFamily::Zorin,
            DistroFamily::Elementary,
            DistroFamily::KdeNeon,
            DistroFamily::LinuxLite,
        ] {
            assert_eq!(fam.persistence(), Some(PersistenceKind::CasperRw));
        }
        for fam in [DistroFamily::Garuda, DistroFamily::Artix] {
            assert_eq!(fam.persistence(), Some(PersistenceKind::ArchOverlay));
        }
        assert_eq!(
            DistroFamily::Knoppix.persistence(),
            Some(PersistenceKind::KnoppixData)
        );
        // Self-managed persistence: no slider, but a note for the UI.
        for fam in [
            DistroFamily::Tails,
            DistroFamily::Puppy,
            DistroFamily::Antix,
        ] {
            assert_eq!(fam.persistence(), None);
            assert!(fam.persistence_note_key().is_some());
        }
    }

    #[test]
    fn root_markers_pin_antix_and_puppy() {
        // antiX/MX: hard-named `antiX/` directory at the root.
        assert_eq!(
            DistroFamily::detect("CUSTOM-LABEL", &[dir("antiX")]),
            DistroFamily::Antix
        );
        // Puppy: squashfs modules at the root.
        let sfs = ("puppy_bookwormpup64_10.0.sfs".to_string(), false, 1_000u64);
        assert_eq!(
            DistroFamily::detect("CUSTOM", std::slice::from_ref(&sfs)),
            DistroFamily::Puppy
        );
    }

    #[test]
    fn detects_real_world_volume_labels() {
        // Exact ISO volume labels (not filenames) seen on real media. These
        // are the strings detection actually keys on.
        for (label, expected) in [
            ("alpine-std 3.23.4 x86_64", DistroFamily::Alpine),
            ("ARCH_202605", DistroFamily::Arch),
            ("COS_202604", DistroFamily::CachyOs),
            ("Rocky-10-1-x86_64-dvd", DistroFamily::Rocky),
            ("AlmaLinux-10-2-x86_64-dvd", DistroFamily::AlmaLinux),
        ] {
            assert_eq!(
                DistroFamily::detect(label, &[]),
                expected,
                "label `{label}` should detect as {expected:?}"
            );
        }
    }

    #[test]
    fn loose_needles_require_token_boundaries() {
        // "details" contains "tails"; "rockyou" contains "rocky". Neither
        // may classify; the real labels (token-delimited) still must.
        assert_eq!(
            DistroFamily::detect("project details disc", &[]),
            DistroFamily::Unknown
        );
        assert_eq!(
            DistroFamily::detect("rockyou wordlists", &[]),
            DistroFamily::Unknown
        );
        assert_eq!(DistroFamily::detect("TAILS 6.5", &[]), DistroFamily::Tails);
        assert_eq!(
            DistroFamily::detect("Rocky-10-1-x86_64-dvd", &[]),
            DistroFamily::Rocky
        );
    }

    #[test]
    fn root_markers_pin_slax_and_knoppix_even_with_renamed_labels() {
        assert_eq!(
            DistroFamily::detect("ANYTHING", &[dir("slax")]),
            DistroFamily::Slax
        );
        assert_eq!(
            DistroFamily::detect("ANYTHING", &[dir("knoppix")]),
            DistroFamily::Knoppix
        );
    }

    #[test]
    fn structural_fallback_classifies_renamed_isos() {
        // Unknown label but a casper/ directory → call it Ubuntu so the
        // user still gets a persistence offer.
        assert_eq!(
            DistroFamily::detect("CUSTOM_LIVE", &[dir("casper")]),
            DistroFamily::Ubuntu
        );
        assert_eq!(
            DistroFamily::detect("WEIRD_LIVE", &[dir("live")]),
            DistroFamily::Debian
        );
        assert_eq!(
            DistroFamily::detect("MY_ARCH", &[dir("arch")]),
            DistroFamily::Arch
        );
        // A `LiveOS/` directory (Fedora/RHEL dmsquash-live) without a known
        // label is treated as the generic Fedora family.
        assert_eq!(
            DistroFamily::detect("CUSTOM_RHEL", &[dir("LiveOS")]),
            DistroFamily::Fedora
        );
        assert_eq!(DistroFamily::detect("", &[]), DistroFamily::Unknown);
    }

    #[test]
    fn persistence_routing_matches_family() {
        assert_eq!(
            DistroFamily::Mint.persistence(),
            Some(PersistenceKind::CasperRw)
        );
        assert_eq!(
            DistroFamily::Lmde.persistence(),
            Some(PersistenceKind::DebianLive)
        );
        // Fedora-current (and derivatives) use the overlayfs mode; the RHEL
        // rebuilds keep the dm-snapshot COW file (older dracut).
        assert_eq!(
            DistroFamily::Bazzite.persistence(),
            Some(PersistenceKind::FedoraOverlayFs)
        );
        assert_eq!(
            DistroFamily::GeckoLinux.persistence(),
            Some(PersistenceKind::OpenSuseCow)
        );
        assert_eq!(
            DistroFamily::Manjaro.persistence(),
            Some(PersistenceKind::ArchOverlay)
        );
        assert_eq!(
            DistroFamily::Slax.persistence(),
            Some(PersistenceKind::SlaxChanges)
        );
        assert_eq!(
            DistroFamily::AlmaLinux.persistence(),
            Some(PersistenceKind::FedoraOverlay)
        );
        assert_eq!(
            DistroFamily::CentOs.persistence(),
            Some(PersistenceKind::FedoraOverlay)
        );
        // Alpine persists via lbu (inline), not an overlay partition.
        assert_eq!(
            DistroFamily::Alpine.persistence(),
            Some(PersistenceKind::AlpineLbu)
        );
        // Knoppix auto-adopts a KNOPPIX-DATA labelled partition.
        assert_eq!(
            DistroFamily::Knoppix.persistence(),
            Some(PersistenceKind::KnoppixData)
        );
        assert_eq!(DistroFamily::Unknown.persistence(), None);
    }

    #[test]
    fn slax_persistence_does_not_need_a_partition() {
        assert!(!PersistenceKind::SlaxChanges.needs_partition());
        assert!(!PersistenceKind::AlpineLbu.needs_partition());
        assert!(PersistenceKind::CasperRw.needs_partition());
        assert!(PersistenceKind::DebianLive.needs_partition());
        assert!(PersistenceKind::FedoraOverlay.needs_partition());
        assert!(PersistenceKind::OpenSuseCow.needs_partition());
        assert!(PersistenceKind::ArchOverlay.needs_partition());
    }
}
