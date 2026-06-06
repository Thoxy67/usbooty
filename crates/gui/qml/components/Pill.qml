import QtQuick
import QtQuick.Controls

// Reusable coloured pill (used for OS chips, phase chips, …).
Rectangle {
    id: pill
    property string label: ""
    // Default falls back to the system highlight colour so a pill with no
    // explicit tint matches whatever Qt theme is in use. `palette` here is
    // the pill's own, inherited from its visual parent (the window).
    property color tint: pill.palette.highlight
    property color ink: pill.palette.highlightedText
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
