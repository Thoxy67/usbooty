import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import com.usbooty

Dialog {
    id: shellDialog
    required property var app
    required property var host
        anchors.centerIn: parent
        width: Math.min(500, host.width - 40)
        // Hug the content, but never overflow a short host window; the
        // inner ScrollView keeps every step reachable when clamped.
        height: Math.min(implicitHeight, host.height - 40)
        modal: true
        topPadding: 14
        bottomPadding: 14
        leftPadding: 18
        rightPadding: 18
        header: DialogHeader {
            tint: "#2b579a"
            iconGlyph: ">_"
            title: qsTr("Download a UEFI Shell")
            subtitle: qsTr("Grab an EFI Shell ISO from the pbatard/UEFI-Shell project")
        }
        standardButtons: Dialog.Close
        // Build the combo only when the dialog actually opens; nothing here
        // is needed at startup.
        contentItem: Loader {
            active: shellDialog.visible
            sourceComponent: ScrollView {
            id: shellScroll
            clip: true
            contentWidth: availableWidth
            ScrollBar.horizontal.policy: ScrollBar.AlwaysOff
            ScrollBar.vertical.policy: ScrollBar.AsNeeded
            ColumnLayout {
            width: shellScroll.availableWidth
            spacing: 10
            Label {
                text: qsTr("The UEFI Shell is a small bootable command environment, "
                    + "handy for inspecting firmware, editing boot entries, or "
                    + "running EFI tools. These are direct downloads, so no extra "
                    + "lookups are needed.")
                wrapMode: Text.Wrap
                Layout.fillWidth: true
            }

            // Step 1: choose a build, then download.
            Label {
                text: qsTr("1.  Choose a build and download")
                font.bold: true
                Layout.topMargin: 4
            }
            RowLayout {
                Layout.fillWidth: true
                FormCombo {
                    id: shellBuild
                    Layout.fillWidth: true
                    enabled: !app.busy && app.uefiShells !== ""
                    // The list (version, release, Release/Debug variant) is
                    // owned by the Rust side; brand strings stay untranslated.
                    model: app.uefiShells !== "" ? app.uefiShells.split("\n") : []
                }
                Button {
                    text: qsTr("Download")
                    highlighted: true
                    enabled: !app.busy && app.uefiShells !== ""
                    onClicked: {
                        app.uefiDownload(shellBuild.currentIndex)
                        shellDialog.close()
                    }
                }
            }

            Label {
                text: Ui.trMsg(app.status)
                color: palette.placeholderText
                wrapMode: Text.Wrap
                Layout.fillWidth: true
            }

            Label {
                text: qsTr("Builds are hosted on GitHub. To browse every release "
                    + "or verify checksums, open the project's releases page:")
                color: palette.placeholderText
                wrapMode: Text.Wrap
                Layout.fillWidth: true
            }
            Button {
                text: qsTr("Open the UEFI-Shell releases page")
                Layout.fillWidth: true
                onClicked: Qt.openUrlExternally(
                    "https://github.com/pbatard/UEFI-Shell/releases")
            }
            } // column
            } // scroll view
        }
    }
