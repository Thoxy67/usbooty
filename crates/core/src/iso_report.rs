//! The result of analyzing an ISO image.
//!
//! Produced by the GUI's `iso` module (which reads the ISO9660 filesystem
//! directly), consumed by [`crate::plan`] to decide a partition layout.

use serde::{Deserialize, Serialize};

/// The live-USB persistence scheme an ISO supports, if any.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PersistenceKind {
    /// Ubuntu / casper-based live systems — a `casper-rw` (or `writable`) partition.
    CasperRw,
    /// Debian-live — a `persistence` partition carrying a `persistence.conf` file.
    DebianLive,
    /// Fedora / RHEL-family live — an ext4 partition with the fixed label
    /// `OVERLAY` holding a sparse `overlay.img` COW file. dracut's dmsquash-live
    /// loop-mounts that file as a dm-snapshot when
    /// `rd.live.overlay=LABEL=OVERLAY:/overlay.img` is on the kernel command
    /// line (which we add when patching configs).
    FedoraOverlay,
    /// openSUSE live (kiwi-live) — an ext4 partition labelled `cow`, picked up
    /// automatically by the live system; no kernel parameter required.
    OpenSuseCow,
    /// archiso-based live systems (Arch Linux, CachyOS, etc.): an ext4 partition
    /// labelled `PERSISTENCE`. archiso turns it into the persistent cowspace
    /// when `cow_label=PERSISTENCE` is on the kernel command line (see the
    /// archiso `README.bootparams` `cow_label` / `cow_device` options).
    ArchOverlay,
    /// Slax — *no separate partition*. Slax 9+ persists into `/slax/changes/`
    /// at the data partition's root; the helper creates the directory and
    /// patches `perch` onto the kernel command line so the boot menu's
    /// "Persistent Changes" path is taken by default. Unlike every other
    /// variant the size slider is ignored — Slax just keeps writing into the
    /// folder until the partition fills.
    SlaxChanges,
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
/// `Unknown` is the polite default — usbooty still writes the ISO with the
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
    /// Slax — own `/slax/changes/` scheme.
    Slax,
    /// Knoppix — own scheme, very old isolinux defaults.
    Knoppix,
}

impl DistroFamily {
    /// Recognise the distribution from the ISO's volume label plus the names
    /// of files and directories at its root.
    ///
    /// Detection follows a most-specific-first cascade so a derivative
    /// (Bazzite, Nobara, Mint, LMDE, GeckoLinux) always wins over its parent
    /// (Fedora, Ubuntu, Debian, openSUSE). Root markers cover ISOs whose
    /// labels were customised away from the upstream default — e.g. a `slax/`
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

        // Root-marker overrides — these beat the label entirely, because
        // these distros put a hard-named directory at the ISO root.
        if has_dir("slax") || dir_starts_with("slax-") {
            return DistroFamily::Slax;
        }
        if dir_starts_with("knoppix") || has_dir("knoppix") {
            return DistroFamily::Knoppix;
        }

        // Label-based detection — most specific first. Needles are pre-lowered
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
            ("almalinux", DistroFamily::AlmaLinux),
            ("rockylinux", DistroFamily::Rocky),
            ("rocky", DistroFamily::Rocky),
            ("centos", DistroFamily::CentOs),
            ("alpine", DistroFamily::Alpine),
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
            if label_low.contains(needle) {
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
        }
    }

    /// The persistence scheme this family uses, if any. Returning `None`
    /// means usbooty doesn't yet know how to set up a writable overlay for
    /// this distro — the user is offered the DD method and no slider.
    pub fn persistence(self) -> Option<PersistenceKind> {
        match self {
            DistroFamily::Ubuntu | DistroFamily::Mint => Some(PersistenceKind::CasperRw),
            DistroFamily::Debian | DistroFamily::Lmde => Some(PersistenceKind::DebianLive),
            DistroFamily::Fedora
            | DistroFamily::Bazzite
            | DistroFamily::Nobara
            | DistroFamily::AlmaLinux
            | DistroFamily::Rocky
            | DistroFamily::CentOs => Some(PersistenceKind::FedoraOverlay),
            DistroFamily::OpenSuse | DistroFamily::GeckoLinux => Some(PersistenceKind::OpenSuseCow),
            DistroFamily::Arch
            | DistroFamily::Manjaro
            | DistroFamily::EndeavourOs
            | DistroFamily::CachyOs => Some(PersistenceKind::ArchOverlay),
            DistroFamily::Slax => Some(PersistenceKind::SlaxChanges),
            // Alpine "diskless" persists via lbu (an apkovl tarball on writable
            // media), an inline scheme rather than an overlay partition.
            DistroFamily::Alpine => Some(PersistenceKind::AlpineLbu),
            DistroFamily::Knoppix | DistroFamily::Unknown => None,
        }
    }
}

/// The kind of operating system an ISO contains.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OsKind {
    /// A Windows installation ISO (`sources/install.wim` + `bootmgr`/`setup.exe`).
    Windows,
    /// A Linux ISO (isolinux or GRUB present).
    Linux,
    /// A BSD ISO (FreeBSD / OpenBSD / NetBSD / …) — written with the DD method.
    Bsd,
    /// Something else — still writable with the DD method.
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
        ] {
            assert_eq!(
                DistroFamily::detect(label, &[]),
                expected,
                "label `{label}` should detect as {expected:?}"
            );
        }
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
        assert_eq!(
            DistroFamily::Bazzite.persistence(),
            Some(PersistenceKind::FedoraOverlay)
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
        assert_eq!(DistroFamily::Knoppix.persistence(), None);
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
