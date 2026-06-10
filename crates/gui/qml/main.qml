import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Dialogs
import QtQuick.Window
import com.usbooty

ApplicationWindow {
    id: window
    visible: true
    // Version gating for the Windows installer options, keyed off the loaded
    // ISO's install.wim build number (app.windowsBuild). 0 means unknown (parse
    // failed / not a Windows ISO), in which case we show the option rather than
    // hide something the user might need.
    readonly property bool isWin11: app.windowsBuild === 0 || app.windowsBuild >= 22000
    readonly property bool isWin11_22500: app.windowsBuild === 0 || app.windowsBuild >= 22500
    // Windows 11 24H2 is build 26100; the forced-local-account network trick is
    // only needed on 24H2+.
    readonly property bool isWin11_24H2: app.windowsBuild === 0 || app.windowsBuild >= 26100
    width: 660
    // Height is content-driven (see binding below). Keep a generous floor
    // so a freshly-launched window doesn't snap to a sliver while QML is
    // still computing its first layout pass.
    // 600 controls-column floor + 24 px RowLayout margins.
    minimumWidth: 624
    minimumHeight: 360
    // Auto-fit the window vertically to the left column's actual content:
    // hidden banners and the absent progress frame contribute zero height,
    // so the window shrinks/grows as the UI gains or loses sections.
    height: Math.max(minimumHeight,
                     mainCol.implicitHeight
                     + 24  // RowLayout top + bottom margins
                     + (menuBar ? menuBar.height : 0))
    Behavior on height {
        // No animation during a job: progress phases and banners would
        // trigger window resizes every few seconds, producing a visible
        // wobble. Idle resizes (toggling the log, switching method) keep
        // the eased transition.
        enabled: !app.busy
        NumberAnimation { duration: 120; easing.type: Easing.OutCubic }
    }
    // Title reflects the job state so progress is visible even when the
    // window is in the background / minimized to the taskbar.
    title: {
        if (app.busy && app.progress > 0)
            return "USBooty: " + Ui.trPhase(app.phase) + " "
                 + Math.round(app.progress * 100) + " %"
        if (app.busy)
            return "USBooty: " + Ui.trPhase(app.phase !== "" ? app.phase : "Working") + "…"
        return qsTr("USBooty: Bootable USB Creator")
    }

    AppController {
        id: app
        Component.onCompleted: {
            app.refreshDevices()
            app.applyStartupArgs()
        }
        // Reset the cancelling latch on every busy→idle transition so the
        // next job's Cancel button starts in the active state again.
        onBusyChanged: if (!app.busy) window.cancelling = false
        onJobFinished: function(success, message) {
            resultDialog.success = success
            resultDialog.message = message
            resultDialog.open()
        }
    }

    // Latched true between the moment the user clicks Cancel and the moment
    // the runner emits its terminal Done/Error. Drives the Cancel button's
    // label + enabled state so the user gets visible feedback instead of
    // wondering whether the click registered.
    property bool cancelling: false

    // Poll for new / removed block devices while idle. Cheap (one sysfs walk
    // every 2.5 s); skipped while a job runs so we don't churn the combo's
    // model mid-write.
    Timer {
        interval: 2500
        repeat: true
        running: !app.busy
        onTriggered: app.refreshDevices()
    }

    // Whether the user can launch a job right now. Format-only (method 2)
    // and FreeDOS (method 4) need no source image; Ventoy (method 3)
    // treats the ISO as optional.
    readonly property bool ready:
        !app.busy && app.selectedDevice >= 0
        && (app.method === 2
            || app.method === 4
            || (app.method === 3 && app.fitWarning === "")
            || (app.isoPath !== "" && app.fitWarning === ""))

    // A wall-clock elapsed counter, ticking once a second while a job runs.
    property int elapsedSecs: 0
    Timer {
        interval: 1000
        repeat: true
        running: app.busy
        onTriggered: window.elapsedSecs++
    }
    Connections {
        target: app
        function onBusyChanged() {
            if (app.busy)
                window.elapsedSecs = 0
        }
    }

    // trPhase / trMsg / fmtTime live in the Ui singleton (qml/Ui.qml) so the
    // dialogs and cards can reach them too; call them as Ui.trPhase(...) etc.

    // Reusable components (StepCard, WindowsLogo, LinuxLogo, Pill,
    // DialogHeader, FormCombo, WrapCheckBox, Banner, SplitButton) live in
    // qml/components/*.qml; same module, so they're usable by type name here.

    // ---- Expanding side-by-side layout ---------------------------------
    // The activity log lives on the right and only appears when there is
    // something to read. The first log line auto-widens the window so the
    // controls keep their full width and the log gets its own column,
    // less disruptive than pushing the action button off-screen.
    readonly property int compactWidth: 660
    readonly property int expandedWidth: 1080
    property bool logExpanded: false
    // Hard floor for the controls column; the user can never drag the
    // separator narrower than this.
    readonly property int leftPanelMinWidth: 600
    // Current width of the controls column. Bound to the separator drag;
    // the log column (Layout.fillWidth) absorbs whatever is left over.
    property int leftPanelWidth: compactWidth - 24
    Behavior on width {
        NumberAnimation { duration: 220; easing.type: Easing.OutCubic }
    }
    // The width the window had right before the log column forced it wider.
    // Collapsing the log restores exactly this (e.g. the launch width) rather
    // than a fixed constant, so the window returns to where it actually was.
    property int preLogWidth: width
    // Resize the window to `w`, but never while it is maximized or
    // fullscreen: the user chose that geometry, and a width tweak would
    // either be silently ignored or snap the window to the wrong size the
    // moment they restore it. The log column still shows/hides via
    // logVisible; only the windowed-mode auto-resize is suppressed.
    function setWindowWidth(w) {
        if (window.visibility === Window.Maximized
                || window.visibility === Window.FullScreen)
            return
        window.width = w
    }
    // Grow to fit the log column, remembering the current width first (read
    // fresh each time, so a manual resize between toggles is honoured). No-op
    // and no remembering if the window is already wide enough.
    function growForLog() {
        if (window.width < window.expandedWidth) {
            window.preLogWidth = window.width
            window.setWindowWidth(window.expandedWidth)
        }
    }
    // Collapse the log column, restoring the remembered pre-log width.
    function shrinkAfterLog() {
        window.setWindowWidth(window.preLogWidth)
    }
    // Effective visibility of the activity log column: forced on by the
    // user setting, OR auto-expanded once the buffer holds something.
    readonly property bool logVisible: app.showLogsAlways || logExpanded
    Connections {
        target: app
        function onLogNonEmptyChanged() {
            if (app.logNonEmpty && !window.logExpanded) {
                window.logExpanded = true
                window.growForLog()
            }
        }
        function onShowLogsAlwaysChanged() {
            if (app.showLogsAlways) {
                // Flipped on: grow to make room for the log column.
                window.growForLog()
            } else if (!window.logExpanded) {
                // Flipped off and nothing else is keeping the panel open
                // (no log content auto-expanded it): reclaim the width.
                window.shrinkAfterLog()
            }
        }
    }
    Component.onCompleted: {
        // Honour the persisted "always show logs" choice on first paint.
        if (app.showLogsAlways)
            window.growForLog()
    }


    menuBar: MenuBar {
        Menu {
            title: qsTr("Device")
            MenuItem {
                text: qsTr("Quick check (fake-drive)…")
                enabled: !app.busy && app.selectedDevice >= 0
                onTriggered: checkConfirm.openFor(0)
            }
            MenuItem {
                text: qsTr("Full bad-blocks scan…")
                enabled: !app.busy && app.selectedDevice >= 0
                onTriggered: checkConfirm.openFor(1)
            }
            MenuSeparator { }
            MenuItem {
                text: qsTr("Save snapshot to file…")
                enabled: !app.busy && app.selectedDevice >= 0
                onTriggered: backupDialog.open()
            }
            // Only offered when the boot test can actually run: QEMU is
            // installed and KVM virtualization is available on this host.
            MenuSeparator {
                visible: app.qemuAvailable && app.qemuKvm
            }
            MenuItem {
                text: qsTr("Verify boot device (QEMU)…")
                visible: app.qemuAvailable && app.qemuKvm
                enabled: !app.busy && app.selectedDevice >= 0
                onTriggered: bootTestDialog.open()
            }
        }
        Menu {
            title: qsTr("Settings")
            MenuItem {
                // Checkable so the active state is visible at-a-glance.
                // Useful on non-English desktops to force the canonical
                // English strings (e.g. for screenshots / bug reports).
                text: qsTr("Force English")
                checkable: true
                checked: app.forceEnglish
                onTriggered: app.applyForceEnglish(checked)
            }
            MenuItem {
                text: qsTr("Always show activity log")
                checkable: true
                checked: app.showLogsAlways
                onTriggered: app.applyShowLogsAlways(checked)
            }
            MenuItem {
                text: qsTr("Log every copied file")
                checkable: true
                checked: app.logAllFiles
                onTriggered: app.applyLogAllFiles(checked)
                ToolTip.visible: hovered
                ToolTip.text: qsTr("List every file copied to the target in the activity log, "
                    + "not just the large ones. Useful to see exactly what was written; "
                    + "produces a long log on big images.")
            }
        }
        Menu {
            title: qsTr("?")
            MenuItem {
                text: qsTr("Dependencies")
                onTriggered: {
                    depsDialog.refresh()
                    depsDialog.open()
                }
            }
            MenuItem {
                text: qsTr("About USBooty")
                onTriggered: aboutDialog.open()
            }
        }
    }

    // Two columns: controls on the left (always), activity log on the
    // right (only after the first log line, with the window auto-widening
    // to make space).
    RowLayout {
        anchors.fill: parent
        anchors.margins: 12
        spacing: 12

        ColumnLayout {
            id: mainCol
            // Sized to its content (the window's `height` binding tracks
            // this implicitHeight). Hugged to the top so the log column
            // can be taller without dragging the controls down.
            Layout.alignment: Qt.AlignTop
            // Width follows the separator drag; the log column fills the rest.
            // Never narrower than leftPanelMinWidth (enforced both here and in
            // the drag handler so the layout can't squeeze it below the floor).
            Layout.preferredWidth: window.leftPanelWidth
            Layout.minimumWidth: window.leftPanelMinWidth
            // Pin the width while the log is visible: extra window width must
            // go to the log column (Layout.fillWidth), never to the controls.
            // The separator drag is the only thing that widens this column.
            // With the log hidden there is no second column, so let it fill.
            Layout.maximumWidth: window.logVisible
                ? window.leftPanelWidth : Number.POSITIVE_INFINITY
            spacing: 8

        // ---- Advisory banners ------------------------------------------
        Banner {
            // Missing external tools.
            severity: "warn"
            message: app.depWarning
        }
        Banner {
            // The ISO cannot fit on the chosen drive. Blanked for the
            // methods that take no ISO (2 = Format only, 4 = FreeDOS),
            // matching can_start, which ignores the warning for both.
            severity: "error"
            message: (app.method === 2 || app.method === 4) ? "" : app.fitWarning
        }
        Banner {
            // SBAT / DBX revocation hits from scanning the ISO's EFI
            // binaries. Promoted to "error" so it stands out as a real
            // boot risk; modern Secure-Boot-enforcing firmware will
            // *refuse* to load a revoked bootloader. Legacy BIOS or
            // firmware with stale SbatLevel still boots, so it's a
            // warning the user can ignore deliberately.
            severity: "error"
            message: app.revocationWarnings
            ToolTip.delay: 500
            ToolTip.visible: hovered
            ToolTip.text: qsTr("USBooty scanned this ISO's signed EFI binaries against the "
                + "Secure Boot revocation database (SBAT generations + the live UEFI Forum "
                + "DBX update). One or more bootloaders are flagged as obsolete. UEFI firmware "
                + "with current revocations will refuse to load them. Try a newer ISO, or "
                + "boot in legacy / non-Secure-Boot mode.")
            // `hovered` is a Banner-level alias for the Label's MouseArea.
            property bool hovered: bannerHoverArea.containsMouse
            MouseArea {
                id: bannerHoverArea
                anchors.fill: parent
                hoverEnabled: true
                acceptedButtons: Qt.NoButton
            }
        }
        Banner {
            // SMART probe result for the selected device; populated
            // asynchronously after select_device.
            severity: "error"
            message: app.smartWarning
                ? qsTr("SMART: %1. Consider replacing this drive.").arg(app.smartWarning)
                : ""
        }

        // ---- Step 1: source image --------------------------------------
        StepCard {
            step: 1
            // Header reflects optionality so the user isn't blocked looking
            // for an ISO when they only want a plain format or a Ventoy
            // stick (which seeds itself empty if no ISO is given).
            heading: (app.method === 2 || app.method === 4)
                        ? qsTr("Source image (not used)")
                    : app.method === 3 ? qsTr("Source image (optional)")
                    : qsTr("Source image")
            accent: "#3498db"
            // Format-only and FreeDOS need no source image.
            enabled: app.method !== 2 && app.method !== 4

            RowLayout {
                Layout.fillWidth: true
                TextField {
                    id: isoField
                    Layout.fillWidth: true
                    readOnly: true
                    placeholderText:
                        app.method === 2 ? qsTr("Not used for a plain format")
                      : app.method === 4 ? qsTr("Not used: FreeDOS files are downloaded from upstream")
                      : app.method === 3 ? qsTr("Optional: Ventoy lets you drop ISOs onto the data partition later")
                      : qsTr("Choose an ISO image, or drag one onto the window…")
                    text: app.isoPath
                    ToolTip.delay: 500
                    ToolTip.visible: hovered && enabled
                    ToolTip.text: app.isoPath !== ""
                        ? app.isoPath
                        : qsTr("Drop an .iso / .img / .vhd / compressed image (.xz / .gz / .bz2 / "
                             + ".zst / .lzma / .zip / .Z) anywhere on the window, or use Browse… "
                             + "to pick one. Compressed and VHD images are unpacked into "
                             + "~/.cache/usbooty/ before writing.")
                }
                // Split button: "Browse…" plus a dropdown to download Windows.
                SplitButton {
                    id: sourceBtn
                    text: qsTr("Browse…")
                    enabled: !app.busy
                    onClicked: isoDialog.open()
                    onMenuRequested: sourceMenu.popup(sourceBtn, 0, sourceBtn.height)
                    Menu {
                        id: sourceMenu
                        MenuItem {
                            text: qsTr("Download a Windows ISO…")
                            onTriggered: winDialog.open()
                        }
                        MenuSeparator { }
                        MenuItem {
                            text: qsTr("Clear source image")
                            enabled: app.isoPath !== ""
                            onTriggered: app.clearIso()
                        }
                    }
                }
            }
            // ISO summary line with an OS-detection chip alongside it. The
            // chip surfaces the result of usbooty's ISO classification so the
            // user can confirm at a glance that their Windows / Linux ISO was
            // recognised (and thus the relevant options will be available).
            RowLayout {
                Layout.fillWidth: true
                spacing: 8
                Row {
                    spacing: 6
                    visible: app.windowsIso || app.linuxIso
                    WindowsLogo {
                        size: 14
                        visible: app.windowsIso
                        anchors.verticalCenter: parent.verticalCenter
                    }
                    Pill {
                        visible: app.windowsIso
                        label: qsTr("Windows")
                        tint: "#0078D4"
                    }
                    LinuxLogo {
                        size: 14
                        visible: app.linuxIso
                        anchors.verticalCenter: parent.verticalCenter
                    }
                    Pill {
                        visible: app.linuxIso
                        label: qsTr("Linux")
                        tint: "#E67E22"
                    }
                }
                Label {
                    text: Ui.trMsg(app.isoSummary)
                    color: palette.placeholderText
                    elide: Text.ElideMiddle
                    Layout.fillWidth: true
                }
            }
            ColumnLayout {
                // Hash computation is opt-in: streaming five hashers over a
                // multi-GiB ISO is CPU-heavy and disk-bound. Show a button
                // that triggers it on demand; once the hashes are filled in,
                // hide the button and expose the (selectable) values.
                visible: app.isoPath !== ""
                spacing: 4
                Layout.fillWidth: true

                readonly property bool anyHash: app.isoSha256 !== "" || app.isoMd5 !== ""
                    || app.isoSha1 !== "" || app.isoSha512 !== "" || app.isoBlake3 !== ""

                RowLayout {
                    Layout.fillWidth: true
                    visible: !app.hashing && !parent.anyHash
                    Button {
                        text: qsTr("Compute checksums")
                        enabled: !app.busy
                        onClicked: app.computeHashes()
                        ToolTip.delay: 500
                        ToolTip.visible: hovered
                        ToolTip.text: qsTr("Stream the ISO through MD5, SHA-1, SHA-256, SHA-512 and "
                            + "BLAKE3 in one pass. Disk-bound and CPU-heavy on a multi-GiB ISO; "
                            + "skip it unless you want to cross-check against a published hash.")
                    }
                    Label {
                        text: qsTr("Checksums skipped. Click to compute every digest.")
                        color: palette.placeholderText
                        font.pointSize: 9
                        elide: Text.ElideRight
                        Layout.fillWidth: true
                    }
                }

                // Computing: a single shared progress bar with the percentage.
                // The five hashes stream through one read pass and finish
                // together, so one bar is clearer than five spinners.
                RowLayout {
                    Layout.fillWidth: true
                    visible: app.hashing
                    Label {
                        text: qsTr("Computing checksums…")
                        color: palette.placeholderText
                        font.pointSize: 9
                    }
                    ProgressBar {
                        Layout.fillWidth: true
                        from: 0
                        to: 1
                        value: app.hashProgress
                    }
                    Label {
                        text: Math.round(app.hashProgress * 100) + " %"
                        color: palette.placeholderText
                        font.pointSize: 9
                    }
                }

                ColumnLayout {
                    visible: !app.hashing && parent.anyHash
                    spacing: 1
                    Layout.fillWidth: true

                    TextEdit {
                        text: "SHA-256:  " + app.isoSha256
                        readOnly: true
                        selectByMouse: true
                        wrapMode: TextEdit.WrapAnywhere
                        color: palette.placeholderText
                        font.family: "monospace"
                        font.pointSize: 8
                        Layout.fillWidth: true
                    }
                    TextEdit {
                        text: "SHA-1:    " + app.isoSha1
                        readOnly: true
                        selectByMouse: true
                        wrapMode: TextEdit.WrapAnywhere
                        color: palette.placeholderText
                        font.family: "monospace"
                        font.pointSize: 8
                        Layout.fillWidth: true
                    }
                    TextEdit {
                        text: "MD5:      " + app.isoMd5
                        readOnly: true
                        selectByMouse: true
                        wrapMode: TextEdit.WrapAnywhere
                        color: palette.placeholderText
                        font.family: "monospace"
                        font.pointSize: 8
                        Layout.fillWidth: true
                    }
                    TextEdit {
                        text: "SHA-512:  " + app.isoSha512
                        readOnly: true
                        selectByMouse: true
                        wrapMode: TextEdit.WrapAnywhere
                        color: palette.placeholderText
                        font.family: "monospace"
                        font.pointSize: 8
                        Layout.fillWidth: true
                    }
                    TextEdit {
                        text: "BLAKE3:   " + app.isoBlake3
                        readOnly: true
                        selectByMouse: true
                        wrapMode: TextEdit.WrapAnywhere
                        color: palette.placeholderText
                        font.family: "monospace"
                        font.pointSize: 8
                        Layout.fillWidth: true
                    }
                    // Green "verified" badge, shown only when the upstream
                    // SHA-1 database recognised this ISO.
                    Rectangle {
                        visible: app.isoAdguardBadge !== ""
                        Layout.fillWidth: true
                        Layout.topMargin: 4
                        color: "#1E7E34"
                        radius: 4
                        implicitHeight: badgeRow.implicitHeight + 8
                        RowLayout {
                            id: badgeRow
                            anchors.fill: parent
                            anchors.margins: 4
                            spacing: 6
                            Label {
                                text: "✓"
                                color: "white"
                                font.bold: true
                            }
                            Label {
                                Layout.fillWidth: true
                                text: qsTr("Verified: %1").arg(app.isoAdguardBadge)
                                color: "white"
                                wrapMode: Text.Wrap
                                font.pointSize: 9
                            }
                        }
                    }
                }
            }
        }

        // ---- Step 2: target device -------------------------------------
        StepCard {
            step: 2
            heading: qsTr("Target device")
            accent: "#E67E22"

            RowLayout {
                Layout.fillWidth: true
                FormCombo {
                    id: deviceBox
                    Layout.fillWidth: true
                    enabled: !app.busy && count > 0
                    model: app.devices.length > 0 ? app.devices.split("\n") : []
                    currentIndex: app.selectedDevice
                    onActivated: function(index) { app.selectDevice(index) }
                    displayText: count > 0 ? currentText
                                           : qsTr("No removable devices found")

                    // Two-line rows: hardware name above, capacity / bus /
                    // node below, with internal disks flagged in red.
                    delegate: ItemDelegate {
                        id: deviceDelegate
                        width: deviceBox.width
                        highlighted: deviceBox.highlightedIndex === index
                        // Split once per delegate, not per Label binding.
                        readonly property var deviceParts: modelData.split(" · ")
                        contentItem: ColumnLayout {
                            spacing: 1
                            Label {
                                text: deviceDelegate.deviceParts[0]
                                font.bold: true
                                elide: Text.ElideRight
                                Layout.fillWidth: true
                            }
                            Label {
                                text: deviceDelegate.deviceParts.length > 1
                                    ? deviceDelegate.deviceParts.slice(1).join(" · ")
                                    : ""
                                font.pointSize: 9
                                elide: Text.ElideRight
                                Layout.fillWidth: true
                                // Red token regardless of theme; internal-disk
                                // warnings should never blend into the row.
                                // Brightened a bit for dark themes via Qt.tint.
                                color: text.indexOf("Internal disk") >= 0
                                       ? Qt.tint(window.palette.windowText,
                                                 Qt.rgba(0.86, 0.21, 0.27, 0.85))
                                       : window.palette.placeholderText
                            }
                        }
                    }
                }
                Button {
                    text: qsTr("Refresh")
                    icon.name: "view-refresh"
                    display: icon.name
                        ? AbstractButton.IconOnly
                        : AbstractButton.TextOnly
                    enabled: !app.busy
                    onClicked: app.refreshDevices()
                    ToolTip.delay: 500
                    ToolTip.visible: hovered
                    ToolTip.text: qsTr("Re-scan /sys/block for connected drives. "
                        + "Usbooty already polls every few seconds while idle; "
                        + "use this if you just hotplugged a device and want it instantly.")
                }
            }
            WrapCheckBox {
                text: qsTr("Show non-removable (internal) disks")
                enabled: !app.busy
                checked: app.showFixedDisks
                onToggled: {
                    app.showFixedDisks = checked
                    app.refreshDevices()
                }
                ToolTip.delay: 500
                ToolTip.visible: hovered
                ToolTip.text: qsTr("Off by default. Internal SATA/NVMe disks are filtered out "
                    + "so they cannot be picked by mistake. Enable only when you really want "
                    + "to target a fixed disk (lab, dual-boot stick, image dump).")
            }
        }

        // ---- Step 3: options -------------------------------------------
        StepCard {
            step: 3
            heading: qsTr("Options")
            accent: "#16A085"

            GridLayout {
                Layout.fillWidth: true
                columns: 2
                columnSpacing: 12
                rowSpacing: 8

                Label { text: qsTr("Write method") }
                FormCombo {
                    Layout.fillWidth: true
                    enabled: !app.busy
                    model: [qsTr("DD image (raw copy)"),
                            qsTr("Partition & copy files"),
                            qsTr("Format only (no ISO)"),
                            qsTr("Ventoy (multi-boot USB)"),
                            qsTr("FreeDOS bootable USB")]
                    currentIndex: app.method
                    onActivated: function(index) { app.method = index }
                    ToolTip.delay: 500
                    ToolTip.visible: hovered
                    ToolTip.text: qsTr(
                        "DD: bit-for-bit copy of the ISO, no partitioning. Works for any "
                        + "isohybrid (most Linux ISOs).\n\n"
                        + "Partition & copy: USBooty creates a fresh partition table, formats it, "
                        + "and copies the ISO files. Required for Windows install media and for "
                        + "anything that needs persistence.\n\n"
                        + "Format only: wipe + new partition table, no ISO involved.\n\n"
                        + "Ventoy: install Ventoy so you can drop multiple ISOs on the data partition "
                        + "and pick one at boot.\n\n"
                        + "FreeDOS: download the latest FreeDOS kernel + shell from upstream and "
                        + "build a self-contained bootable DOS stick (no ISO needed). Useful for "
                        + "BIOS flashing utilities and legacy DOS tools.")
                }

                Label { text: qsTr("Filesystem") }
                FormCombo {
                    Layout.fillWidth: true
                    // The filesystem is chosen automatically when writing an
                    // image; it is only user-selectable for a plain format
                    // or for the FreeDOS method (which needs FAT16/FAT32).
                    enabled: !app.busy && (app.method === 2 || app.method === 4)
                    // Bound to the list of filesystems whose mkfs tools are
                    // installed on this host, keeps the user from picking a
                    // variant that would fail at format time. Filesystem
                    // names stay in their canonical form across every
                    // language, so no qsTr wrapping is needed.
                    model: app.availableFilesystems.length > 0
                        ? app.availableFilesystems.split("\n")
                        : ["FAT32"]
                    currentIndex: app.filesystem
                    onActivated: function(index) { app.filesystem = index }
                    ToolTip.visible: hovered && !enabled
                    ToolTip.text: qsTr("When writing an image, the filesystem is chosen automatically.")
                }

                Label { text: qsTr("Partition scheme") }
                FormCombo {
                    Layout.fillWidth: true
                    // Meaningful for the partition and format methods; a raw
                    // DD copy keeps the ISO's own embedded table.
                    enabled: !app.busy && app.method !== 0
                    // Order is mirrored by `filesystem_kind_from_index` on
                    // the Rust side; keep them aligned when adding entries.
                    model: [
                        qsTr("GPT (UEFI)"),
                        qsTr("MBR (BIOS)"),
                        qsTr("MBR (BIOS+UEFI)"),
                        qsTr("Hybrid MBR+GPT (BIOS+UEFI)")
                    ]
                    currentIndex: app.table
                    onActivated: function(index) { app.table = index }
                    ToolTip.delay: 500
                    ToolTip.visible: hovered
                    ToolTip.text: !enabled
                        ? qsTr("The DD method preserves the ISO's own partition table.")
                        : qsTr("How the disk is laid out for the firmware that boots it.\n\n"
                             + "• GPT (UEFI): modern default. Boots only on UEFI firmware. Required "
                             + "for disks larger than 2 TiB and for more than 4 partitions.\n\n"
                             + "• MBR (BIOS): legacy 1980s table. Boots only on BIOS / CSM. Pick this "
                             + "when the target PC's firmware truly is BIOS-only.\n\n"
                             + "• MBR (BIOS+UEFI): same on-disk layout as MBR, plus a bootable FAT "
                             + "partition with /EFI/BOOT/BOOTx64.EFI so UEFI firmware finds it via the "
                             + "fallback path. Simplest dual-firmware stick.\n\n"
                             + "• Hybrid MBR+GPT (BIOS+UEFI): real GPT + a synthesised MBR mirror "
                             + "of the data partition (Apple-style). Maximum compatibility, but some "
                             + "buggy firmwares dislike hybrid MBRs entirely. Use only if MBR(BIOS+UEFI) "
                             + "doesn't boot on a specific machine.")
                }

                // Ventoy names its own data partition, no label field for it.
                Label {
                    text: qsTr("Volume label")
                    visible: app.method !== 3
                }
                TextField {
                    id: labelField
                    Layout.fillWidth: true
                    visible: app.method !== 3
                    enabled: !app.busy && app.method !== 0
                    placeholderText: qsTr("Drive label")
                    text: app.label
                    // onTextEdited (not onTextChanged) avoids a binding loop:
                    // it fires only for user edits, not the pre-fill above.
                    onTextEdited: app.label = text
                    ToolTip.delay: 400
                    ToolTip.visible: hovered && app.label !== ""
                    // Show the user *exactly* what will land on disk after
                    // each filesystem's length / case / charset trimming.
                    ToolTip.text: {
                        if (!enabled)
                            return qsTr("The label is sanitized to each filesystem's limits.")
                        var sanitized = app.sanitizedLabel()
                        if (sanitized === app.label)
                            return qsTr("Will be written as “%1” (fits the chosen filesystem).")
                                       .arg(sanitized)
                        return qsTr("Will be written as “%1” (trimmed for the chosen filesystem)")
                                       .arg(sanitized)
                    }
                }
            }

            WrapCheckBox {
                text: qsTr("Full format: erase the whole device first (slow)")
                // DD overwrites every sector anyway, and Ventoy does its own
                // partitioning and formatting.
                enabled: !app.busy && (app.method === 1 || app.method === 2)
                checked: app.fullFormat
                onToggled: app.fullFormat = checked
                ToolTip.delay: 500
                ToolTip.visible: hovered
                ToolTip.text: qsTr("Zeroes every sector before writing. Slow (tens of minutes "
                    + "on a 64 GB stick), but it wipes any prior partition layout / hidden "
                    + "partitions and gives a clean slate. The quick path skips this and "
                    + "only writes the new layout.")
            }

            WrapCheckBox {
                // Windows ISO with oversized install.wim, partition method.
                // The default is the UEFI:NTFS two-partition layout; ticking
                // this asks USBooty to split install.wim into <4 GiB chunks
                // via wimlib-imagex and keep a single FAT32 partition.
                visible: app.windowsIso && app.method === 1
                text: qsTr("Split install.wim onto FAT32 (needs wimlib-imagex): broader firmware support than UEFI:NTFS")
                enabled: !app.busy
                checked: app.splitWim
                onToggled: app.splitWim = checked
                ToolTip.delay: 500
                ToolTip.visible: hovered
                ToolTip.text: qsTr("Windows ISOs with install.wim larger than 4 GiB cannot live on a single "
                    + "FAT32 partition as-is. The default layout is UEFI:NTFS (a small FAT32 + a big NTFS "
                    + "partition with a signed UEFI loader). This alternative uses wimlib-imagex to split "
                    + "install.wim into install.swm chunks Windows Setup loads natively, leaving you with "
                    + "a single FAT32 partition that boots on more firmware.")
            }

            WrapCheckBox {
                text: qsTr("Verify after writing: read the data back and check it")
                // A plain format / Ventoy install writes no verifiable payload.
                enabled: !app.busy && app.method < 2
                checked: app.verify
                onToggled: app.verify = checked
                ToolTip.delay: 500
                ToolTip.visible: hovered
                ToolTip.text: qsTr("Re-reads the entire device after writing and compares it "
                    + "to a BLAKE3 hash captured during the write. Roughly doubles the job "
                    + "time but catches counterfeit / failing flash that silently corrupts data.")
            }

            // Ventoy options, only for the Ventoy write method.
            ColumnLayout {
                Layout.fillWidth: true
                visible: app.method === 3
                spacing: 4
                WrapCheckBox {
                    text: qsTr("Update an existing Ventoy install (keeps your ISOs)")
                    enabled: !app.busy
                    checked: app.ventoyUpdate
                    onToggled: app.ventoyUpdate = checked
                    ToolTip.delay: 500
                    ToolTip.visible: hovered
                    ToolTip.text: qsTr("Upgrade the Ventoy bootloader in-place. The existing data "
                        + "partition (with your ISOs) is preserved; only the small EFI partition "
                        + "and the Ventoy boot files get rewritten.")
                }
                WrapCheckBox {
                    text: qsTr("Secure Boot support")
                    enabled: !app.busy
                    checked: app.ventoySecureBoot
                    onToggled: app.ventoySecureBoot = checked
                    ToolTip.delay: 500
                    ToolTip.visible: hovered
                    ToolTip.text: qsTr("Install Ventoy with the Microsoft-signed shim so the stick "
                        + "boots on UEFI machines that have Secure Boot enabled. Off → smaller "
                        + "footprint and no MOK enrollment, but Secure Boot must be disabled.")
                }
                Label {
                    text: qsTr("Ventoy makes a USB you drop ISOs onto and boot directly. "
                             + "A loaded ISO above (optional) is copied onto it.")
                    color: palette.placeholderText
                    font.pointSize: 8
                    wrapMode: Text.Wrap
                    Layout.fillWidth: true
                }
            }

            // Persistence, only for Linux live ISOs that support it.
            // Partition-based variants show a size slider; inline-folder
            // variants (Slax) show a simple on/off checkbox because the
            // changes directory lives inside the main data partition.
            // The slider section also disappears when no device is plugged
            // in / selected; there is nothing meaningful to slide against
            // until the user picks the target drive.
            ColumnLayout {
                Layout.fillWidth: true
                visible: app.persistenceSupported
                         && app.method === 1
                         && (app.persistenceInline || (app.selectedDevice >= 0 && app.persistenceMaxMib > 0))
                spacing: 2
                Label {
                    visible: !app.persistenceInline
                    // Below 1 GiB: show MiB. At or above 1 GiB: GiB with one
                    // decimal place; snapping makes that decimal meaningful.
                    text: {
                        if (app.persistenceSize <= 0)
                            return qsTr("Persistent storage:  off")
                        if (app.persistenceSize < 1024)
                            return qsTr("Persistent storage:  %1 MiB").arg(app.persistenceSize)
                        return qsTr("Persistent storage:  %1 GiB")
                                   .arg((app.persistenceSize / 1024).toFixed(1))
                    }
                    font.bold: true
                }
                RowLayout {
                    Layout.fillWidth: true
                    spacing: 6
                    visible: !app.persistenceInline
                    Slider {
                        id: persistenceSlider
                        Layout.fillWidth: true
                        enabled: !app.busy && app.persistenceMaxMib > 0
                        from: 0
                        // Always exactly the room the selected device has
                        // left, recomputed by AppController whenever the
                        // device or ISO changes.
                        to: app.persistenceMaxMib
                        // 256 MiB steps below 1 GiB (fine-grained for small
                        // overlays), 512 MiB steps above (matches the displayed
                        // 0.5 GiB precision so the label never lies).
                        stepSize: value < 1024 ? 256 : 512
                        value: app.persistenceSize
                        onMoved: app.persistenceSize = value
                        ToolTip.visible: pressed
                        ToolTip.text: value <= 0 ? qsTr("Off")
                                    : value < 1024 ? qsTr("%1 MiB").arg(value)
                                    : qsTr("%1 GiB").arg((value / 1024).toFixed(1))
                    }
                    Button {
                        text: qsTr("Max")
                        enabled: !app.busy && app.persistenceMaxMib > 0
                        onClicked: app.persistenceSize = app.persistenceMaxMib
                        ToolTip.delay: 500
                        ToolTip.visible: hovered
                        ToolTip.text: qsTr("Set the overlay to fill the device: uses every byte "
                            + "the chosen drive has left after the ISO and a small partition-table margin.")
                    }
                }
                // Inline-folder persistence (currently Slax): a single
                // toggle. Non-zero `persistenceSize` is how the job builder
                // detects the request; the value itself is ignored.
                CheckBox {
                    visible: app.persistenceInline
                    enabled: !app.busy
                    text: qsTr("Enable persistent changes")
                    checked: app.persistenceSize > 0
                    onToggled: app.persistenceSize = checked ? 1 : 0
                    ToolTip.delay: 500
                    ToolTip.visible: hovered
                    ToolTip.text: qsTr("Persistence lives on the writable boot stick, with no "
                        + "separate partition. Slax saves to /slax/changes/ automatically; Alpine "
                        + "runs from RAM and persists with lbu, so run `lbu commit` inside Alpine to "
                        + "save the apkovl (an apk cache folder is prepared for you).")
                }
                Label {
                    text: app.persistenceInline
                        ? qsTr("Changes are saved to the writable boot stick; no separate "
                             + "partition is created.")
                        : qsTr("Keeps your files and settings across reboots of this live USB.")
                    color: palette.placeholderText
                    font.pointSize: 8
                    wrapMode: Text.Wrap
                    Layout.fillWidth: true
                }
                // The distro family usbooty matched, so the user can tell at
                // a glance why a particular scheme was selected.
                Label {
                    visible: app.distroLabel.length > 0
                    text: qsTr("Detected distribution: %1").arg(app.distroLabel)
                    color: palette.placeholderText
                    font.pointSize: 8
                    Layout.fillWidth: true
                }
            }

            // Linux ISO whose distribution has no partition-persistence
            // support. Distros that manage persistence themselves get a
            // specific explanation (keyed by persistenceNoteKey) instead of
            // the generic "isn't supported" line.
            Label {
                Layout.fillWidth: true
                visible: app.linuxIso && !app.persistenceSupported && app.method === 1
                text: {
                    if (app.persistenceNoteKey === "tails")
                        return qsTr("Tails manages its own encrypted Persistent Storage: create it from inside Tails after the first boot.")
                    if (app.persistenceNoteKey === "puppy")
                        return qsTr("Puppy Linux offers to create its save file on first shutdown; nothing to set up here.")
                    if (app.persistenceNoteKey === "antix")
                        return qsTr("antiX / MX Linux set up persistence from their own live boot menu (the Persist options) on first boot.")
                    return app.distroLabel.length > 0
                        ? qsTr("Persistent storage isn't supported for %1.").arg(app.distroLabel)
                        : qsTr("Persistent storage isn't supported for this distribution.")
                }
                color: palette.placeholderText
                font.pointSize: 8
                wrapMode: Text.Wrap
            }

            // Persistence is available but the user hasn't picked a device
            // yet; explain why the slider isn't there so it doesn't look
            // like the feature is missing.
            Label {
                Layout.fillWidth: true
                visible: app.persistenceSupported
                         && !app.persistenceInline
                         && app.method === 1
                         && (app.selectedDevice < 0 || app.persistenceMaxMib <= 0)
                text: app.selectedDevice < 0
                    ? qsTr("Plug in or select a target device to set the persistence size.")
                    : qsTr("The selected device has no room left for a persistence partition once the ISO is written.")
                color: palette.placeholderText
                font.pointSize: 8
                wrapMode: Text.Wrap
            }
        }

        // ---- Action -----------------------------------------------------
        Button {
            Layout.fillWidth: true
            Layout.preferredHeight: 44
            // While the user has clicked Cancel but the runner has not yet
            // acknowledged, show "Cancelling…" disabled, so a rage-click on
            // a non-responsive cancel doesn't leave the user guessing.
            text: window.cancelling ? qsTr("Cancelling…")
                                    : (app.busy ? qsTr("Cancel") : qsTr("Start"))
            highlighted: true
            font.bold: true
            enabled: !window.cancelling && (app.busy || window.ready)
            ToolTip.delay: 500
            ToolTip.visible: hovered
            ToolTip.text: app.busy
                ? qsTr("Ask the running helper to stop. The current sector finishes writing, then "
                     + "the partition table is left in whatever state the helper had got to. "
                     + "Expect a partially-written drive.")
                : (app.windowsIso && app.method === 1
                    ? qsTr("Opens the Windows-setup dialog first (TPM/Secure-Boot/RAM bypasses, "
                         + "local account, debloat, …); the actual write begins after you click OK there.")
                    : qsTr("Confirm and start writing. All data on the selected device is erased."))
            onClicked: {
                if (app.busy) {
                    window.cancelling = true
                    app.cancel()
                } else if (app.windowsIso && app.method === 1) {
                    // Windows installer, partition method: offer the setup
                    // options first. (DD is a raw copy; it cannot apply an
                    // autounattend.xml, so no setup dialog there.)
                    windowsSetupDialog.open()
                } else {
                    confirmDialog.open()
                }
            }
        }

        // ---- Progress ---------------------------------------------------
        Frame {
            id: progressFrame
            Layout.fillWidth: true
            visible: app.busy || app.progress > 0
            padding: 12
            // Lower-cased phase name, computed once and shared by the phase
            // colour below and the ProgressBar's indeterminate test.
            readonly property string phaseLower: app.phase.toLowerCase()
            // A colour for the active phase, so the user can tell at a glance
            // whether we are still writing or already verifying the bytes back.
            readonly property color phaseColor: {
                var p = progressFrame.phaseLower
                if (p.indexOf("verif") >= 0)  return "#27AE60" // green
                if (p.indexOf("flush") >= 0)  return "#F39C12" // amber
                if (p.indexOf("format") >= 0) return "#8E44AD" // violet
                if (p.indexOf("ventoy") >= 0) return "#16A085" // teal
                if (p.indexOf("download") >= 0) return "#0078D4" // Windows blue
                if (p.indexOf("decompress") >= 0) return "#9B59B6" // light violet
                return "#3498DB"                                 // default blue
            }
            background: Rectangle {
                radius: 8
                color: palette.base
                border.color: palette.mid
                // A thin coloured stripe along the top edge mirroring the
                // active phase. Reads as "this is currently a Writing pass"
                // even before the user looks at the chip below.
                Rectangle {
                    anchors.left: parent.left
                    anchors.right: parent.right
                    anchors.top: parent.top
                    height: 3
                    radius: parent.radius
                    color: progressFrame.phaseColor
                    opacity: app.busy ? 0.95 : 0.45
                }
            }
            contentItem: ColumnLayout {
                spacing: 8
                ProgressBar {
                    id: phaseBar
                    Layout.fillWidth: true
                    from: 0
                    to: 1
                    value: app.progress
                    // Some phases never emit progress events (wimlib split,
                    // mkfs, syslinux install, ext4 persistence creation):
                    // switch to the indeterminate animation while we're in
                    // them so the bar doesn't sit frozen for minutes.
                    indeterminate: app.busy
                        && (progressFrame.phaseLower.indexOf("splitting") >= 0
                            || progressFrame.phaseLower.indexOf("syslinux") >= 0
                            || progressFrame.phaseLower.indexOf("extlinux") >= 0
                            || progressFrame.phaseLower.indexOf("persistence") >= 0
                            || progressFrame.phaseLower.indexOf("formatting") >= 0)
                    // Re-tint the fill bar to match the phase. The Qt default
                    // contentItem is a Rectangle, so we override it cleanly.
                    contentItem: Item {
                        implicitHeight: 6
                        Rectangle {
                            visible: !phaseBar.indeterminate
                            width: phaseBar.visualPosition * parent.width
                            height: parent.height
                            radius: 3
                            color: progressFrame.phaseColor
                        }
                        // Indeterminate sweep: a 35 %-wide block ping-pongs
                        // across the track. Activated only while
                        // `phaseBar.indeterminate` is true.
                        Rectangle {
                            id: sweep
                            visible: phaseBar.indeterminate
                            width: parent.width * 0.35
                            height: parent.height
                            radius: 3
                            color: progressFrame.phaseColor
                            x: 0
                            SequentialAnimation on x {
                                running: phaseBar.indeterminate
                                loops: Animation.Infinite
                                NumberAnimation {
                                    from: 0
                                    to: sweep.parent.width - sweep.width
                                    duration: 1200
                                    easing.type: Easing.InOutQuad
                                }
                                NumberAnimation {
                                    from: sweep.parent.width - sweep.width
                                    to: 0
                                    duration: 1200
                                    easing.type: Easing.InOutQuad
                                }
                            }
                        }
                    }
                    background: Rectangle {
                        implicitHeight: 6
                        // palette.mid is a midtone that contrasts against
                        // both light (slightly dark) and dark (slightly
                        // light) Qt themes.
                        color: palette.mid
                        opacity: 0.4
                        radius: 3
                    }
                }
                RowLayout {
                    Layout.fillWidth: true
                    spacing: 8
                    Pill {
                        visible: app.phase !== ""
                        label: Ui.trPhase(app.phase)
                        tint: progressFrame.phaseColor
                    }
                    Label {
                        text: app.phase === "" ? qsTr("Working") : ""
                        font.bold: true
                        visible: text !== ""
                    }
                    Item { Layout.fillWidth: true }
                    Label {
                        text: app.progress > 0
                              ? Math.round(app.progress * 100) + " %" : ""
                        font.bold: true
                        color: progressFrame.phaseColor
                    }
                }
                // Live transfer statistics: speed · ETA · elapsed.
                Label {
                    Layout.fillWidth: true
                    visible: text !== ""
                    color: palette.placeholderText
                    text: {
                        var parts = []
                        if (app.speed !== "")
                            parts.push("⬆ " + app.speed)
                        if (app.eta !== "")
                            parts.push(qsTr("ETA %1").arg(app.eta))
                        if (app.busy) {
                            var e = Ui.fmtTime(window.elapsedSecs)
                            if (e !== "")
                                parts.push(qsTr("%1 elapsed").arg(e))
                        }
                        return parts.join("     ·     ")
                    }
                }
                Label {
                    text: Ui.trMsg(app.status)
                    color: palette.placeholderText
                    elide: Text.ElideRight
                    Layout.fillWidth: true
                }
            }
        }

        }

        // ---- Resizable separator ---------------------------------------
        // Drag to redistribute width between the controls column and the
        // log column. Only present while the log column itself is shown.
        Rectangle {
            id: logSplitter
            visible: window.logVisible
            Layout.fillHeight: true
            Layout.preferredWidth: 6
            // The grip line; brightens on hover/drag for affordance.
            color: (splitterMouse.containsMouse || splitterMouse.pressed)
                ? palette.highlight : palette.mid
            radius: 3
            // Keep the lower bound for the log column in one place: its own
            // minimum (340) plus the two RowLayout gaps and the splitter.
            readonly property int logMinWidth: 340
            MouseArea {
                id: splitterMouse
                anchors.fill: parent
                // Widen the hit area beyond the visible 6 px line so it is
                // easy to grab without enlarging the drawn separator.
                anchors.leftMargin: -4
                anchors.rightMargin: -4
                hoverEnabled: true
                cursorShape: Qt.SplitHCursor
                property real pressX: 0
                property int startWidth: 0
                onPressed: function(mouse) {
                    pressX = mapToItem(window.contentItem, mouse.x, mouse.y).x
                    startWidth = window.leftPanelWidth
                }
                onPositionChanged: function(mouse) {
                    if (!pressed)
                        return
                    var curX = mapToItem(window.contentItem, mouse.x, mouse.y).x
                    var proposed = startWidth + (curX - pressX)
                    // Upper bound: leave the log column at least logMinWidth.
                    // 54 = 24 outer margins + 2×12 RowLayout gaps + 6 splitter.
                    var maxWidth = window.width - 54 - logSplitter.logMinWidth
                    window.leftPanelWidth = Math.max(
                        window.leftPanelMinWidth,
                        Math.min(maxWidth, proposed))
                }
            }
        }

        // ---- Activity log (right column, lazy) -------------------------
        Frame {
            id: logFrame
            visible: window.logVisible
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.minimumWidth: 340
            padding: 10
            background: Rectangle {
                radius: 8
                color: palette.base
                border.color: palette.mid
            }
            // The TextArea renders a potentially large RichText document.
            // Don't build it until the panel is actually shown; when the user
            // collapses the log we unload it again (it repopulates from
            // app.logHtmlSnapshot() on the next load).
            contentItem: Loader {
                active: window.logVisible
                sourceComponent: ColumnLayout {
                spacing: 6
                RowLayout {
                    Layout.fillWidth: true
                    Label {
                        text: qsTr("Activity log")
                        font.bold: true
                        Layout.fillWidth: true
                    }
                    Button {
                        // Freedesktop theme name: Breeze / Adwaita /
                        // Papirus all ship `document-save`. When the icon
                        // is available we hide the text label so the
                        // button stays compact; on icon-less themes the
                        // text appears as a fallback (Qt drops to
                        // TextOnly automatically when icon.source/name
                        // resolves to nothing).
                        text: qsTr("Save…")
                        icon.name: "document-save"
                        display: icon.name
                            ? AbstractButton.IconOnly
                            : AbstractButton.TextOnly
                        flat: true
                        enabled: app.logNonEmpty
                        onClicked: saveLogDialog.open()
                        ToolTip.delay: 500
                        ToolTip.visible: hovered
                        ToolTip.text: qsTr("Write the current activity log to a text file. Useful "
                            + "for bug reports. Attach the file instead of pasting in the panel.")
                    }
                    Button {
                        text: qsTr("Clear")
                        icon.name: "edit-clear"
                        display: icon.name
                            ? AbstractButton.IconOnly
                            : AbstractButton.TextOnly
                        flat: true
                        enabled: app.logNonEmpty && !app.busy
                        ToolTip.delay: 500
                        ToolTip.visible: hovered
                        ToolTip.text: qsTr("Empty the activity log panel.")
                        onClicked: {
                            app.clearLog()
                            // Collapsing back also shrinks the window;
                            // the user explicitly asked for the screen
                            // estate back, so honour that. Unless they
                            // turned on "always show logs", in which
                            // case the panel stays put.
                            if (!app.showLogsAlways) {
                                window.logExpanded = false
                                window.shrinkAfterLog()
                            }
                        }
                    }
                }
                ScrollView {
                    id: logScroll
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    clip: true
                    ScrollBar.vertical.policy: ScrollBar.AlwaysOn
                    ScrollBar.horizontal.policy: ScrollBar.AlwaysOff

                    TextArea {
                        id: logArea
                        // Binding the width to the viewport makes lines wrap;
                        // without it the text runs off the side and the view
                        // never gains the height it needs to scroll.
                        width: logScroll.availableWidth
                        readOnly: true
                        wrapMode: TextArea.Wrap
                        font.family: "monospace"
                        font.pointSize: 9
                        placeholderText: qsTr("Job output will appear here.")
                        // RichText lets the runner colour warnings (amber),
                        // errors (red), and phase headers (blue/bold) via
                        // inline HTML; plain info lines stay the default colour.
                        textFormat: TextEdit.RichText
                        // The log buffer lives Rust-side and is streamed line by
                        // line via `appendLogHtml` (so appends stay cheap and the
                        // whole document is never re-parsed). This panel is in a
                        // Loader that unloads when collapsed, so on (re)load we
                        // repopulate from the snapshot, then append live lines.
                        Component.onCompleted: text = app.logHtmlSnapshot()
                        Connections {
                            target: app
                            function onAppendLogHtml(html) {
                                logArea.append(html)
                            }
                            function onLogNonEmptyChanged() {
                                if (!app.logNonEmpty)
                                    logArea.clear()
                            }
                        }
                        // Smart autoscroll: only snap to the bottom when the
                        // user was already at (or near) the bottom before the
                        // append, so scrolling up to inspect an earlier line
                        // is not yanked back by every new log entry.
                        property bool followTail: true
                        onContentHeightChanged: {
                            if (followTail)
                                cursorPosition = length
                        }
                        Connections {
                            target: logScroll.ScrollBar.vertical
                            function onPositionChanged() {
                                var sb = logScroll.ScrollBar.vertical
                                // "At the bottom" means within 2 % of the end;
                                // small margin so a near-bottom scroll counts.
                                logArea.followTail =
                                    sb.position + sb.size >= 0.98
                            }
                        }
                    }
                }
                }
            }
        }
    }

    // ---- Drag-and-drop overlay -----------------------------------------
    // Predicate shared with the file dialog: accepts plain disk images and
    // any of the compressed variants the decompressor recognises.
    function looksLikeImage(url) {
        var u = url.toString().toLowerCase()
        // The compressed suffixes subsume their `.iso.xz`-style spellings.
        var suffixes = [".iso", ".img", ".vhd",
                        ".xz", ".gz", ".bz2", ".zst", ".lzma", ".zip", ".z"]
        return suffixes.some(function(s) { return u.endsWith(s) })
    }
    DropArea {
        id: isoDrop
        anchors.fill: parent
        // Name of the first file the user is dragging, used to make the
        // overlay feel responsive. Cleared when the drag leaves.
        property string hoverName: ""
        property bool hoverIsoLike: false
        onEntered: function(drag) {
            if (drag.hasUrls && drag.urls.length > 0) {
                var u = drag.urls[0].toString()
                hoverName = u.substring(u.lastIndexOf("/") + 1)
                // Same predicate the drop handler uses, so the overlay's
                // verdict never contradicts what releasing would do.
                hoverIsoLike = looksLikeImage(drag.urls[0])
            }
        }
        onExited: { hoverName = ""; hoverIsoLike = false }
        onDropped: function(drop) {
            hoverName = ""; hoverIsoLike = false
            if (app.busy || !drop.hasUrls)
                return
            if (looksLikeImage(drop.urls[0]))
                app.setIso(drop.urls[0])
        }
        Rectangle {
            anchors.fill: parent
            anchors.margins: 6
            visible: isoDrop.containsDrag && !app.busy
            // Tint the system base colour with the highlight hue so the
            // overlay reads well on both light and dark themes. A red tint
            // signals "I won't accept this file" for non-ISO drops.
            color: {
                var c = isoDrop.hoverIsoLike
                    ? palette.highlight
                    : Qt.rgba(0.86, 0.21, 0.27, 1.0)
                return Qt.tint(palette.base, Qt.rgba(c.r, c.g, c.b, 0.18))
            }
            border.color: isoDrop.hoverIsoLike ? palette.highlight
                                               : Qt.rgba(0.86, 0.21, 0.27, 1.0)
            border.width: 2
            radius: 10
            ColumnLayout {
                anchors.centerIn: parent
                spacing: 4
                Label {
                    Layout.alignment: Qt.AlignHCenter
                    text: isoDrop.hoverIsoLike ? qsTr("Drop to load")
                                               : qsTr("Unsupported file")
                    font.pointSize: 18
                    font.bold: true
                    color: isoDrop.hoverIsoLike ? palette.highlight
                                                : Qt.rgba(0.86, 0.21, 0.27, 1.0)
                }
                Label {
                    Layout.alignment: Qt.AlignHCenter
                    visible: isoDrop.hoverName !== ""
                    text: isoDrop.hoverName
                    font.pointSize: 11
                    color: palette.windowText
                }
                Label {
                    Layout.alignment: Qt.AlignHCenter
                    visible: !isoDrop.hoverIsoLike && isoDrop.hoverName !== ""
                    text: qsTr("Only disk images (.iso, .img, .vhd), plain or compressed, are accepted")
                    font.pointSize: 9
                    color: palette.placeholderText
                }
            }
        }
    }

    // ---- Dialogs --------------------------------------------------------
    FileDialog {
        id: isoDialog
        title: qsTr("Select an ISO image")
        nameFilters: [
            qsTr("Disk images (*.iso *.img *.vhd *.iso.xz *.iso.gz *.iso.bz2 *.iso.zst *.iso.lzma *.iso.zip *.iso.Z *.img.xz *.img.gz *.img.bz2 *.img.zst *.img.lzma *.img.zip *.img.Z)"),
            qsTr("Compressed (*.xz *.gz *.bz2 *.zst *.lzma *.zip *.Z)"),
            qsTr("All files (*)")
        ]
        onAccepted: app.setIso(selectedFile)
    }

    FileDialog {
        // Snapshot output. The extension picks the compressor; the helper
        // resolves it. `.img` / no extension → raw.
        id: backupDialog
        title: qsTr("Save device snapshot")
        fileMode: FileDialog.SaveFile
        defaultSuffix: "img"
        nameFilters: [
            qsTr("Raw images (*.img)"),
            qsTr("Compressed images (*.img.gz *.img.xz *.img.zst *.img.bz2)"),
            qsTr("All files (*)")
        ]
        onAccepted: app.startBackup(selectedFile)
    }

    FileDialog {
        // Save the activity-log buffer to a user-chosen text file.
        id: saveLogDialog
        title: qsTr("Save activity log")
        fileMode: FileDialog.SaveFile
        defaultSuffix: "log"
        nameFilters: [qsTr("Log files (*.log *.txt)"), qsTr("All files (*)")]
        onAccepted: app.saveLogTo(selectedFile)
    }

    // ---- Modal dialogs -------------------------------------------------
    // Each lives in qml/dialogs/*.qml and receives the AppController (app)
    // and the root window (host). Cross-dialog actions (one dialog opening
    // another) are surfaced as signals and wired together here.
    CheckConfirmDialog { id: checkConfirm; app: app; host: window }
    WindowsSetupDialog {
        id: windowsSetupDialog
        app: app
        host: window
        // Continue… advances to the erase-confirmation dialog.
        onContinued: { windowsSetupDialog.close(); confirmDialog.open() }
    }
    EraseConfirmDialog {
        id: confirmDialog
        app: app
        host: window
        // "Inspect device details…" opens the read-only inspect panel.
        onInspectRequested: inspectDialog.open()
    }
    InspectDialog { id: inspectDialog; app: app; host: window }
    DepsDialog { id: depsDialog; app: app; host: window }
    BootTestDialog { id: bootTestDialog; app: app; host: window }
    ResultDialog { id: resultDialog; app: app; host: window }
    AboutDialog { id: aboutDialog; app: app; host: window }
    WindowsDownloadDialog { id: winDialog; app: app; host: window }
}
