import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import com.usbooty

Dialog {
    id: windowsSetupDialog
    // AppController + root window, passed from main.qml. `continued` fires
    // when the user presses Continue…; main.qml then advances to the erase
    // confirmation dialog.
    required property var app
    required property var host
    signal continued()
        anchors.centerIn: parent
        width: host.width - 60
        // Match the window height (minus ~80px of chrome for the dialog
        // header, footer, and outer margin). The inner ScrollView keeps
        // overflowing content scrollable, so on small displays the dialog
        // shrinks gracefully, and on tall displays it grows to show more
        // checkboxes at once instead of being capped at a fixed value.
        height: host.height - 80
        modal: true
        // Inset the contentItem from the dialog frame on every side; the
        // header and footer carry their own spacing.
        topPadding: 14
        bottomPadding: 14
        leftPadding: 18
        rightPadding: 18
        // Microsoft-blue header with the Windows flag immediately identifies
        // this as the Windows installer tweak panel.
        header: DialogHeader {
            tint: "#0078D4"
            iconComponent: WindowsLogo { size: 24; tint: "white" }
            title: qsTr("Windows setup")
            subtitle: qsTr("Optional install tweaks (written to autounattend.xml)")
        }
        // "Continue…" makes it explicit that this does not start writing yet;
        // it advances to the erase-confirmation dialog. Custom footer for the
        // same Qt 6 reason as the confirm dialog below.
        footer: DialogButtonBox {
            standardButtons: DialogButtonBox.Cancel
            Button {
                text: qsTr("Continue…")
                DialogButtonBox.buttonRole: DialogButtonBox.AcceptRole
                onClicked: windowsSetupDialog.continued()
            }
        }
        // The Windows-setup ScrollView contains ~25 checkboxes + text
        // fields and is only opened when the user starts a Windows ISO
        // job. Defer everything until visible, the biggest single win
        // in this file.
        contentItem: Loader {
            active: windowsSetupDialog.visible
            sourceComponent: ScrollView {
            id: setupScroll
            clip: true
            ScrollBar.vertical.policy: ScrollBar.AsNeeded
            ScrollBar.horizontal.policy: ScrollBar.AlwaysOff
            ColumnLayout {
                // Reserve room on the right so the vertical scrollbar never
                // sits on top of long checkbox labels or section dividers.
                width: setupScroll.availableWidth - 14
                spacing: 10
            Label {
                text: qsTr("Customize the installation below, or just press Continue to skip. "
                         + "Every option is optional.")
                color: palette.placeholderText
                wrapMode: Text.Wrap
                Layout.fillWidth: true
            }

            // --- Setup-time tweaks --------------------------------------
            // These act during Windows Setup.
            Label {
                text: qsTr("Setup"); font.bold: true; Layout.topMargin: 6
            }
            Rectangle {
                Layout.fillWidth: true; height: 1; color: palette.mid; opacity: 0.5
            }
            WrapCheckBox {
                // Win 11-only: these hardware checks don't exist on Windows 10.
                visible: host.isWin11
                text: qsTr("Bypass Windows 11 hardware checks: TPM, Secure Boot, RAM, Storage, CPU, Disk")
                checked: app.bypassTpm
                onToggled: {
                    app.bypassTpm = checked
                    app.bypassSecureboot = checked
                    app.bypassRam = checked
                    app.bypassStorage = checked
                    app.bypassCpu = checked
                    app.bypassDisk = checked
                }
                ToolTip.delay: 500
                ToolTip.visible: hovered
                ToolTip.text: qsTr("Lets Windows 11 install on hardware that fails its requirements check: "
                    + "no TPM 2.0, Secure Boot disabled, less than 8 GB RAM, system drive smaller than 64 GB, "
                    + "an older / non-allowlisted CPU, or unusual disk geometry. Sets the six LabConfig "
                    + "registry flags during Setup. Has no effect on Windows 10 (which doesn't check any of these).")
            }
            WrapCheckBox {
                text: qsTr("Auto-accept the Setup EULA")
                checked: app.acceptEula
                onToggled: app.acceptEula = checked
                ToolTip.delay: 500
                ToolTip.visible: hovered
                ToolTip.text: qsTr("Pre-clicks 'Accept' on the licence-agreement page that appears near "
                    + "the start of Windows Setup, so the install proceeds without waiting for the user to "
                    + "scroll and tick the box.")
            }
            WrapCheckBox {
                text: qsTr("Enable .NET Framework 3.5 from the install media")
                checked: app.enableDotnet35
                onToggled: app.enableDotnet35 = checked
                ToolTip.delay: 500
                ToolTip.visible: hovered
                ToolTip.text: qsTr("Installs the legacy .NET Framework 3.5 runtime alongside the modern "
                    + ".NET 4.x that Windows ships with by default. Many older desktop apps (games, accounting "
                    + "software, in-house tools from the 2000s) refuse to run without it. The files are pulled "
                    + "from the install media itself, so no internet is needed.")
            }
            RowLayout {
                Layout.fillWidth: true
                Label { text: qsTr("Product key"); Layout.minimumWidth: 110 }
                TextField {
                    Layout.fillWidth: true
                    // Qt sets implicitWidth from the placeholder; without
                    // a 0 minimum the field refuses to shrink below it
                    // and a long translated placeholder pushes the parent
                    // RowLayout past the dialog width.
                    Layout.minimumWidth: 0
                    placeholderText: qsTr("Optional, e.g. VK7JG-NPHTM-C97JM-9MPGT-3V66T (Win 11 Pro)")
                    text: app.productKey
                    onTextEdited: app.productKey = text
                }
                Button {
                    id: genericKeyButton
                    text: qsTr("Generic key")
                    // Display name for the detected ISO; the KMS client setup
                    // keys are identical between Windows 10 and 11 for matching
                    // editions, so only the label changes by version.
                    readonly property string winName: host.isWin11 ? "Windows 11" : "Windows 10"
                    // Microsoft's published KMS client setup keys (a.k.a.
                    // "generic" keys). They let Setup pick the edition to
                    // install without a paid key; they do NOT activate Windows.
                    readonly property var genericKeys: [
                        { edition: "Home",                 key: "TX9XD-98N7V-6WMQ6-BX7FG-H8Q99" },
                        { edition: "Home N",               key: "3KHY7-WNT83-DGQKR-F7HPR-844BM" },
                        { edition: "Home Single Language", key: "7HNRX-D7KGG-3K4RQ-4WPJ4-YTDFH" },
                        { edition: "Pro",                  key: "VK7JG-NPHTM-C97JM-9MPGT-3V66T" },
                        { edition: "Pro N",                key: "2B87N-8KFHP-DKV6R-Y2C8J-PKCKT" },
                        { edition: "Pro for Workstations", key: "DXG7C-N36C4-C4HTG-X4T3X-2YV77" },
                        { edition: "Pro Education",        key: "6TP4R-GNPTD-KYYHQ-7B7DP-J447Y" },
                        { edition: "Education",            key: "NW6C2-QMPVW-D7KKK-3GKT6-VCFB2" },
                        { edition: "Education N",          key: "2WH4N-8QGBV-H22JP-CT43Q-MDWWJ" },
                        { edition: "Enterprise",           key: "NPPR9-FWDCX-D2C8J-H872K-2YT43" },
                        { edition: "Enterprise N",         key: "DPH2V-TTNVB-4X9Q3-TJR4H-KHJW4" },
                    ]
                    onClicked: genericKeyMenu.popup(0, genericKeyButton.height)
                    ToolTip.delay: 500
                    ToolTip.visible: hovered
                    ToolTip.text: qsTr("Fill the field with a Microsoft generic key (KMS client "
                        + "setup key) so Setup installs the edition you pick. These choose the "
                        + "edition only, they do NOT activate Windows, activate separately "
                        + "afterwards. The list matches the loaded ISO (Windows 10 or 11); the "
                        + "keys themselves are the same across both versions.")
                    Menu {
                        id: genericKeyMenu
                        MenuItem {
                            // Non-clickable header so it's obvious which ISO the
                            // keys below belong to.
                            text: qsTr("%1 generic keys (install only)").arg(genericKeyButton.winName)
                            enabled: false
                        }
                        MenuSeparator { }
                        Repeater {
                            model: genericKeyButton.genericKeys
                            MenuItem {
                                required property var modelData
                                text: genericKeyButton.winName + " " + modelData.edition
                                onTriggered: app.productKey = modelData.key
                            }
                        }
                        MenuSeparator { }
                        MenuItem {
                            text: qsTr("Clear")
                            onTriggered: app.productKey = ""
                        }
                    }
                }
            }
            WrapCheckBox {
                text: qsTr("Force the edition picker at boot (OEM PCs)")
                checked: app.forceEditionPicker
                onToggled: app.forceEditionPicker = checked
                ToolTip.delay: 500
                ToolTip.visible: hovered
                ToolTip.text: qsTr("On an OEM PC with a firmware MSDM/SLIC key (typical of "
                    + "laptops sold with Windows Home Familiale pre-installed), Setup "
                    + "normally reads that key on boot and silently installs the matching "
                    + "edition. This option drops a sources/ei.cfg on the USB that tells "
                    + "Setup to ignore the firmware key, so you can pick a different "
                    + "edition (Pro, Enterprise, …) from Setup's built-in edition picker. "
                    + "Activation is a separate step. Install in the chosen edition first, "
                    + "then activate from inside Windows (e.g. with Microsoft Activation "
                    + "Scripts). Leave Product key empty above to get straight to the picker.")
            }

            // --- OOBE (first-boot) skips --------------------------------
            Label { text: qsTr("Out-of-box experience"); font.bold: true; Layout.topMargin: 6 }
            Rectangle { Layout.fillWidth: true; height: 1; color: palette.mid; opacity: 0.5 }

            // Sign-in: how first boot creates the user. The two options are
            // complementary fallbacks, not alternatives, so they stay separate
            // checkboxes (both may be on together): skip-MSA flips the OOBE
            // flags, disable-network is the harder 24H2+ workaround for when
            // those flags are silently ignored.
            Label { text: qsTr("Sign-in"); color: palette.placeholderText; Layout.topMargin: 2 }
            WrapCheckBox {
                text: qsTr("Skip Microsoft-account requirement (works on Win 10 and all Win 11)")
                checked: app.skipMsaccount
                onToggled: app.skipMsaccount = checked
                ToolTip.delay: 500
                ToolTip.visible: hovered
                ToolTip.text: qsTr("Lets you create a *local* Windows account during first-boot setup, "
                    + "instead of being forced to sign in with (or create) a Microsoft account. Works on "
                    + "every supported Windows version: Win 10, Win 11 pre-24H2, and Win 11 24H2+ all use "
                    + "different mechanisms, this option applies whichever one is needed.")
            }
            WrapCheckBox {
                // Win 11 24H2+ only: the forced-local-account network trick.
                visible: host.isWin11_24H2
                text: qsTr("Disable network during OOBE: force local account on Win 11 24H2+")
                checked: app.disableNetworkDuringOobe
                onToggled: app.disableNetworkDuringOobe = checked
                ToolTip.delay: 500
                ToolTip.visible: hovered
                ToolTip.text: qsTr("Hard-disables every network adapter for the duration of first-boot "
                    + "setup, so Windows physically can't reach Microsoft's servers to force online "
                    + "sign-in. Network is re-enabled automatically after the first sign-in. The most "
                    + "reliable local-account workaround on recent Win 11 builds where the regular "
                    + "'skip Microsoft account' flags are silently ignored.")
            }

            // Optional prompts: the Express button flips all four boxes below
            // at once; the indented boxes give finer control. A button (not a
            // checkbox) because it's an action, not a saved setting, its label
            // tracks whether the four are currently all on.
            Label { text: qsTr("Optional prompts"); color: palette.placeholderText; Layout.topMargin: 6 }
            Button {
                // Checked-state of the group: true only when all four are on.
                readonly property bool allOn: app.hideWirelessSetup && app.hideOemRegistration
                         && app.networkLocationWork && app.disableTelemetry
                text: allOn ? qsTr("Restore the four optional OOBE prompts")
                            : qsTr("Express: skip all four optional OOBE prompts")
                onClicked: {
                    var on = !allOn
                    app.hideWirelessSetup = on
                    app.hideOemRegistration = on
                    app.networkLocationWork = on
                    app.disableTelemetry = on
                }
                ToolTip.delay: 500
                ToolTip.visible: hovered
                ToolTip.text: qsTr("One switch for the four prompt-skip options below: hide the Wi-Fi "
                    + "screen, the OEM-registration screen, pre-answer the network type as Work, and "
                    + "skip the privacy / data-collection page. Toggle the individual boxes for finer control.")
            }
            WrapCheckBox {
                Layout.leftMargin: 16
                text: qsTr("Skip the \"connect to a network\" Wi-Fi screen")
                checked: app.hideWirelessSetup
                onToggled: app.hideWirelessSetup = checked
                ToolTip.delay: 500
                ToolTip.visible: hovered
                ToolTip.text: qsTr("Skips the 'Let's connect you to a network' page during first boot. "
                    + "Useful if the machine already has wired Ethernet (no Wi-Fi password to enter), or "
                    + "if you'd rather finish OOBE first and configure Wi-Fi inside Windows after.")
            }
            WrapCheckBox {
                Layout.leftMargin: 16
                text: qsTr("Hide the OEM-registration screen")
                checked: app.hideOemRegistration
                onToggled: app.hideOemRegistration = checked
                ToolTip.delay: 500
                ToolTip.visible: hovered
                ToolTip.text: qsTr("Skips the OEM-registration / product-activation pages that appear "
                    + "during first boot on factory-restore images (Dell, HP, Lenovo). Has no effect on "
                    + "clean Microsoft ISOs. There's no OEM page to hide.")
            }
            WrapCheckBox {
                Layout.leftMargin: 16
                text: qsTr("Pre-answer the network-type prompt as \"Work\" (private/trusted)")
                checked: app.networkLocationWork
                onToggled: app.networkLocationWork = checked
                ToolTip.delay: 500
                ToolTip.visible: hovered
                ToolTip.text: qsTr("Tells Windows the network you connect to during setup is "
                    + "private / trusted, no 'Is this a home, work or public network?' prompt. The result "
                    + "is the same firewall profile a home or office LAN gets: file sharing and network "
                    + "discovery enabled. Pick this on a LAN you control; skip it on cafés / hotels.")
            }
            WrapCheckBox {
                Layout.leftMargin: 16
                text: qsTr("Disable data-collection / telemetry prompts")
                checked: app.disableTelemetry
                onToggled: app.disableTelemetry = checked
                ToolTip.delay: 500
                ToolTip.visible: hovered
                ToolTip.text: qsTr("Pre-selects the most privacy-conscious answers on the OOBE "
                    + "'Choose privacy settings for your device' screen: minimum required diagnostic "
                    + "data, no inking & typing telemetry, no advertising ID, no tailored experiences, "
                    + "no Find-my-device. Equivalent to clicking 'No' on every toggle and submitting.")
            }

            // --- Local account ------------------------------------------
            // Name + optional password; a password also enables one-shot
            // auto-logon on first boot.
            Label {
                text: qsTr("Local account"); font.bold: true; Layout.topMargin: 6
            }
            Rectangle {
                Layout.fillWidth: true; height: 1; color: palette.mid; opacity: 0.5
            }
            RowLayout {
                Layout.fillWidth: true
                Label { text: qsTr("Name"); Layout.minimumWidth: 110 }
                TextField {
                    Layout.fillWidth: true
                    // Qt sets implicitWidth from the placeholder; without
                    // a 0 minimum the field refuses to shrink below it
                    // and a long translated placeholder pushes the parent
                    // RowLayout past the dialog width.
                    Layout.minimumWidth: 0
                    placeholderText: qsTr("Optional, leave empty to keep the OOBE prompt")
                    text: app.localAccount
                    onTextEdited: app.localAccount = text
                }
            }
            RowLayout {
                Layout.fillWidth: true
                Label { text: qsTr("Password"); Layout.minimumWidth: 110 }
                TextField {
                    Layout.fillWidth: true
                    // Qt sets implicitWidth from the placeholder; without
                    // a 0 minimum the field refuses to shrink below it
                    // and a long translated placeholder pushes the parent
                    // RowLayout past the dialog width.
                    Layout.minimumWidth: 0
                    placeholderText: qsTr("Optional, sets a password and enables one-shot auto-logon")
                    echoMode: TextInput.Password
                    text: app.localAccountPassword
                    onTextEdited: app.localAccountPassword = text
                }
            }
            WrapCheckBox {
                text: qsTr("Never expire the account password")
                checked: app.preventPasswordExpiration
                onToggled: app.preventPasswordExpiration = checked
                ToolTip.delay: 500
                ToolTip.visible: hovered
                ToolTip.text: qsTr("Sets the 'password never expires' flag on every local account "
                    + "at first logon, so Windows never forces a password change. Applies whether or "
                    + "not you set a password above, and to any account, including ones created later "
                    + "in OOBE. Handy for home PCs, kiosks and lab machines you don't want nagging for "
                    + "a new password.")
            }

            // --- System identity ----------------------------------------
            Label { text: qsTr("System"); font.bold: true; Layout.topMargin: 6 }
            Rectangle { Layout.fillWidth: true; height: 1; color: palette.mid; opacity: 0.5 }
            RowLayout {
                Layout.fillWidth: true
                Label { text: qsTr("Computer name"); Layout.minimumWidth: 110 }
                TextField {
                    Layout.fillWidth: true
                    // Qt sets implicitWidth from the placeholder; without
                    // a 0 minimum the field refuses to shrink below it
                    // and a long translated placeholder pushes the parent
                    // RowLayout past the dialog width.
                    Layout.minimumWidth: 0
                    placeholderText: qsTr("Optional, up to 15 characters, no whitespace")
                    text: app.computerName
                    onTextEdited: app.computerName = text
                }
                Button {
                    icon.name: "view-refresh"
                    display: icon.name
                        ? AbstractButton.IconOnly
                        : AbstractButton.TextOnly
                    text: qsTr("Random name")
                    onClicked: {
                        // PC- + 6 uppercase alphanumerics: 9 chars total,
                        // comfortably under the 15-char NETBIOS hostname
                        // limit and visually distinct from the Windows
                        // DESKTOP-* default.
                        var alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789"
                        var s = ""
                        for (var i = 0; i < 6; i++) {
                            s += alphabet.charAt(Math.floor(Math.random() * alphabet.length))
                        }
                        app.computerName = "PC-" + s
                    }
                    ToolTip.delay: 500
                    ToolTip.visible: hovered
                    ToolTip.text: qsTr("Generate a random PC-XXXXXX name. "
                        + "Useful when you don't care what the host is "
                        + "called and just want something unique.")
                }
            }
            RowLayout {
                Layout.fillWidth: true
                Label { text: qsTr("Locale"); Layout.minimumWidth: 110 }
                TextField {
                    Layout.fillWidth: true
                    // Qt sets implicitWidth from the placeholder; without
                    // a 0 minimum the field refuses to shrink below it
                    // and a long translated placeholder pushes the parent
                    // RowLayout past the dialog width.
                    Layout.minimumWidth: 0
                    placeholderText: qsTr("Optional, e.g. en-US, fr-FR, de-DE")
                    text: app.locale
                    onTextEdited: app.locale = text
                    ToolTip.delay: 500
                    ToolTip.visible: hovered
                    ToolTip.text: qsTr("Sets the system language, display language and regional "
                        + "format. List several entries separated by a comma or a space to add "
                        + "extra keyboard layouts, e.g. \"fr-FR, en-US\": the first one is the "
                        + "main language; every entry is added as a keyboard you can switch "
                        + "between with the language bar.")
                }
            }
            RowLayout {
                Layout.fillWidth: true
                Label { text: qsTr("Time zone"); Layout.minimumWidth: 110 }
                FormCombo {
                    id: timezoneCombo
                    Layout.fillWidth: true
                    // Parallel newline-separated lists built once in Rust from
                    // the Microsoft TimeZone catalog, sorted by UTC offset.
                    readonly property var tzIds: app.timezoneIds.split("\n")
                    model: app.timezoneLabels.split("\n")
                    // Restore the previously-chosen ID on dialog open, and
                    // re-sync whenever `app.timezone` changes (e.g. the
                    // 'Copy from system' button below).
                    Component.onCompleted: currentIndex = Math.max(0, tzIds.indexOf(app.timezone))
                    onActivated: app.timezone = tzIds[currentIndex] || ""
                    Connections {
                        target: app
                        function onTimezoneChanged() {
                            var i = timezoneCombo.tzIds.indexOf(app.timezone)
                            if (i >= 0) timezoneCombo.currentIndex = i
                        }
                    }
                }
            }
            RowLayout {
                Layout.fillWidth: true
                Layout.topMargin: 2
                Item { Layout.preferredWidth: 110 }
                Button {
                    text: qsTr("Copy from system")
                    onClicked: app.replicateRegionalFromHost()
                    ToolTip.delay: 500
                    ToolTip.visible: hovered
                    ToolTip.text: qsTr("Reads the host's $LANG and /etc/localtime, picks the "
                        + "matching Microsoft TimeZone ID, and fills the Locale + Time zone "
                        + "fields. Saves typing en-US / Pacific Standard Time by hand.")
                }
            }

            // --- Debloat ------------------------------------------------
            Label { text: qsTr("Privacy & debloat"); font.bold: true; Layout.topMargin: 6 }
            Rectangle { Layout.fillWidth: true; height: 1; color: palette.mid; opacity: 0.5 }
            WrapCheckBox {
                // 24H2+ only: automatic device encryption on a clean first boot
                // is a Windows 11 24H2 (build 26100) behavior.
                visible: host.isWin11_24H2
                text: qsTr("Disable automatic BitLocker device encryption")
                checked: app.disableBitlocker
                onToggled: app.disableBitlocker = checked
                ToolTip.delay: 500
                ToolTip.visible: hovered
                ToolTip.text: qsTr("Stops Windows 11 24H2+ from silently encrypting the system "
                    + "drive on first sign-in. Without this, recent installs auto-turn-on BitLocker "
                    + "and the user is never asked, leaving anyone who later mounts the disk from "
                    + "Linux or another Windows install staring at an unreadable partition. "
                    + "Recommended for dual-boot, lab, and IT-imaged systems.")
            }
            WrapCheckBox {
                text: qsTr("Install Windows CA 2023 Secure Boot policy")
                checked: app.windowsCa2023
                onToggled: app.windowsCa2023 = checked
                ToolTip.delay: 500
                ToolTip.visible: hovered
                ToolTip.text: qsTr("Lets older UEFI firmware boot recent Windows installers that "
                    + "are signed by Microsoft's new 'Windows UEFI CA 2023' chain. If your motherboard "
                    + "hasn't received the new CA via Windows Update yet (common on workstations / "
                    + "servers that don't run Windows), Secure-Boot will otherwise refuse the install. "
                    + "Needs wimlib-imagex on the host; the option silently no-ops on older Windows ISOs.")
            }
            WrapCheckBox {
                id: debloatBox
                // Imports a .reg from the USB media during the specialize pass.
                text: qsTr("Apply debloat profile")
                checked: app.applyDebloat
                onToggled: app.applyDebloat = checked
                ToolTip.delay: 500
                ToolTip.visible: hovered
                ToolTip.text: qsTr("Turns off the Windows 'features' most people uninstall by hand: "
                    + "Cortana voice assistant, Copilot, Recall (the AI screenshot history), the "
                    + "News & Interests taskbar widget, Bing / web suggestions in Start search, "
                    + "lockscreen ads + 'suggested' apps + suggested toast notifications, the "
                    + "advertising ID, tailored experiences, online speech model downloads, the "
                    + "Phone-Link prompt, and the Feedback-Hub frequency popups. Telemetry is "
                    + "knocked down to the minimum the OS still requires. Applied both machine-wide "
                    + "and for every new user account. Win-11-only entries silently no-op on Win 10. "
                    + "Click the box again to see the full per-item list.")
            }
            Label {
                visible: debloatBox.checked
                Layout.fillWidth: true
                Layout.leftMargin: 24
                wrapMode: Text.Wrap
                color: palette.placeholderText
                font.pointSize: 9
                textFormat: Text.RichText
                text: qsTr(
                    "<b>Applied machine-wide (HKLM Group Policy):</b><br>"
                    + "&nbsp;• News &amp; Interests feed (taskbar widget)<br>"
                    + "&nbsp;• Consumer-feature ads: suggested Store apps, OEM-style inserts<br>"
                    + "&nbsp;• Activity History sync to Microsoft<br>"
                    + "&nbsp;• Cortana in Search<br>"
                    + "&nbsp;• Windows Copilot service<br>"
                    + "&nbsp;• Windows Recall: the rolling-screenshot AI history (Win 11 24H2+)<br>"
                    + "&nbsp;• Diagnostic data: set to Required only<br>"
                    + "<br>"
                    + "<b>Applied to the default user profile (inherited by every new account):</b><br>"
                    + "&nbsp;• Bing / web suggestions in Start &amp; Search<br>"
                    + "&nbsp;• File extensions shown (instead of hidden)<br>"
                    + "&nbsp;• Copilot, Task View, Widgets and \"People\" buttons hidden from the taskbar<br>"
                    + "&nbsp;• Sync-provider ads in Explorer suppressed<br>"
                    + "&nbsp;• Start menu \"recommendations\" and Iris suggestions disabled<br>"
                    + "&nbsp;• ContentDeliveryManager: lock-screen rotation ads, pre-installed-app suggestions, \"subscribed content\" tiles<br>"
                    + "&nbsp;• Cortana / Bing inside per-user Search<br>"
                    + "&nbsp;• Advertising ID disabled<br>"
                    + "&nbsp;• \"Tailored experiences\" derived from diagnostic data<br>"
                    + "&nbsp;• \"Suggested\" toast notifications<br>"
                    + "&nbsp;• Phone Link / \"use your mobile with Windows\" prompts<br>"
                    + "&nbsp;• Online speech recognition (voice stays local)<br>"
                    + "&nbsp;• Contact harvesting for input personalization<br>"
                    + "&nbsp;• Feedback Hub frequency set to Never<br>"
                    + "&nbsp;• \"Finish setting up your device\" prompts<br>"
                    + "<br>"
                    + "Windows 11-only keys (Copilot, Widgets, News &amp; Interests, Recall) "
                    + "are silently ignored on Windows 10.")
            }

            // --- Post-install desktop helpers ---------------------------
            Label {
                text: qsTr("Post-install desktop helpers")
                font.bold: true
                Layout.topMargin: 6
            }
            Rectangle { Layout.fillWidth: true; height: 1; color: palette.mid; opacity: 0.5 }
            WrapCheckBox {
                id: desktopHelpersBox
                // Staged on the USB media and xcopied onto the Default user's
                // Desktop during the specialize pass.
                text: qsTr("Drop a USBooty folder on the user's Desktop with ready-to-run scripts")
                checked: app.desktopHelpers
                onToggled: app.desktopHelpers = checked
                ToolTip.delay: 500
                ToolTip.visible: hovered
                ToolTip.text: qsTr("After install, the user finds a USBooty folder on their "
                    + "Desktop with right-click → \"Run as administrator\" .bat scripts: "
                    + "Win11Debloat (Raphire), Chris Titus winutil (stable + dev), Microsoft "
                    + "Activation Scripts (Massgrave), an OneDrive remover, an OfficeTool "
                    + "downloader, one-click installers for Chocolatey, Scoop and winget, a "
                    + "Windows-AI stripper (Copilot / Recall / generative Paint+Notepad), "
                    + "Winhance, FR33THY's Ultimate gaming/latency tweaks, PowerToys, system "
                    + "tweaks (Fast Startup off, long paths on), VC++ Redistributables 2015-"
                    + "2022 and DirectX legacy runtimes, plus an interactive browser-installer "
                    + "menu (Chrome, Firefox, Brave, Zen, LibreWolf, Floorp, Waterfox, Opera, "
                    + "Opera GX, Vivaldi, Arc). The folder is copied to the Default user "
                    + "profile during Windows setup, so every account created at OOBE inherits "
                    + "it.")
            }
            Label {
                visible: desktopHelpersBox.checked
                Layout.fillWidth: true
                Layout.leftMargin: 24
                wrapMode: Text.Wrap
                color: palette.placeholderText
                font.pointSize: 9
                textFormat: Text.RichText
                // Backslashes in literal Windows paths confuse lupdate's
                // QML lexer (it eats `\U` from "C:\Users\" as an escape).
                // Use the &#x5C; HTML entity; RichText renders it as `\`.
                text: qsTr(
                    "Lands in <code>C:&#x5C;Users&#x5C;&lt;NewUser&gt;&#x5C;Desktop&#x5C;USBooty&#x5C;</code>, grouped into folders:<br>"
                    + "<br><b>Debloat &amp; Privacy</b><br>"
                    + "&nbsp;• <b>Win11Debloat</b>: Raphire's debloat (debloat.raphi.re)<br>"
                    + "&nbsp;• <b>ChrisTitus-Winutil</b>: Chris Titus winutil, stable channel<br>"
                    + "&nbsp;• <b>ChrisTitus-Winutil-Dev</b>: same tool, dev channel<br>"
                    + "&nbsp;• <b>Remove-OneDrive</b>: kill + uninstall OneDrive (x64 &amp; WoW64)<br>"
                    + "&nbsp;• <b>Remove-Windows-AI</b>: strip Copilot / Recall / AI features (zoicware)<br>"
                    + "&nbsp;• <b>Winhance</b>: debloat / privacy / optimise GUI<br>"
                    + "<br><b>Tweaks &amp; Performance</b><br>"
                    + "&nbsp;• <b>FR33THY-Ultimate</b>: gaming / latency tweaks (aggressive)<br>"
                    + "&nbsp;• <b>Disable-FastStartup</b>: clear HiberbootEnabled (dual-boot fix)<br>"
                    + "&nbsp;• <b>Enable-LongPaths</b>: set LongPathsEnabled=1 (developer)<br>"
                    + "&nbsp;• <b>Disable-GameBar-GameDVR</b>: stop background game recording<br>"
                    + "&nbsp;• <b>Enable-GPU-Scheduling</b>: hardware-accelerated GPU scheduling<br>"
                    + "&nbsp;• <b>Enable-Ultimate-Performance</b>: unlock the Ultimate power plan<br>"
                    + "&nbsp;• <b>Disable-Hibernation</b>: powercfg -h off (frees disk)<br>"
                    + "&nbsp;• <b>Enable-GodMode</b>: All-Tasks folder on the Desktop<br>"
                    + "&nbsp;• <b>Restore-Classic-ContextMenu</b>: full Win10 right-click menu<br>"
                    + "<br><b>Install Apps</b><br>"
                    + "&nbsp;• <b>OfficeTool</b>: download OfficeTool Plus runtime<br>"
                    + "&nbsp;• <b>Install-PowerToys</b>: Microsoft PowerToys via winget<br>"
                    + "&nbsp;• <b>Install-VCRedist</b>: VC++ Redistributable 2015-2022, x64 + x86<br>"
                    + "&nbsp;• <b>Install-DirectX</b>: legacy DirectX runtime (older games)<br>"
                    + "&nbsp;• <b>Install-Browser</b>: menu of 11 browsers (Chrome, Firefox, Brave, …)<br>"
                    + "&nbsp;• <b>Install-DotNet-Runtimes</b>: .NET Desktop Runtime 8 + 9<br>"
                    + "&nbsp;• <b>Install-ExplorerPatcher</b>: latest ExplorerPatcher, x64 / ARM64 (Win 10 taskbar on 11)<br>"
                    + "<br><b>Package Managers</b><br>"
                    + "&nbsp;• <b>Install-Chocolatey</b>: Chocolatey (machine-wide, admin)<br>"
                    + "&nbsp;• <b>Install-Scoop</b>: Scoop (per-user, no admin)<br>"
                    + "&nbsp;• <b>Install-Winget</b>: install / repair winget (asheroto)<br>"
                    + "<br><b>Activation</b><br>"
                    + "&nbsp;• <b>Massgravel-Activator</b>: Microsoft Activation Scripts (MAS)<br>"
                    + "<br>"
                    + "The debloat suites, installers and activator fetch code from the internet "
                    + "on first run; the tweak scripts only change local settings.")
            }

            // --- Tweaks -------------------------------------------------
            // Optional quality-of-life registry tweaks, baked into the
            // default user profile (and HKLM) during the specialize pass so
            // the created account gets them on first sign-in. All off by
            // default.
            Label { text: qsTr("Tweaks"); font.bold: true; Layout.topMargin: 6 }
            Rectangle { Layout.fillWidth: true; height: 1; color: palette.mid; opacity: 0.5 }
            WrapCheckBox {
                text: qsTr("Show known file extensions in Explorer")
                checked: app.showFileExtensions
                onToggled: app.showFileExtensions = checked
                ToolTip.delay: 500
                ToolTip.visible: hovered
                ToolTip.text: qsTr("Turns off 'Hide extensions for known file types', so Explorer "
                    + "shows .exe, .txt, .docx and the like. Applied to the default user profile, so "
                    + "the account created at first boot already has it on.")
            }
            WrapCheckBox {
                text: qsTr("Show hidden files in Explorer")
                checked: app.showHiddenFiles
                onToggled: app.showHiddenFiles = checked
                ToolTip.delay: 500
                ToolTip.visible: hovered
                ToolTip.text: qsTr("Sets Explorer to show hidden files and folders. Does not reveal "
                    + "protected operating-system files (that's a separate, riskier toggle). Applied to "
                    + "the default user profile.")
            }
            WrapCheckBox {
                // The classic-menu CLSID trick only matters on Win 11, which
                // introduced the trimmed 'Show more options' command bar.
                visible: host.isWin11
                text: qsTr("Restore the classic right-click context menu (Windows 11)")
                checked: app.classicContextMenu
                onToggled: app.classicContextMenu = checked
                ToolTip.delay: 500
                ToolTip.visible: hovered
                ToolTip.text: qsTr("Brings back the full Windows 10 right-click menu instead of the "
                    + "trimmed Windows 11 one that hides most entries behind 'Show more options'. "
                    + "Applied to the default user profile's class store.")
            }
            WrapCheckBox {
                text: qsTr("Use the dark theme by default")
                checked: app.darkMode
                onToggled: app.darkMode = checked
                ToolTip.delay: 500
                ToolTip.visible: hovered
                ToolTip.text: qsTr("Defaults both the app and system UI to the dark theme for the "
                    + "created account, instead of the out-of-the-box light theme. Purely cosmetic; "
                    + "switch it back any time in Settings > Personalization.")
            }
            WrapCheckBox {
                text: qsTr("Disable Fast Startup (hybrid shutdown)")
                checked: app.disableFastStartup
                onToggled: app.disableFastStartup = checked
                ToolTip.delay: 500
                ToolTip.visible: hovered
                ToolTip.text: qsTr("Clears HiberbootEnabled so 'Shut down' performs a real, full "
                    + "shutdown instead of saving the kernel to a hibernation file. The standard fix "
                    + "for dual-boot setups where Windows otherwise locks the disks, and for machines "
                    + "that won't cleanly power off. Machine-wide (HKLM).")
            }
            }
        }
        }
    }
