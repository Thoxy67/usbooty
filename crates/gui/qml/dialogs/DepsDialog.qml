import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import com.usbooty

Dialog {
    id: depsDialog
    required property var app
    required property var host
        anchors.centerIn: parent
        width: Math.min(560, host.width - 40)
        height: Math.min(host.height - 80, 600)
        modal: true
        topPadding: 14
        bottomPadding: 14
        leftPadding: 18
        rightPadding: 18
        // Parsed rows from app.dependencyReport(); refreshed on open.
        property var rows: []
        function refresh() {
            var out = []
            var lines = app.dependencyReport().split("\n")
            for (var i = 0; i < lines.length; i++) {
                if (lines[i] === "")
                    continue
                var f = lines[i].split("")
                out.push({ present: f[0] === "1", group: f[1],
                           name: f[2], pkg: f[3], purpose: f[4] })
            }
            rows = out
        }
        readonly property int presentCount:
            rows.filter(function(d) { return d.present }).length
        header: DialogHeader {
            tint: palette.highlight
            iconGlyph: "ⓘ"
            title: qsTr("Dependencies")
            subtitle: qsTr("%1 of %2 present").arg(depsDialog.presentCount)
                                              .arg(depsDialog.rows.length)
        }
        standardButtons: Dialog.Close
        contentItem: ScrollView {
            id: depsScroll
            clip: true
            ScrollBar.vertical.policy: ScrollBar.AsNeeded
            ScrollBar.horizontal.policy: ScrollBar.AlwaysOff
            ColumnLayout {
                width: depsScroll.availableWidth
                spacing: 12
                Repeater {
                    model: [
                        { key: "required",   title: qsTr("Required") },
                        { key: "filesystem", title: qsTr("Filesystem formatters") },
                        { key: "feature",    title: qsTr("Feature backends") },
                        { key: "boot",       title: qsTr("Boot test (QEMU)") },
                        { key: "qol",        title: qsTr("Quality-of-life") }
                    ]
                    delegate: ColumnLayout {
                        id: section
                        // The category key this section filters the dep rows by.
                        readonly property string catKey: modelData.key
                        Layout.fillWidth: true
                        spacing: 4
                        Label {
                            text: modelData.title
                            font.bold: true
                            font.pointSize: 11
                            color: palette.highlight
                            Layout.fillWidth: true
                        }
                        Repeater {
                            model: depsDialog.rows.filter(function(d) {
                                return d.group === section.catKey
                            })
                            delegate: RowLayout {
                                id: deprow
                                Layout.fillWidth: true
                                Layout.leftMargin: 6
                                spacing: 8
                                Label {
                                    text: modelData.present ? "✓" : "✗"
                                    color: modelData.present ? "#27AE60" : "#E74C3C"
                                    font.bold: true
                                    font.pointSize: 11
                                    Layout.alignment: Qt.AlignTop
                                }
                                ColumnLayout {
                                    Layout.fillWidth: true
                                    spacing: 0
                                    RowLayout {
                                        Layout.fillWidth: true
                                        spacing: 8
                                        Label {
                                            text: modelData.name
                                            font.bold: true
                                            elide: Text.ElideRight
                                        }
                                        Label {
                                            text: modelData.pkg
                                            font.family: "monospace"
                                            font.pointSize: 8
                                            color: palette.placeholderText
                                            elide: Text.ElideRight
                                        }
                                        Item { Layout.fillWidth: true }
                                        Label {
                                            text: modelData.present
                                                ? qsTr("available") : qsTr("missing")
                                            color: modelData.present ? "#27AE60" : "#E74C3C"
                                            font.pointSize: 8
                                        }
                                    }
                                    Label {
                                        text: modelData.purpose
                                        color: palette.placeholderText
                                        font.pointSize: 8
                                        wrapMode: Text.Wrap
                                        Layout.fillWidth: true
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
