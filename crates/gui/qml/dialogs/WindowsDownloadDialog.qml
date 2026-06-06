import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import com.usbooty

Dialog {
    id: winDialog
    required property var app
    required property var host
        anchors.centerIn: parent
        width: Math.min(500, host.width - 40)
        modal: true
        // Frame the contents away from the coloured header so the layout
        // mirrors the Windows Setup dialog above.
        topPadding: 14
        bottomPadding: 14
        leftPadding: 18
        rightPadding: 18
        header: DialogHeader {
            tint: "#0078D4"
            iconComponent: WindowsLogo { size: 24; tint: "white" }
            title: qsTr("Download a Windows ISO")
            subtitle: qsTr("Pull an official image directly from Microsoft")
        }
        standardButtons: Dialog.Close
        // Only build the three combo boxes + the Microsoft-fetch buttons
        // when the dialog actually opens. Nothing inside this contentItem
        // is needed at startup.
        contentItem: Loader {
            active: winDialog.visible
            sourceComponent: ColumnLayout {
            spacing: 10
            Label {
                text: qsTr("Fetch an official ISO from Microsoft. Each step queries "
                    + "Microsoft and may take a few seconds.")
                wrapMode: Text.Wrap
                Layout.fillWidth: true
            }

            // Step 1: choose the Windows release.
            Label {
                text: qsTr("1.  Choose a Windows release")
                font.bold: true
                Layout.topMargin: 4
            }
            RowLayout {
                Layout.fillWidth: true
                FormCombo {
                    id: winVersion
                    Layout.fillWidth: true
                    enabled: !app.busy
                    // Windows release names are brand identifiers; leave untranslated.
                    model: ["Windows 11", "Windows 10"]
                }
                Button {
                    text: qsTr("List languages")
                    enabled: !app.busy
                    onClicked: app.winFetchLanguages(winVersion.currentIndex)
                }
            }

            // Step 2: choose a language.
            Label {
                text: qsTr("2.  Choose a language")
                font.bold: true
                enabled: app.winLanguages !== ""
                Layout.topMargin: 4
            }
            RowLayout {
                Layout.fillWidth: true
                FormCombo {
                    id: winLang
                    Layout.fillWidth: true
                    enabled: !app.busy && app.winLanguages !== ""
                    model: app.winLanguages !== "" ? app.winLanguages.split("\n") : []
                }
                Button {
                    text: qsTr("List downloads")
                    enabled: !app.busy && app.winLanguages !== ""
                    onClicked: app.winFetchOptions(winLang.currentIndex)
                }
            }

            // Step 3: choose an edition/architecture and download.
            Label {
                text: qsTr("3.  Choose an edition and download")
                font.bold: true
                enabled: app.winOptions !== ""
                Layout.topMargin: 4
            }
            RowLayout {
                Layout.fillWidth: true
                FormCombo {
                    id: winOpt
                    Layout.fillWidth: true
                    enabled: !app.busy && app.winOptions !== ""
                    model: app.winOptions !== "" ? app.winOptions.split("\n") : []
                }
                Button {
                    text: qsTr("Download")
                    highlighted: true
                    enabled: !app.busy && app.winOptions !== ""
                    onClicked: {
                        app.winDownload(winOpt.currentIndex)
                        winDialog.close()
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
                text: qsTr("If Microsoft's anti-bot system rejects the request "
                    + "(common on VPNs and some networks), download manually:")
                color: palette.placeholderText
                wrapMode: Text.Wrap
                Layout.fillWidth: true
            }
            Button {
                text: qsTr("Open Microsoft download page")
                Layout.fillWidth: true
                onClicked: app.openMicrosoftPage(winVersion.currentIndex)
            }
            }
        }
    }
