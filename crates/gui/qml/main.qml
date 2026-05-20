import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Dialogs
import com.usbooty

ApplicationWindow {
    id: window
    visible: true
    width: 660
    height: 860
    minimumWidth: 600
    minimumHeight: 620
    title: "usbooty — Bootable USB Creator"

    AppController {
        id: app
        Component.onCompleted: app.refreshDevices()
        onJobFinished: function(success, message) {
            resultDialog.title = success ? "Success" : "Failed"
            resultDialog.success = success
            resultLabel.text = message
            resultDialog.open()
        }
    }

    // Whether the user can launch a job right now. Format-only (method 2)
    // needs no source image; Ventoy (method 3) treats the ISO as optional.
    readonly property bool ready:
        !app.busy && app.selectedDevice >= 0
        && (app.method === 2
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

    // Format a second count as "1h 04m", "2m 12s" or "38s".
    function fmtTime(s) {
        if (s <= 0)
            return ""
        var h = Math.floor(s / 3600)
        var m = Math.floor((s % 3600) / 60)
        var sec = s % 60
        if (h > 0)
            return h + "h " + ("0" + m).slice(-2) + "m"
        if (m > 0)
            return m + "m " + ("0" + sec).slice(-2) + "s"
        return sec + "s"
    }

    // ---- Reusable numbered section card --------------------------------
    component StepCard: Frame {
        id: card
        property int step: 0
        property string heading: ""
        // Per-step accent — colours the round step badge and the card's left
        // edge so the three cards have a quick visual rhythm.
        property color accent: card.palette.highlight
        default property alias body: bodyColumn.data
        Layout.fillWidth: true
        padding: 10
        background: Rectangle {
            radius: 8
            color: card.palette.base
            border.color: card.palette.mid
            // 3-pixel coloured stripe down the left edge; clipped by `radius`
            // so it follows the card's rounded corners.
            Rectangle {
                anchors.left: parent.left
                anchors.top: parent.top
                anchors.bottom: parent.bottom
                width: 3
                color: card.accent
                opacity: 0.85
            }
        }
        contentItem: ColumnLayout {
            spacing: 8
            RowLayout {
                Layout.fillWidth: true
                spacing: 8
                Rectangle {
                    width: 22
                    height: 22
                    radius: 11
                    color: card.accent
                    Label {
                        anchors.centerIn: parent
                        text: card.step
                        color: "white"
                        font.bold: true
                    }
                }
                Label {
                    text: card.heading
                    font.bold: true
                    font.pointSize: 11
                    Layout.fillWidth: true
                }
            }
            ColumnLayout {
                id: bodyColumn
                Layout.fillWidth: true
                spacing: 6
            }
        }
    }

    // ---- Reusable Windows logo (modern 2×2 squares, Microsoft blue) ----
    component WindowsLogo: Item {
        id: wlogo
        property real size: 18
        property color tint: "#0078D4"
        implicitWidth: size
        implicitHeight: size
        Grid {
            anchors.fill: parent
            columns: 2
            rows: 2
            spacing: Math.max(1, wlogo.size * 0.08)
            Repeater {
                model: 4
                Rectangle {
                    width: (wlogo.size - parent.spacing) / 2
                    height: (wlogo.size - parent.spacing) / 2
                    color: wlogo.tint
                }
            }
        }
    }

    // ---- Reusable coloured pill (used for OS chips, phase chips, …) ----
    component Pill: Rectangle {
        id: pill
        property string label: ""
        property color tint: "#3498db"
        property color ink: "white"
        implicitWidth: pillLabel.implicitWidth + 16
        implicitHeight: pillLabel.implicitHeight + 6
        radius: implicitHeight / 2
        color: tint
        Label {
            id: pillLabel
            anchors.centerIn: parent
            text: pill.label
            color: pill.ink
            font.bold: true
            font.pointSize: 8
        }
    }

    // ---- Reusable coloured advisory banner ------------------------------
    component Banner: Rectangle {
        id: banner
        property string message: ""
        property color tint: "#fff3cd"
        property color line: "#e0a800"
        property color ink: "#664d03"
        Layout.fillWidth: true
        visible: message !== ""
        implicitHeight: visible ? bannerLabel.implicitHeight + 18 : 0
        color: tint
        border.color: line
        radius: 6
        Label {
            id: bannerLabel
            anchors.fill: parent
            anchors.margins: 9
            text: banner.message
            color: banner.ink
            wrapMode: Text.Wrap
        }
    }

    // ---- Reusable split button: a main action + an attached dropdown ----
    component SplitButton: Control {
        id: sb
        property string text: ""
        signal clicked()
        signal menuRequested()

        padding: 1
        implicitHeight: 32
        implicitWidth: splitRow.implicitWidth + 2

        background: Rectangle {
            radius: 4
            color: sb.palette.button
            border.color: sb.palette.mid
        }

        contentItem: Row {
            id: splitRow
            opacity: sb.enabled ? 1.0 : 0.5

            // Main action zone.
            Rectangle {
                height: sb.availableHeight
                width: mainText.implicitWidth + 26
                radius: 3
                color: mainArea.pressed ? Qt.darker(sb.palette.button, 1.25)
                     : mainArea.containsMouse ? Qt.lighter(sb.palette.button, 1.08)
                     : "transparent"
                Label {
                    id: mainText
                    anchors.centerIn: parent
                    text: sb.text
                    color: sb.palette.buttonText
                }
                MouseArea {
                    id: mainArea
                    anchors.fill: parent
                    enabled: sb.enabled
                    hoverEnabled: true
                    onClicked: sb.clicked()
                }
            }
            // Divider.
            Rectangle {
                width: 1
                height: sb.availableHeight
                color: sb.palette.mid
            }
            // Dropdown-arrow zone.
            Rectangle {
                height: sb.availableHeight
                width: 24
                radius: 3
                color: arrowArea.pressed ? Qt.darker(sb.palette.button, 1.25)
                     : arrowArea.containsMouse ? Qt.lighter(sb.palette.button, 1.08)
                     : "transparent"
                Label {
                    anchors.centerIn: parent
                    text: "▾"
                    color: sb.palette.buttonText
                }
                MouseArea {
                    id: arrowArea
                    anchors.fill: parent
                    enabled: sb.enabled
                    hoverEnabled: true
                    onClicked: sb.menuRequested()
                }
            }
        }
    }

    menuBar: MenuBar {
        Menu {
            title: "?"
            MenuItem {
                text: "About usbooty"
                onTriggered: aboutDialog.open()
            }
        }
    }

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 12
        spacing: 8

        // ---- Advisory banners ------------------------------------------
        Banner {
            // Missing external tools.
            message: app.depWarning
        }
        Banner {
            // The ISO cannot fit on the chosen drive.
            message: app.method === 2 ? "" : app.fitWarning
            tint: "#f8d7da"
            line: "#dc3545"
            ink: "#842029"
        }

        // ---- Step 1: source image --------------------------------------
        StepCard {
            step: 1
            heading: "Source image"
            accent: "#3498db"
            // The format-only method takes no source image.
            enabled: app.method !== 2

            RowLayout {
                Layout.fillWidth: true
                TextField {
                    id: isoField
                    Layout.fillWidth: true
                    readOnly: true
                    placeholderText: app.method === 2
                        ? "Not used for a plain format"
                        : "Choose an ISO image, or drag one onto the window…"
                    text: app.isoPath
                }
                // Split button: "Browse…" plus a dropdown to download Windows.
                SplitButton {
                    id: sourceBtn
                    text: "Browse…"
                    enabled: !app.busy
                    onClicked: isoDialog.open()
                    onMenuRequested: sourceMenu.popup(sourceBtn, 0, sourceBtn.height)
                    Menu {
                        id: sourceMenu
                        MenuItem {
                            text: "Download a Windows ISO…"
                            onTriggered: winDialog.open()
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
                        label: "Windows"
                        tint: "#0078D4"
                    }
                    Pill {
                        visible: app.linuxIso
                        label: "Linux"
                        tint: "#E67E22"
                    }
                }
                Label {
                    text: app.isoSummary
                    color: palette.placeholderText
                    elide: Text.ElideMiddle
                    Layout.fillWidth: true
                }
            }
            TextEdit {
                // Selectable so the user can copy the hash to cross-check it
                // against the value the distro / Microsoft publishes.
                visible: app.isoPath !== ""
                text: "SHA-256:  " + app.isoSha256
                readOnly: true
                selectByMouse: true
                wrapMode: TextEdit.WrapAnywhere
                color: palette.placeholderText
                font.family: "monospace"
                font.pointSize: 8
                Layout.fillWidth: true
            }
        }

        // ---- Step 2: target device -------------------------------------
        StepCard {
            step: 2
            heading: "Target device"
            accent: "#E67E22"

            RowLayout {
                Layout.fillWidth: true
                ComboBox {
                    id: deviceBox
                    Layout.fillWidth: true
                    enabled: !app.busy && count > 0
                    model: app.devices.length > 0 ? app.devices.split("\n") : []
                    currentIndex: app.selectedDevice
                    onActivated: function(index) { app.selectDevice(index) }
                    displayText: count > 0 ? currentText
                                           : "No removable devices found"

                    // Two-line rows: hardware name above, capacity / bus /
                    // node below, with internal disks flagged in red.
                    delegate: ItemDelegate {
                        width: deviceBox.width
                        highlighted: deviceBox.highlightedIndex === index
                        contentItem: ColumnLayout {
                            spacing: 1
                            Label {
                                text: modelData.split(" — ")[0]
                                font.bold: true
                                elide: Text.ElideRight
                                Layout.fillWidth: true
                            }
                            Label {
                                text: {
                                    var parts = modelData.split(" — ")
                                    return parts.length > 1
                                        ? parts.slice(1).join(" — ") : ""
                                }
                                font.pointSize: 9
                                elide: Text.ElideRight
                                Layout.fillWidth: true
                                color: text.indexOf("Internal disk") >= 0
                                       ? "#c0392b" : window.palette.placeholderText
                            }
                        }
                    }
                }
                Button {
                    text: "Refresh"
                    enabled: !app.busy
                    onClicked: app.refreshDevices()
                }
            }
            CheckBox {
                text: "Show non-removable (internal) disks"
                enabled: !app.busy
                checked: app.showFixedDisks
                onToggled: {
                    app.showFixedDisks = checked
                    app.refreshDevices()
                }
            }
        }

        // ---- Step 3: options -------------------------------------------
        StepCard {
            step: 3
            heading: "Options"
            accent: "#16A085"

            GridLayout {
                Layout.fillWidth: true
                columns: 2
                columnSpacing: 12
                rowSpacing: 8

                Label { text: "Write method" }
                ComboBox {
                    Layout.fillWidth: true
                    enabled: !app.busy
                    model: ["DD image (raw copy)", "Partition & copy files",
                            "Format only (no ISO)", "Ventoy (multi-boot USB)"]
                    currentIndex: app.method
                    onActivated: function(index) { app.method = index }
                }

                Label { text: "Filesystem" }
                ComboBox {
                    Layout.fillWidth: true
                    // The filesystem is chosen automatically when writing an
                    // image; it is only user-selectable for a plain format.
                    enabled: !app.busy && app.method === 2
                    model: ["FAT32", "NTFS", "exFAT", "ext4"]
                    currentIndex: app.filesystem
                    onActivated: function(index) { app.filesystem = index }
                    ToolTip.visible: hovered && !enabled
                    ToolTip.text: "When writing an image, the filesystem is chosen automatically."
                }

                Label { text: "Partition scheme" }
                ComboBox {
                    Layout.fillWidth: true
                    // Meaningful for the partition and format methods; a raw
                    // DD copy keeps the ISO's own embedded table.
                    enabled: !app.busy && app.method !== 0
                    model: ["GPT (UEFI)", "MBR (BIOS/Legacy)"]
                    currentIndex: app.table
                    onActivated: function(index) { app.table = index }
                    ToolTip.visible: hovered && !enabled
                    ToolTip.text: "The DD method preserves the ISO's own partition table."
                }

                // Ventoy names its own data partition — no label field for it.
                Label {
                    text: "Volume label"
                    visible: app.method !== 3
                }
                TextField {
                    Layout.fillWidth: true
                    visible: app.method !== 3
                    enabled: !app.busy && app.method !== 0
                    placeholderText: "Drive label"
                    text: app.label
                    // onTextEdited (not onTextChanged) avoids a binding loop:
                    // it fires only for user edits, not the pre-fill above.
                    onTextEdited: app.label = text
                    ToolTip.visible: hovered && !enabled
                    ToolTip.text: "The label is sanitized to each filesystem's limits."
                }
            }

            CheckBox {
                text: "Full format — erase the whole device first (slow)"
                // DD overwrites every sector anyway, and Ventoy does its own
                // partitioning and formatting.
                enabled: !app.busy && (app.method === 1 || app.method === 2)
                checked: app.fullFormat
                onToggled: app.fullFormat = checked
            }

            CheckBox {
                text: "Verify after writing — read the data back and check it"
                // A plain format / Ventoy install writes no verifiable payload.
                enabled: !app.busy && app.method < 2
                checked: app.verify
                onToggled: app.verify = checked
            }

            // Ventoy options — only for the Ventoy write method.
            ColumnLayout {
                Layout.fillWidth: true
                visible: app.method === 3
                spacing: 4
                CheckBox {
                    text: "Update an existing Ventoy install (keeps your ISOs)"
                    enabled: !app.busy
                    checked: app.ventoyUpdate
                    onToggled: app.ventoyUpdate = checked
                }
                CheckBox {
                    text: "Secure Boot support"
                    enabled: !app.busy
                    checked: app.ventoySecureBoot
                    onToggled: app.ventoySecureBoot = checked
                }
                Label {
                    text: "Ventoy makes a USB you drop ISOs onto and boot directly. "
                        + "A loaded ISO above (optional) is copied onto it."
                    color: palette.placeholderText
                    font.pointSize: 8
                    wrapMode: Text.Wrap
                    Layout.fillWidth: true
                }
            }

            // Persistence — only for Linux live ISOs that support it.
            ColumnLayout {
                Layout.fillWidth: true
                visible: app.persistenceSupported && app.method === 1
                spacing: 2
                Label {
                    text: app.persistenceSize > 0
                        ? "Persistent storage:  "
                          + (app.persistenceSize / 1024).toFixed(1) + " GB"
                        : "Persistent storage:  off"
                    font.bold: true
                }
                Slider {
                    Layout.fillWidth: true
                    enabled: !app.busy
                    from: 0
                    to: 32768
                    stepSize: 256
                    value: app.persistenceSize
                    onMoved: app.persistenceSize = value
                }
                Label {
                    text: "Keeps your files and settings across reboots of this live USB."
                    color: palette.placeholderText
                    font.pointSize: 8
                    wrapMode: Text.Wrap
                    Layout.fillWidth: true
                }
            }

            // Linux ISO whose distribution has no partition-persistence support.
            Label {
                Layout.fillWidth: true
                visible: app.linuxIso && !app.persistenceSupported && app.method === 1
                text: "Persistent storage isn't supported for this distribution "
                    + "(only Debian/Ubuntu-family live systems can use it)."
                color: palette.placeholderText
                font.pointSize: 8
                wrapMode: Text.Wrap
            }
        }

        // ---- Action -----------------------------------------------------
        Button {
            Layout.fillWidth: true
            Layout.preferredHeight: 44
            text: app.busy ? "Cancel" : "Start"
            highlighted: true
            font.bold: true
            enabled: app.busy || window.ready
            onClicked: {
                if (app.busy) {
                    app.cancel()
                } else if (app.windowsIso && app.method === 1) {
                    // Windows installer, partition method: offer the setup
                    // options first. (DD is a raw copy — it cannot apply an
                    // autounattend.xml, so no setup dialog there.)
                    windowsSetupDialog.open()
                } else {
                    confirmLabel.text = app.confirmText()
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
            // A colour for the active phase, so the user can tell at a glance
            // whether we are still writing or already verifying the bytes back.
            readonly property color phaseColor: {
                var p = app.phase.toLowerCase()
                if (p.indexOf("verif") >= 0)  return "#27AE60" // green
                if (p.indexOf("flush") >= 0)  return "#F39C12" // amber
                if (p.indexOf("format") >= 0) return "#8E44AD" // violet
                if (p.indexOf("ventoy") >= 0) return "#16A085" // teal
                if (p.indexOf("download") >= 0) return "#0078D4" // Windows blue
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
                    // Re-tint the fill bar to match the phase. The Qt default
                    // contentItem is a Rectangle, so we override it cleanly.
                    contentItem: Item {
                        implicitHeight: 6
                        Rectangle {
                            width: phaseBar.visualPosition * parent.width
                            height: parent.height
                            radius: 3
                            color: progressFrame.phaseColor
                        }
                    }
                    background: Rectangle {
                        implicitHeight: 6
                        color: Qt.rgba(0, 0, 0, 0.08)
                        radius: 3
                    }
                }
                RowLayout {
                    Layout.fillWidth: true
                    spacing: 8
                    Pill {
                        visible: app.phase !== ""
                        label: app.phase
                        tint: progressFrame.phaseColor
                    }
                    Label {
                        text: app.phase === "" ? "Working" : ""
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
                            parts.push("ETA " + app.eta)
                        if (app.busy) {
                            var e = window.fmtTime(window.elapsedSecs)
                            if (e !== "")
                                parts.push(e + " elapsed")
                        }
                        return parts.join("     ·     ")
                    }
                }
                Label {
                    text: app.status
                    color: palette.placeholderText
                    elide: Text.ElideRight
                    Layout.fillWidth: true
                }
            }
        }

        // ---- Activity log (always visible) ------------------------------
        Frame {
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.minimumHeight: 130
            padding: 10
            background: Rectangle {
                radius: 8
                color: palette.base
                border.color: palette.mid
            }
            contentItem: ColumnLayout {
                spacing: 6
                RowLayout {
                    Layout.fillWidth: true
                    Label {
                        text: "Activity log"
                        font.bold: true
                        Layout.fillWidth: true
                    }
                    Button {
                        text: "Clear"
                        flat: true
                        enabled: app.logText !== "" && !app.busy
                        onClicked: app.logText = ""
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
                        // Binding the width to the viewport makes lines wrap;
                        // without it the text runs off the side and the view
                        // never gains the height it needs to scroll.
                        width: logScroll.availableWidth
                        readOnly: true
                        wrapMode: TextArea.Wrap
                        font.family: "monospace"
                        font.pointSize: 9
                        placeholderText: "Job output will appear here."
                        text: app.logText
                        // Keep the newest line in view as the log grows.
                        onTextChanged: cursorPosition = length
                    }
                }
            }
        }
    }

    // ---- Drag-and-drop overlay -----------------------------------------
    DropArea {
        id: isoDrop
        anchors.fill: parent
        onDropped: function(drop) {
            if (app.busy || !drop.hasUrls)
                return
            var u = drop.urls[0].toString().toLowerCase()
            if (u.endsWith(".iso") || u.endsWith(".img"))
                app.setIso(drop.urls[0])
        }
        Rectangle {
            anchors.fill: parent
            anchors.margins: 6
            visible: isoDrop.containsDrag && !app.busy
            // Low-alpha tint (ARGB); no `opacity` so the label stays crisp.
            color: "#241e88e5"
            border.color: "#1e88e5"
            border.width: 2
            radius: 10
            Label {
                anchors.centerIn: parent
                text: "Drop ISO image here"
                font.pointSize: 18
                font.bold: true
                color: "#1565c0"
            }
        }
    }

    // ---- Dialogs --------------------------------------------------------
    FileDialog {
        id: isoDialog
        title: "Select an ISO image"
        nameFilters: ["ISO images (*.iso *.img)", "All files (*)"]
        onAccepted: app.setIso(selectedFile)
    }

    // Shown when Start is pressed on a Windows ISO — optional install tweaks.
    // All settings flow into a generated `autounattend.xml` on the USB root.
    Dialog {
        id: windowsSetupDialog
        anchors.centerIn: parent
        width: 600
        // Cap the dialog height to the window minus chrome so the content
        // scrolls instead of overflowing on smaller displays.
        height: Math.min(window.height - 80, 760)
        modal: true
        // Inset the contentItem from the dialog frame on every side; the
        // header and footer carry their own spacing.
        topPadding: 14
        bottomPadding: 14
        leftPadding: 18
        rightPadding: 18
        // Custom header replacing the default text-only title bar: the
        // Microsoft blue accent and the Windows flag immediately identify
        // this as the Windows installer tweak panel.
        header: Rectangle {
            color: "#0078D4"
            implicitHeight: 52
            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: 16
                anchors.rightMargin: 16
                spacing: 12
                WindowsLogo {
                    size: 24
                    tint: "white"
                }
                ColumnLayout {
                    spacing: 0
                    Layout.fillWidth: true
                    Label {
                        text: "Windows setup"
                        color: "white"
                        font.bold: true
                        font.pointSize: 12
                    }
                    Label {
                        text: "Optional install tweaks — written to autounattend.xml"
                        color: Qt.rgba(1, 1, 1, 0.82)
                        font.pointSize: 8
                    }
                }
            }
        }
        standardButtons: Dialog.Ok | Dialog.Cancel
        onAccepted: {
            confirmLabel.text = app.confirmText()
            confirmDialog.open()
        }
        contentItem: ScrollView {
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
                text: "Customize the installation below, or just press OK to skip — "
                    + "every option is optional."
                color: palette.placeholderText
                wrapMode: Text.Wrap
                Layout.fillWidth: true
            }

            // --- Setup-time tweaks --------------------------------------
            Label { text: "Setup"; font.bold: true; Layout.topMargin: 6 }
            Rectangle { Layout.fillWidth: true; height: 1; color: palette.mid; opacity: 0.5 }
            CheckBox {
                text: "Bypass Windows 11 hardware checks — TPM, Secure Boot, RAM, Storage, CPU, Disk"
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
                ToolTip.text: "Writes six LabConfig registry keys during Setup so Win 11 skips its " +
                    "hardware requirements: TPM 2.0, Secure Boot, 8 GB RAM, 64 GB system disk, " +
                    "supported-CPU allowlist, and disk-geometry check. Harmless on Windows 10."
            }
            CheckBox {
                text: "Auto-accept the Setup EULA"
                checked: app.acceptEula
                onToggled: app.acceptEula = checked
                ToolTip.delay: 500
                ToolTip.visible: hovered
                ToolTip.text: "Sets <AcceptEula>true</AcceptEula> in the windowsPE UserData block " +
                    "so the Setup-time license prompt is skipped."
            }
            CheckBox {
                text: "Enable .NET Framework 3.5 from the install media"
                checked: app.enableDotnet35
                onToggled: app.enableDotnet35 = checked
                ToolTip.delay: 500
                ToolTip.visible: hovered
                ToolTip.text: "Runs DISM in the specialize pass to enable NetFx3 from the install " +
                    "media's sources\\sxs folder. Needed by many legacy apps; no network required."
            }
            RowLayout {
                Layout.fillWidth: true
                Label { text: "Product key"; Layout.minimumWidth: 110 }
                TextField {
                    Layout.fillWidth: true
                    placeholderText: "Optional — e.g. VK7JG-NPHTM-C97JM-9MPGT-3V66T (Win 11 Pro)"
                    text: app.productKey
                    onTextEdited: app.productKey = text
                }
            }

            // --- OOBE (first-boot) skips --------------------------------
            Label { text: "Out-of-box experience"; font.bold: true; Layout.topMargin: 6 }
            Rectangle { Layout.fillWidth: true; height: 1; color: palette.mid; opacity: 0.5 }
            CheckBox {
                text: "Skip Microsoft-account requirement (works on Win 10 and all Win 11)"
                checked: app.skipMsaccount
                onToggled: app.skipMsaccount = checked
                ToolTip.delay: 500
                ToolTip.visible: hovered
                ToolTip.text: "Emits both BypassNRO (Win 10 / Win 11 pre-24H2) and " +
                    "<HideOnlineAccountScreens>true</HideOnlineAccountScreens> (Win 11 24H2+), " +
                    "so a single toggle covers the whole supported matrix."
            }
            CheckBox {
                text: "Disable network during OOBE — force local account on Win 11 24H2+"
                checked: app.disableNetworkDuringOobe
                onToggled: app.disableNetworkDuringOobe = checked
                ToolTip.delay: 500
                ToolTip.visible: hovered
                ToolTip.text: "Runs `Get-NetAdapter | Disable-NetAdapter` in specialize, then a " +
                    "matching Enable-NetAdapter in FirstLogonCommands. With no network during OOBE, " +
                    "Windows falls back to local-account creation — the most robust workaround when " +
                    "BypassNRO and HideOnlineAccountScreens are ignored."
            }
            CheckBox {
                text: "Skip the \"connect to a network\" Wi-Fi screen"
                checked: app.hideWirelessSetup
                onToggled: app.hideWirelessSetup = checked
                ToolTip.delay: 500
                ToolTip.visible: hovered
                ToolTip.text: "Sets <HideWirelessSetupInOOBE>true</HideWirelessSetupInOOBE> so the " +
                    "OOBE \"Let's connect you to a network\" page is skipped entirely."
            }
            CheckBox {
                text: "Hide the OEM-registration screen"
                checked: app.hideOemRegistration
                onToggled: app.hideOemRegistration = checked
                ToolTip.delay: 500
                ToolTip.visible: hovered
                ToolTip.text: "Sets <HideOEMRegistrationScreen>true</HideOEMRegistrationScreen> " +
                    "so OEM-branded OOBE pages do not appear (irrelevant on clean Microsoft ISOs)."
            }
            CheckBox {
                text: "Pre-answer the network-type prompt as \"Work\" (private/trusted)"
                checked: app.networkLocationWork
                onToggled: app.networkLocationWork = checked
                ToolTip.delay: 500
                ToolTip.visible: hovered
                ToolTip.text: "Sets <NetworkLocation>Work</NetworkLocation> so OOBE classifies the " +
                    "network as private/trusted without prompting (file sharing and discovery enabled)."
            }
            CheckBox {
                text: "Disable data-collection / telemetry prompts"
                checked: app.disableTelemetry
                onToggled: app.disableTelemetry = checked
                ToolTip.delay: 500
                ToolTip.visible: hovered
                ToolTip.text: "Sets <HideEULAPage>true</HideEULAPage> and <ProtectYourPC>3</ProtectYourPC> " +
                    "— the OOBE \"skip Express settings\" answer that opts out of every diagnostic / " +
                    "advertising / tailored-experience prompt."
            }

            // --- Local account ------------------------------------------
            Label { text: "Local account"; font.bold: true; Layout.topMargin: 6 }
            Rectangle { Layout.fillWidth: true; height: 1; color: palette.mid; opacity: 0.5 }
            RowLayout {
                Layout.fillWidth: true
                Label { text: "Name"; Layout.minimumWidth: 110 }
                TextField {
                    Layout.fillWidth: true
                    placeholderText: "Optional — leave empty to keep the OOBE prompt"
                    text: app.localAccount
                    onTextEdited: app.localAccount = text
                }
            }
            RowLayout {
                Layout.fillWidth: true
                Label { text: "Password"; Layout.minimumWidth: 110 }
                TextField {
                    Layout.fillWidth: true
                    placeholderText: "Optional — sets a password and enables one-shot auto-logon"
                    echoMode: TextInput.Password
                    text: app.localAccountPassword
                    onTextEdited: app.localAccountPassword = text
                }
            }

            // --- System identity ----------------------------------------
            Label { text: "System"; font.bold: true; Layout.topMargin: 6 }
            Rectangle { Layout.fillWidth: true; height: 1; color: palette.mid; opacity: 0.5 }
            RowLayout {
                Layout.fillWidth: true
                Label { text: "Computer name"; Layout.minimumWidth: 110 }
                TextField {
                    Layout.fillWidth: true
                    placeholderText: "Optional — up to 15 characters, no whitespace"
                    text: app.computerName
                    onTextEdited: app.computerName = text
                }
            }
            RowLayout {
                Layout.fillWidth: true
                Label { text: "Locale"; Layout.minimumWidth: 110 }
                TextField {
                    Layout.fillWidth: true
                    placeholderText: "Optional — e.g. en-US, fr-FR, de-DE"
                    text: app.locale
                    onTextEdited: app.locale = text
                }
            }
            RowLayout {
                Layout.fillWidth: true
                Label { text: "Time zone"; Layout.minimumWidth: 110 }
                ComboBox {
                    id: timezoneCombo
                    Layout.fillWidth: true
                    // Parallel newline-separated lists built once in Rust from
                    // the Microsoft TimeZone catalog, sorted by UTC offset.
                    readonly property var tzIds: app.timezoneIds.split("\n")
                    model: app.timezoneLabels.split("\n")
                    // Restore the previously-chosen ID on dialog open.
                    Component.onCompleted: currentIndex = Math.max(0, tzIds.indexOf(app.timezone))
                    onActivated: app.timezone = tzIds[currentIndex] || ""
                }
            }

            // --- Debloat ------------------------------------------------
            Label { text: "Privacy & debloat"; font.bold: true; Layout.topMargin: 6 }
            Rectangle { Layout.fillWidth: true; height: 1; color: palette.mid; opacity: 0.5 }
            CheckBox {
                id: debloatBox
                text: "Apply debloat profile"
                checked: app.applyDebloat
                onToggled: app.applyDebloat = checked
                ToolTip.delay: 500
                ToolTip.visible: hovered
                ToolTip.text: "Writes usbooty-debloat.reg to the USB and imports it during specialize " +
                    "(HKLM machine policies + HKU\\DFT default-user policies). Disables Cortana, " +
                    "Copilot, Recall, telemetry, lockscreen ads, suggested apps, advertising ID, " +
                    "and assorted recommendation surfaces. Win 11-only keys are no-ops on Win 10."
            }
            Label {
                visible: debloatBox.checked
                Layout.fillWidth: true
                Layout.leftMargin: 24
                wrapMode: Text.Wrap
                color: palette.placeholderText
                font.pointSize: 9
                textFormat: Text.RichText
                text:
                    "<b>Applied machine-wide (HKLM Group Policy):</b><br>" +
                    "&nbsp;• News &amp; Interests feed (taskbar widget)<br>" +
                    "&nbsp;• Consumer-feature ads — suggested Store apps, OEM-style inserts<br>" +
                    "&nbsp;• Activity History sync to Microsoft<br>" +
                    "&nbsp;• Cortana in Search<br>" +
                    "&nbsp;• Windows Copilot service<br>" +
                    "&nbsp;• Windows Recall — the rolling-screenshot AI history (Win 11 24H2+)<br>" +
                    "&nbsp;• Diagnostic data — set to Required only<br>" +
                    "<br>" +
                    "<b>Applied to the default user profile (inherited by every new account):</b><br>" +
                    "&nbsp;• Bing / web suggestions in Start &amp; Search<br>" +
                    "&nbsp;• File extensions shown (instead of hidden)<br>" +
                    "&nbsp;• Copilot, Task View, Widgets and \"People\" buttons hidden from the taskbar<br>" +
                    "&nbsp;• Sync-provider ads in Explorer suppressed<br>" +
                    "&nbsp;• Start menu \"recommendations\" and Iris suggestions disabled<br>" +
                    "&nbsp;• ContentDeliveryManager: lock-screen rotation ads, pre-installed-app suggestions, \"subscribed content\" tiles<br>" +
                    "&nbsp;• Cortana / Bing inside per-user Search<br>" +
                    "&nbsp;• Advertising ID disabled<br>" +
                    "&nbsp;• \"Tailored experiences\" derived from diagnostic data<br>" +
                    "&nbsp;• \"Suggested\" toast notifications<br>" +
                    "&nbsp;• Phone Link / \"use your mobile with Windows\" prompts<br>" +
                    "&nbsp;• Online speech recognition (voice stays local)<br>" +
                    "&nbsp;• Contact harvesting for input personalization<br>" +
                    "&nbsp;• Feedback Hub frequency set to Never<br>" +
                    "&nbsp;• \"Finish setting up your device\" prompts<br>" +
                    "<br>" +
                    "Windows 11-only keys (Copilot, Widgets, News &amp; Interests, Recall) " +
                    "are silently ignored on Windows 10."
            }
            }
        }
    }

    Dialog {
        id: confirmDialog
        title: "Erase device?"
        anchors.centerIn: parent
        width: 440
        modal: true
        standardButtons: Dialog.Ok | Dialog.Cancel
        onAccepted: app.start()
        contentItem: ColumnLayout {
            spacing: 8
            Label {
                id: confirmLabel
                wrapMode: Text.Wrap
                Layout.fillWidth: true
            }
            Label {
                text: "This cannot be undone."
                font.bold: true
            }
        }
    }

    Dialog {
        id: resultDialog
        anchors.centerIn: parent
        width: 440
        modal: true
        // Set by the AppController.onJobFinished handler; drives the colour
        // of the header strip and the success / failure glyph below.
        property bool success: true
        standardButtons: Dialog.Ok
        // A thin coloured strip across the top of the dialog — instant
        // green/red signal before the user even reads the message.
        header: Rectangle {
            implicitHeight: 4
            color: resultDialog.success ? "#27AE60" : "#E74C3C"
        }
        contentItem: RowLayout {
            spacing: 14
            Rectangle {
                width: 40
                height: 40
                radius: 20
                color: resultDialog.success ? "#27AE60" : "#E74C3C"
                Layout.alignment: Qt.AlignTop
                Label {
                    anchors.centerIn: parent
                    text: resultDialog.success ? "✓" : "✕"
                    color: "white"
                    font.bold: true
                    font.pointSize: 18
                }
            }
            Label {
                id: resultLabel
                wrapMode: Text.Wrap
                Layout.fillWidth: true
                Layout.alignment: Qt.AlignVCenter
            }
        }
    }

    Dialog {
        id: aboutDialog
        title: "About usbooty"
        anchors.centerIn: parent
        width: 440
        modal: true
        standardButtons: Dialog.Ok
        contentItem: ColumnLayout {
            spacing: 12

            // Logo + name / version side by side at the top of the dialog.
            RowLayout {
                Layout.fillWidth: true
                spacing: 14
                Image {
                    source: "qrc:/icons/usbooty.svg"
                    sourceSize.width: 64
                    sourceSize.height: 64
                    Layout.preferredWidth: 64
                    Layout.preferredHeight: 64
                    fillMode: Image.PreserveAspectFit
                    smooth: true
                }
                ColumnLayout {
                    spacing: 0
                    Layout.fillWidth: true
                    Label {
                        text: "usbooty"
                        font.bold: true
                        font.pointSize: 16
                    }
                    Label {
                        text: "Version " + app.appVersion
                        color: palette.placeholderText
                        font.pointSize: 9
                    }
                }
            }

            Label {
                text: "Create bootable USB drives from ISO images."
                wrapMode: Text.Wrap
                Layout.fillWidth: true
            }

            GridLayout {
                columns: 2
                columnSpacing: 16
                rowSpacing: 3
                Label { text: "Author"; font.bold: true }
                Label { text: "Thoxy" }
                Label { text: "License"; font.bold: true }
                Label { text: "MIT" }
            }

            Label {
                text: "DD raw write, partition-and-copy (FAT32 / NTFS / exFAT / "
                    + "ext4, with the Windows UEFI:NTFS layout), Linux "
                    + "persistence, Windows 11 setup customization, plain "
                    + "formatting, and Ventoy multi-boot USBs."
                wrapMode: Text.Wrap
                color: palette.placeholderText
                font.pointSize: 9
                Layout.fillWidth: true
            }

            Label {
                text: "<a href=\"https://git.thoxy.xyz/thoxy/usbooty\">"
                    + "git.thoxy.xyz/thoxy/usbooty</a>"
                textFormat: Text.RichText
                font.pointSize: 9
                onLinkActivated: function(link) { Qt.openUrlExternally(link) }
            }
        }
    }

    Dialog {
        id: winDialog
        title: "Download a Windows ISO"
        anchors.centerIn: parent
        width: 500
        modal: true
        standardButtons: Dialog.Close
        contentItem: ColumnLayout {
            spacing: 10
            Label {
                text: "Fetch an official ISO from Microsoft. Each step queries "
                    + "Microsoft and may take a few seconds."
                wrapMode: Text.Wrap
                Layout.fillWidth: true
            }

            // Step 1 — choose the Windows release.
            RowLayout {
                Layout.fillWidth: true
                ComboBox {
                    id: winVersion
                    Layout.fillWidth: true
                    enabled: !app.busy
                    model: ["Windows 11", "Windows 10"]
                }
                Button {
                    text: "List languages"
                    enabled: !app.busy
                    onClicked: app.winFetchLanguages(winVersion.currentIndex)
                }
            }

            // Step 2 — choose a language.
            RowLayout {
                Layout.fillWidth: true
                ComboBox {
                    id: winLang
                    Layout.fillWidth: true
                    enabled: !app.busy && app.winLanguages !== ""
                    model: app.winLanguages !== "" ? app.winLanguages.split("\n") : []
                }
                Button {
                    text: "List downloads"
                    enabled: !app.busy && app.winLanguages !== ""
                    onClicked: app.winFetchOptions(winLang.currentIndex)
                }
            }

            // Step 3 — choose an edition/architecture and download.
            RowLayout {
                Layout.fillWidth: true
                ComboBox {
                    id: winOpt
                    Layout.fillWidth: true
                    enabled: !app.busy && app.winOptions !== ""
                    model: app.winOptions !== "" ? app.winOptions.split("\n") : []
                }
                Button {
                    text: "Download"
                    highlighted: true
                    enabled: !app.busy && app.winOptions !== ""
                    onClicked: {
                        app.winDownload(winOpt.currentIndex)
                        winDialog.close()
                    }
                }
            }

            Label {
                text: app.status
                color: palette.placeholderText
                wrapMode: Text.Wrap
                Layout.fillWidth: true
            }

            Label {
                text: "If Microsoft's anti-bot system rejects the request "
                    + "(common on VPNs and some networks), download manually:"
                color: palette.placeholderText
                wrapMode: Text.Wrap
                Layout.fillWidth: true
            }
            Button {
                text: "Open Microsoft download page"
                Layout.fillWidth: true
                onClicked: app.openMicrosoftPage(winVersion.currentIndex)
            }
        }
    }
}
