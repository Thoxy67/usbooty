import QtQuick
import QtQuick.Controls
import com.usbooty

Dialog {
    id: resultDialog
    required property var app
    required property var host
    anchors.centerIn: parent
    width: Math.min(480, host.width - 40)
    modal: true
    topPadding: 14
    bottomPadding: 14
    leftPadding: 18
    rightPadding: 18
    // Set by the AppController.onJobFinished handler (in main.qml); drive the
    // header colour, the title text, and the body message.
    property bool success: true
    property string message: ""
    header: DialogHeader {
        tint: resultDialog.success ? "#27AE60" : "#E74C3C"
        iconGlyph: resultDialog.success ? "✓" : "✕"
        title: resultDialog.success ? qsTr("Finished") : qsTr("Failed")
        subtitle: resultDialog.success
            ? qsTr("The device is ready to use.")
            : qsTr("The job did not complete. See details below.")
    }
    // Custom footer so the success case can offer an "Eject" action
    // alongside Close, without the standard-button reordering trickery.
    footer: DialogButtonBox {
        standardButtons: DialogButtonBox.Close
        Button {
            text: qsTr("Eject device")
            visible: resultDialog.success && app.selectedDevice >= 0
            DialogButtonBox.buttonRole: DialogButtonBox.ActionRole
            onClicked: {
                app.ejectDevice()
                resultDialog.close()
            }
        }
    }
    contentItem: Label {
        id: resultLabel
        text: resultDialog.message
        wrapMode: Text.Wrap
    }
}
