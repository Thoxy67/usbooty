import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import com.usbooty

Dialog {
    id: confirmDialog
    required property var app
    required property var host
    // Fired when the user clicks "Inspect device details…"; main.qml opens
    // the read-only inspect dialog in response.
    signal inspectRequested()
        anchors.centerIn: parent
        width: Math.min(480, host.width - 40)
        // Hug the content, but never overflow a short host window; the
        // inner ScrollView keeps the confirm controls reachable when clamped.
        height: Math.min(implicitHeight, host.height - 40)
        modal: true
        topPadding: 14
        bottomPadding: 14
        leftPadding: 18
        rightPadding: 18
        // The Rust invokables don't emit change signals, so a plain
        // binding to `app.selectedX()` would never refresh. Gating on
        // `visible` re-evaluates the binding every time the dialog
        // opens, same effect as the imperative onOpened we used to run,
        // but free of an extra signal handler.
        property bool internalDisk: confirmDialog.visible && app.selectedIsInternal()
        property string busLabel: confirmDialog.visible ? app.selectedBus() : ""
        property string serialLabel: confirmDialog.visible ? app.selectedSerial() : ""
        header: DialogHeader {
            tint: "#C0392B"
            iconGlyph: "⚠"
            title: qsTr("Erase device?")
            subtitle: qsTr("All data on the target will be permanently lost")
        }
        // A verb-labelled accept button ("Erase device") reads far clearer
        // than a generic "OK" on a destructive action. Full custom footer
        // (rather than relabelling a standardButton) because mixing custom
        // buttons with Dialog.Ok/Cancel breaks the accept signal in Qt 6.
        footer: DialogButtonBox {
            standardButtons: DialogButtonBox.Cancel
            Button {
                text: app.method === 3 && app.ventoyUpdate
                    ? qsTr("Update Ventoy")
                    : qsTr("Erase device")
                DialogButtonBox.buttonRole: DialogButtonBox.AcceptRole
                onClicked: { app.start(); confirmDialog.close() }
            }
        }
        // The card + the three labels carry no state at startup. Only
        // build them when the dialog opens; the Loader re-instantiates
        // on each open so the text bindings to invokables re-evaluate
        // with the current selection.
        contentItem: Loader {
            active: confirmDialog.visible
            sourceComponent: ScrollView {
            id: confirmScroll
            clip: true
            contentWidth: availableWidth
            ScrollBar.horizontal.policy: ScrollBar.AlwaysOff
            ScrollBar.vertical.policy: ScrollBar.AsNeeded
            ColumnLayout {
            width: confirmScroll.availableWidth
            spacing: 12
            // Target device card.
            Rectangle {
                Layout.fillWidth: true
                radius: 8
                color: confirmDialog.internalDisk
                    ? Qt.tint(palette.base, Qt.rgba(0.75, 0.23, 0.17, 0.14))
                    : Qt.tint(palette.base, Qt.rgba(palette.highlight.r,
                                                   palette.highlight.g,
                                                   palette.highlight.b, 0.10))
                border.color: confirmDialog.internalDisk
                    ? "#C0392B" : palette.mid
                implicitHeight: cardCol.implicitHeight + 24
                ColumnLayout {
                    id: cardCol
                    anchors.fill: parent
                    anchors.margins: 12
                    spacing: 2
                    Label {
                        // Each open re-instantiates the Loader, so the
                        // invokable is re-called with a fresh selection.
                        text: app.selectedModel()
                        font.bold: true
                        font.pointSize: 13
                        elide: Text.ElideRight
                        Layout.fillWidth: true
                    }
                    RowLayout {
                        Layout.fillWidth: true
                        spacing: 10
                        Label {
                            text: app.selectedSizeText()
                            color: palette.windowText
                            font.pointSize: 11
                        }
                        Pill {
                            visible: confirmDialog.busLabel !== ""
                            label: confirmDialog.busLabel
                            tint: palette.mid
                            ink: palette.windowText
                        }
                        Item { Layout.fillWidth: true }
                        Label {
                            text: app.selectedPath()
                            color: palette.placeholderText
                            font.family: "monospace"
                            font.pointSize: 10
                            elide: Text.ElideMiddle
                            horizontalAlignment: Text.AlignRight
                        }
                    }
                    Label {
                        visible: confirmDialog.serialLabel !== ""
                        text: qsTr("Serial: %1").arg(confirmDialog.serialLabel)
                        color: palette.placeholderText
                        font.family: "monospace"
                        font.pointSize: 9
                        elide: Text.ElideRight
                        Layout.fillWidth: true
                    }
                    Label {
                        visible: confirmDialog.internalDisk
                        Layout.fillWidth: true
                        Layout.topMargin: 6
                        text: qsTr("⚠ This is an INTERNAL (non-removable) disk. "
                                 + "Make absolutely sure it is the device you mean to erase.")
                        // Red-shifted from the theme's main text colour so
                        // the warning pops on both light and dark themes.
                        color: Qt.tint(host.palette.windowText,
                                       Qt.rgba(0.86, 0.21, 0.27, 0.85))
                        wrapMode: Text.Wrap
                        font.bold: true
                        font.pointSize: 9
                    }
                }
            }
            // Secondary action: opens the read-only Inspect modal (lsblk /
            // udevadm / smartctl dump). Kept inside the contentItem rather
            // than the dialog footer because mixing custom footer buttons
            // with Dialog.Ok / Dialog.Cancel breaks the accept signal in
            // Qt 6 Quick Controls.
            Button {
                flat: true
                Layout.alignment: Qt.AlignLeft
                text: qsTr("🔍  Inspect device details…")
                ToolTip.delay: 500
                ToolTip.visible: hovered
                ToolTip.text: qsTr("Open lsblk + udevadm + smartctl output for this device "
                    + "in a read-only panel. Useful if anything above looks off.")
                onClicked: {
                    // Kick the worker, then open the dialog right away.
                    // The TextArea binds to app.inspectText, so it shows
                    // the placeholder while lsblk/udevadm/smartctl run.
                    app.requestInspect()
                    confirmDialog.inspectRequested()
                }
            }
            Label {
                text: app.method === 3 && app.ventoyUpdate
                    ? qsTr("Ventoy will be updated. Your existing ISOs on the data partition are kept.")
                    : qsTr("All data on this device will be permanently erased.")
                wrapMode: Text.Wrap
                Layout.fillWidth: true
            }
            Label {
                text: qsTr("This cannot be undone.")
                font.bold: true
            }
            } // column
            } // scroll view
        }
    }
