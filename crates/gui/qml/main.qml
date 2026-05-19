import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Dialogs
import com.usbooty

ApplicationWindow {
    id: window
    visible: true
    width: 600
    height: 860
    minimumWidth: 520
    minimumHeight: 700
    title: "usbooty — Bootable USB Creator"

    AppController {
        id: app
        Component.onCompleted: app.refreshDevices()
        onJobFinished: function(success, message) {
            resultDialog.title = success ? "Success" : "Failed"
            resultLabel.text = message
            resultDialog.open()
        }
    }

    // Whether the user can launch a job right now.
    readonly property bool ready:
        !app.busy && app.isoPath !== "" && app.selectedDevice >= 0
        && app.fitWarning === ""

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
        default property alias body: bodyColumn.data
        Layout.fillWidth: true
        padding: 14
        background: Rectangle {
            radius: 8
            color: card.palette.base
            border.color: card.palette.mid
        }
        contentItem: ColumnLayout {
            spacing: 12
            RowLayout {
                Layout.fillWidth: true
                spacing: 9
                Rectangle {
                    width: 24
                    height: 24
                    radius: 12
                    color: card.palette.highlight
                    Label {
                        anchors.centerIn: parent
                        text: card.step
                        color: card.palette.highlightedText
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
                spacing: 8
            }
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

    header: ToolBar {
        RowLayout {
            anchors.fill: parent
            Label {
                text: window.title
                font.bold: true
                Layout.leftMargin: 12
                Layout.fillWidth: true
            }
            Button {
                text: "About"
                flat: true
                onClicked: aboutDialog.open()
            }
        }
    }

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 16
        spacing: 12

        // ---- Advisory banners ------------------------------------------
        Banner {
            // Missing external tools.
            message: app.depWarning
        }
        Banner {
            // The ISO cannot fit on the chosen drive.
            message: app.fitWarning
            tint: "#f8d7da"
            line: "#dc3545"
            ink: "#842029"
        }

        // ---- Step 1: source image --------------------------------------
        StepCard {
            step: 1
            heading: "Source image"

            RowLayout {
                Layout.fillWidth: true
                TextField {
                    id: isoField
                    Layout.fillWidth: true
                    readOnly: true
                    placeholderText: "Choose an ISO image, or drag one onto the window…"
                    text: app.isoPath
                }
                Button {
                    text: "Browse…"
                    enabled: !app.busy
                    onClicked: isoDialog.open()
                }
            }
            RowLayout {
                Layout.fillWidth: true
                Label {
                    text: app.isoSummary
                    color: palette.placeholderText
                    elide: Text.ElideMiddle
                    Layout.fillWidth: true
                }
                Button {
                    text: "Download Windows…"
                    flat: true
                    enabled: !app.busy
                    onClicked: winDialog.open()
                }
            }
        }

        // ---- Step 2: target device -------------------------------------
        StepCard {
            step: 2
            heading: "Target device"

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

            GridLayout {
                Layout.fillWidth: true
                columns: 2
                columnSpacing: 12
                rowSpacing: 8

                Label { text: "Write method" }
                ComboBox {
                    Layout.fillWidth: true
                    enabled: !app.busy
                    model: ["DD image (raw copy)", "Partition & copy files"]
                    currentIndex: app.method
                    onActivated: function(index) { app.method = index }
                }

                Label { text: "Partition scheme" }
                ComboBox {
                    Layout.fillWidth: true
                    // Always shown; only meaningful for the FAT32 method, since
                    // a raw DD copy keeps the ISO's own embedded table.
                    enabled: !app.busy && app.method === 1
                    model: ["GPT (UEFI)", "MBR (BIOS/Legacy)"]
                    currentIndex: app.table
                    onActivated: function(index) { app.table = index }
                    ToolTip.visible: hovered && !enabled
                    ToolTip.text: "The DD method preserves the ISO's own partition table."
                }
            }
        }

        // ---- Action -----------------------------------------------------
        Button {
            Layout.fillWidth: true
            Layout.preferredHeight: 46
            text: app.busy ? "Cancel" : "Start"
            highlighted: true
            font.bold: true
            enabled: app.busy || window.ready
            onClicked: {
                if (app.busy) {
                    app.cancel()
                } else {
                    confirmLabel.text = app.confirmText()
                    confirmDialog.open()
                }
            }
        }

        // ---- Progress ---------------------------------------------------
        Frame {
            Layout.fillWidth: true
            visible: app.busy || app.progress > 0
            padding: 12
            background: Rectangle {
                radius: 8
                color: palette.base
                border.color: palette.mid
            }
            contentItem: ColumnLayout {
                spacing: 8
                ProgressBar {
                    Layout.fillWidth: true
                    from: 0
                    to: 1
                    value: app.progress
                }
                RowLayout {
                    Layout.fillWidth: true
                    Label {
                        text: app.phase !== "" ? app.phase : "Working"
                        font.bold: true
                    }
                    Item { Layout.fillWidth: true }
                    Label {
                        text: app.progress > 0
                              ? Math.round(app.progress * 100) + " %" : ""
                        font.bold: true
                        color: palette.highlight
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
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    clip: true
                    TextArea {
                        readOnly: true
                        wrapMode: TextEdit.Wrap
                        font.family: "monospace"
                        placeholderText: "Job output will appear here."
                        text: app.logText
                        // Keep the newest line in view.
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

    Dialog {
        id: confirmDialog
        title: "Erase device?"
        anchors.centerIn: parent
        width: 440
        modal: true
        standardButtons: Dialog.Ok | Dialog.Cancel
        // After confirmation, a large Windows install.wim needs a follow-up
        // choice; otherwise start immediately.
        onAccepted: {
            if (app.needsWimChoice())
                wimDialog.open()
            else
                app.start()
        }
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
        id: wimDialog
        title: "Large install.wim"
        anchors.centerIn: parent
        width: 460
        modal: true
        standardButtons: Dialog.Ok | Dialog.Cancel
        onAccepted: {
            app.wimChoice = uefiNtfsRadio.checked ? 1 : 0
            app.start()
        }
        contentItem: ColumnLayout {
            spacing: 8
            Label {
                text: "This Windows ISO's install.wim is larger than 4 GB — "
                    + "too big for a single file on FAT32. Choose how to handle it:"
                wrapMode: Text.Wrap
                Layout.fillWidth: true
            }
            RadioButton {
                id: splitRadio
                text: "Split install.wim into .swm chunks (FAT32, most compatible)"
                checked: true
                Layout.fillWidth: true
            }
            RadioButton {
                id: uefiNtfsRadio
                text: "UEFI:NTFS — NTFS partition + signed bootloader (keeps install.wim intact)"
                Layout.fillWidth: true
            }
        }
    }

    Dialog {
        id: resultDialog
        anchors.centerIn: parent
        width: 440
        modal: true
        standardButtons: Dialog.Ok
        contentItem: ColumnLayout {
            Label {
                id: resultLabel
                wrapMode: Text.Wrap
                Layout.fillWidth: true
            }
        }
    }

    Dialog {
        id: aboutDialog
        title: "About usbooty"
        anchors.centerIn: parent
        width: 420
        modal: true
        standardButtons: Dialog.Ok
        contentItem: ColumnLayout {
            spacing: 6
            Label {
                text: "usbooty"
                font.bold: true
                font.pointSize: 14
            }
            Label {
                text: "Create bootable USB drives from ISO images."
                wrapMode: Text.Wrap
                Layout.fillWidth: true
            }
            Label {
                text: "Two write methods: a raw DD copy, and a partition-and-"
                    + "copy method that creates FAT32 or NTFS partitions and "
                    + "supports the Windows UEFI:NTFS layout."
                wrapMode: Text.Wrap
                Layout.fillWidth: true
                color: palette.placeholderText
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
