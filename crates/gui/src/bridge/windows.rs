//! `AppController` methods for the Windows-ISO download dialog (a port of
//! Rufus's Fido) and assembling the installer customization.

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::QString;
use usbooty_core::WindowsSetup;

use super::helpers::{non_empty_opt, trimmed_opt};
use super::{JobHandle, qobject};

impl qobject::AppController {
    /// Fetch the language list for a Windows release (an index into
    /// [`crate::windisco::RELEASES`]).
    pub fn win_fetch_languages(mut self: core::pin::Pin<&mut Self>, version_index: i32) {
        if *self.busy() {
            return;
        }
        let Some(&(_, edition_id)) = crate::windisco::RELEASES.get(version_index.max(0) as usize)
        else {
            return;
        };
        self.as_mut().set_busy(true);
        self.as_mut().set_win_languages(QString::default());
        self.as_mut().set_win_options(QString::default());
        self.as_mut()
            .set_status(QString::from("Contacting Microsoft…"));

        let qt = self.qt_thread();
        std::thread::spawn(move || crate::runner::win_fetch_languages(qt, edition_id));
    }

    /// Fetch the download options for a previously-listed language.
    pub fn win_fetch_options(mut self: core::pin::Pin<&mut Self>, language_index: i32) {
        if *self.busy() || language_index < 0 {
            return;
        }
        let Some(catalog) = self.rust().win_catalog.clone() else {
            return;
        };
        self.as_mut().set_busy(true);
        self.as_mut().set_win_options(QString::default());
        self.as_mut()
            .set_status(QString::from("Fetching download options…"));

        let qt = self.qt_thread();
        std::thread::spawn(move || {
            crate::runner::win_fetch_options(qt, catalog, language_index as usize)
        });
    }

    /// Build a [`WindowsSetup`] from the current customization properties,
    /// applied during the partition-copy installer path via `autounattend.xml`.
    pub(crate) fn collect_windows_setup(&self) -> WindowsSetup {
        WindowsSetup {
            bypass_tpm: *self.bypass_tpm(),
            bypass_secureboot: *self.bypass_secureboot(),
            bypass_ram: *self.bypass_ram(),
            bypass_storage: *self.bypass_storage(),
            bypass_cpu: *self.bypass_cpu(),
            bypass_disk: *self.bypass_disk(),
            skip_msaccount: *self.skip_msaccount(),
            disable_network_during_oobe: *self.disable_network_during_oobe(),
            hide_wireless_setup: *self.hide_wireless_setup(),
            hide_oem_registration: *self.hide_oem_registration(),
            network_location_work: *self.network_location_work(),
            disable_telemetry: *self.disable_telemetry(),
            accept_eula: *self.accept_eula(),
            enable_dotnet35: *self.enable_dotnet35(),
            apply_debloat: *self.apply_debloat(),
            disable_bitlocker: *self.disable_bitlocker(),
            windows_ca_2023: *self.windows_ca_2023(),
            desktop_helpers: *self.desktop_helpers(),
            force_edition_picker: *self.force_edition_picker(),
            local_account: trimmed_opt(&self.local_account().to_string()),
            local_account_password: non_empty_opt(&self.local_account_password().to_string()),
            computer_name: trimmed_opt(&self.computer_name().to_string()),
            locale: trimmed_opt(&self.locale().to_string()),
            timezone: trimmed_opt(&self.timezone().to_string()),
            product_key: trimmed_opt(&self.product_key().to_string()),
            arch: trimmed_opt(&self.windows_arch().to_string()),
        }
    }

    /// Download a previously-listed Windows ISO option and select it.
    pub fn win_download(mut self: core::pin::Pin<&mut Self>, option_index: i32) {
        if *self.busy() || option_index < 0 {
            return;
        }
        let Some(option) = self.rust().win_option_list.get(option_index as usize) else {
            return;
        };
        let url = option.url.clone();
        self.as_mut().init_job_ui("Downloading Windows ISO…");

        let qt = self.qt_thread();
        let abort = Arc::new(AtomicBool::new(false));
        let abort_clone = abort.clone();
        std::thread::spawn(move || crate::runner::download_windows_url(qt, url, abort_clone));

        // Park a JobHandle so cancel() can reach the download; the
        // stdin slot stays empty because there is no helper to talk to.
        self.as_mut().rust_mut().job = Some(JobHandle {
            stdin: Arc::new(Mutex::new(None)),
            download_abort: Some(abort),
        });
    }

    /// Open Microsoft's official download page in the system browser: the
    /// reliable fallback when Microsoft's anti-bot system blocks the in-app
    /// query (common on VPNs and some ISPs).
    pub fn open_microsoft_page(&self, version_index: i32) {
        let url = if version_index == 1 {
            "https://www.microsoft.com/software-download/windows10"
        } else {
            "https://www.microsoft.com/software-download/windows11"
        };
        let _ = std::process::Command::new("xdg-open").arg(url).spawn();
    }
}
