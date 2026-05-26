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
    /// Fedora live — an ext4 partition whose label matches the ISO's volume
    /// label with an `-Live-overlay` suffix. dracut detects it at boot via the
    /// `rd.live.overlay` kernel parameter (which we add when patching configs).
    FedoraOverlay,
    /// openSUSE live (kiwi-live) — an ext4 partition labelled `cow`, picked up
    /// automatically by the live system; no kernel parameter required.
    OpenSuseCow,
    /// archiso-based live systems (Arch Linux, CachyOS, …) — an ext4 partition
    /// labelled `PERSISTENCE`, activated by adding `cow_label=PERSISTENCE` to
    /// the kernel command line. See the Arch wiki article on the USB flash
    /// installation medium and Rufus issue #691 for the original write-up.
    ArchOverlay,
    /// Slax — *no separate partition*. Slax 9+ persists into `/slax/changes/`
    /// at the data partition's root; the helper creates the directory and
    /// patches `perch` onto the kernel command line so the boot menu's
    /// "Persistent Changes" path is taken by default. Unlike every other
    /// variant the size slider is ignored — Slax just keeps writing into the
    /// folder until the partition fills.
    SlaxChanges,
}

impl PersistenceKind {
    /// Whether this scheme needs its own dedicated partition. False for
    /// inline-directory variants like Slax, where the size slider is moot
    /// because persistence lives inside the main data partition.
    pub fn needs_partition(self) -> bool {
        !matches!(self, PersistenceKind::SlaxChanges)
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

        // Label-based detection — most specific first.
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
            ("ubuntu", DistroFamily::Ubuntu),
            ("kubuntu", DistroFamily::Ubuntu),
            ("xubuntu", DistroFamily::Ubuntu),
            ("lubuntu", DistroFamily::Ubuntu),
            ("debian", DistroFamily::Debian),
            ("fedora", DistroFamily::Fedora),
            ("fed_", DistroFamily::Fedora),
            ("opensuse", DistroFamily::OpenSuse),
            ("openSUSE", DistroFamily::OpenSuse),
            ("suse-", DistroFamily::OpenSuse),
            ("archlinux", DistroFamily::Arch),
            ("arch_", DistroFamily::Arch),
            ("arch-", DistroFamily::Arch),
        ] {
            if label_low.contains(&needle.to_ascii_lowercase()) {
                return family;
            }
        }

        // Structural fallback: if the ISO has a `casper/` directory but the
        // label didn't name an Ubuntu derivative, call it generic Ubuntu so
        // CasperRw persistence is still offered.
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
            DistroFamily::OpenSuse => "openSUSE",
            DistroFamily::GeckoLinux => "GeckoLinux",
            DistroFamily::Arch => "Arch Linux",
            DistroFamily::Manjaro => "Manjaro",
            DistroFamily::EndeavourOs => "EndeavourOS",
            DistroFamily::CachyOs => "CachyOS",
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
            DistroFamily::Fedora | DistroFamily::Bazzite | DistroFamily::Nobara => {
                Some(PersistenceKind::FedoraOverlay)
            }
            DistroFamily::OpenSuse | DistroFamily::GeckoLinux => {
                Some(PersistenceKind::OpenSuseCow)
            }
            DistroFamily::Arch
            | DistroFamily::Manjaro
            | DistroFamily::EndeavourOs
            | DistroFamily::CachyOs => Some(PersistenceKind::ArchOverlay),
            DistroFamily::Slax => Some(PersistenceKind::SlaxChanges),
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
        assert_eq!(DistroFamily::Knoppix.persistence(), None);
        assert_eq!(DistroFamily::Unknown.persistence(), None);
    }

    #[test]
    fn slax_persistence_does_not_need_a_partition() {
        assert!(!PersistenceKind::SlaxChanges.needs_partition());
        assert!(PersistenceKind::CasperRw.needs_partition());
        assert!(PersistenceKind::DebianLive.needs_partition());
        assert!(PersistenceKind::FedoraOverlay.needs_partition());
        assert!(PersistenceKind::OpenSuseCow.needs_partition());
        assert!(PersistenceKind::ArchOverlay.needs_partition());
    }
}
