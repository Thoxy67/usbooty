import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import com.usbooty

Dialog {
    id: checkConfirm
    // The AppController and the root window, passed in from main.qml.
    required property var app
    required property var host
    anchors.centerIn: parent
    // Clamp to the window's current width so the dialog never overflows
    // when the user shrinks the window; the 40 px buffer leaves room
    // for the modal backdrop and any window-manager shadow.
    width: Math.min(480, host.width - 40)
    modal: true
    topPadding: 14
    bottomPadding: 14
    leftPadding: 18
    rightPadding: 18
    property int mode: 0
    function openFor(m) {
        checkConfirm.mode = m
        checkConfirm.open()
    }
    header: DialogHeader {
        // Quick = amber lightning ("fast spot-check"); Full = red warning
        // triangle ("slow exhaustive scan"). Same destructive verb, very
        // different effort + duration, hence the distinct visual weight.
        tint: checkConfirm.mode === 1 ? "#C0392B" : "#E67E22"
        iconGlyph: checkConfirm.mode === 1 ? "⚠" : "⚡"
        title: checkConfirm.mode === 1
            ? qsTr("Full bad-blocks scan?")
            : qsTr("Quick fake-drive check?")
        subtitle: checkConfirm.mode === 1
            ? qsTr("Writes two patterns across every sector, slow and exhaustive")
            : qsTr("Writes a fingerprint at ~256 sample positions, finishes in seconds")
    }
    standardButtons: Dialog.Ok | Dialog.Cancel
    onAccepted: app.startCheck(checkConfirm.mode)
    contentItem: ColumnLayout {
        spacing: 8
        Label {
            text: checkConfirm.mode === 1
                ? qsTr("The full scan will write two patterns across every sector of the selected device, "
                     + "then read them back. It is slow and destroys all data on the device.")
                : qsTr("The quick check writes a unique fingerprint at ~256 sample positions and reads "
                     + "them back. It finishes in seconds, but destroys any data on the selected device.")
            wrapMode: Text.Wrap
            Layout.fillWidth: true
        }
        Label {
            text: qsTr("This cannot be undone.")
            font.bold: true
        }
    }
}
