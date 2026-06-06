import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import com.usbooty

Dialog {
    id: aboutDialog
    required property var app
    required property var host
        anchors.centerIn: parent
        // Tight cap; the dialog has to fit comfortably inside the 660 px
        // compact window so it never spills past the parent.
        width: Math.min(500, host.width - 40)
        modal: true
        topPadding: 14
        bottomPadding: 12
        leftPadding: 18
        rightPadding: 18
        header: DialogHeader {
            // Deep blurple, close to Discord's brand mark (#5865F2) but a
            // few shades darker, so the white USBooty logo + title pop
            // without competing with the Microsoft-blue / red Erase headers.
            tint: "#4752C4"
            iconComponent: Image {
                source: "qrc:/icons/usbooty.svg"
                sourceSize.width: 28
                sourceSize.height: 28
                width: 28
                height: 28
                fillMode: Image.PreserveAspectFit
                smooth: true
            }
            title: "USBooty"
            subtitle: qsTr("Bootable USB Creator · Version %1").arg(app.appVersion)
        }
        standardButtons: Dialog.Ok
        // The 64 px SVG logo, the GridLayout, the link-handling Labels and
        // the three external-launch Buttons cost nothing at startup if
        // they don't exist yet.
        contentItem: Loader {
            active: aboutDialog.visible
            sourceComponent: ColumnLayout {
            spacing: 12

            // Centred logo + one-line tagline; small enough to keep the
            // dialog short on the compact 660 px window.
            Image {
                source: "qrc:/icons/usbooty.svg"
                sourceSize.width: 64
                sourceSize.height: 64
                Layout.preferredWidth: 64
                Layout.preferredHeight: 64
                Layout.alignment: Qt.AlignHCenter
                Layout.topMargin: 4
                fillMode: Image.PreserveAspectFit
                smooth: true
            }
            Label {
                text: qsTr("Create bootable USB drives from ISO images.")
                wrapMode: Text.Wrap
                horizontalAlignment: Text.AlignHCenter
                Layout.fillWidth: true
            }

            // Author + License + Source on one tidy row each.
            GridLayout {
                columns: 2
                columnSpacing: 14
                rowSpacing: 3
                Layout.fillWidth: true
                Layout.topMargin: 4
                Label { text: qsTr("Author"); font.bold: true }
                Label { text: "Thoxy" }
                Label { text: qsTr("License"); font.bold: true }
                Label {
                    Layout.fillWidth: true
                    text: "<a href=\"https://www.gnu.org/licenses/gpl-3.0.html\">GPL-3.0-or-later</a>"
                    textFormat: Text.RichText
                    onLinkActivated: function(link) { Qt.openUrlExternally(link) }
                }
                Label { text: qsTr("Source"); font.bold: true }
                Label {
                    Layout.fillWidth: true
                    elide: Text.ElideRight
                    text: "<a href=\"https://git.thoxy.xyz/thoxy/usbooty\">git.thoxy.xyz/thoxy/usbooty</a>"
                    textFormat: Text.RichText
                    onLinkActivated: function(link) { Qt.openUrlExternally(link) }
                }
            }

            // Single-line "what it does", replaces the four-column
            // feature grid that was overflowing the compact window.
            Label {
                Layout.fillWidth: true
                Layout.topMargin: 4
                color: palette.placeholderText
                font.pointSize: 9
                wrapMode: Text.Wrap
                text: qsTr("DD raw / partition+copy / format / Ventoy / FreeDOS · "
                    + "FAT16-32, NTFS, exFAT, UDF, ext2/3/4, Btrfs, XFS, F2FS · "
                    + "Linux persistence · Windows 11 setup customisation · "
                    + "BLAKE3 verify · SBAT + DBX revocation · SMART probe.")
            }

            // Quick action row: Docs / Source-code / Report-issue.
            RowLayout {
                Layout.fillWidth: true
                Layout.topMargin: 2
                spacing: 4
                Button {
                    text: qsTr("Docs")
                    icon.name: "help-contents"
                    display: icon.name
                        ? AbstractButton.TextBesideIcon
                        : AbstractButton.TextOnly
                    flat: true
                    onClicked: Qt.openUrlExternally(
                        "https://git.thoxy.xyz/thoxy/usbooty/wiki")
                }
                Button {
                    text: qsTr("Source code")
                    icon.name: "applications-development"
                    display: icon.name
                        ? AbstractButton.TextBesideIcon
                        : AbstractButton.TextOnly
                    flat: true
                    onClicked: Qt.openUrlExternally("https://git.thoxy.xyz/thoxy/usbooty")
                }
                Button {
                    text: qsTr("Report an issue")
                    icon.name: "tools-report-bug"
                    display: icon.name
                        ? AbstractButton.TextBesideIcon
                        : AbstractButton.TextOnly
                    flat: true
                    onClicked: Qt.openUrlExternally("https://git.thoxy.xyz/thoxy/usbooty/issues")
                }
                Item { Layout.fillWidth: true }
            }
            }
        }
    }
