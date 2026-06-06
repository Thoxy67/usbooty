import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import com.usbooty

Dialog {
    id: inspectDialog
    required property var app
    required property var host
        anchors.centerIn: parent
        width: Math.min(host.width - 60, 720)
        height: Math.min(host.height - 80, 560)
        modal: true
        topPadding: 14
        bottomPadding: 14
        leftPadding: 18
        rightPadding: 18
        header: DialogHeader {
            tint: palette.highlight
            iconGlyph: "ⓘ"
            title: qsTr("Device details")
            subtitle: qsTr("Read-only: lsblk + udevadm output for the chosen device")
        }
        standardButtons: Dialog.Close
        // The monospace TextArea only matters when the user has opened
        // the inspect panel; until then it doesn't need to exist.
        contentItem: Loader {
            active: inspectDialog.visible
            sourceComponent: ScrollView {
            id: inspectScroll
            clip: true
            ScrollBar.vertical.policy: ScrollBar.AsNeeded
            TextArea {
                id: inspectText
                width: inspectScroll.availableWidth
                readOnly: true
                wrapMode: TextArea.NoWrap
                font.family: "monospace"
                font.pointSize: 9
                selectByMouse: true
                // Worker output lands here via the qproperty. The body of
                // the bind shows the placeholder while the children run.
                text: app.inspectText
            }
            }
        }
    }
