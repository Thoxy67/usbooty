import QtQuick

// Reusable Windows logo (modern 2×2 squares, Microsoft blue).
Item {
    id: wlogo
    property real size: 18
    property color tint: "#0078D4"
    implicitWidth: size
    implicitHeight: size
    Grid {
        anchors.fill: parent
        columns: 2
        rows: 2
        spacing: Math.max(1, wlogo.size * 0.08)
        Repeater {
            model: 4
            Rectangle {
                width: (wlogo.size - parent.spacing) / 2
                height: (wlogo.size - parent.spacing) / 2
                color: wlogo.tint
            }
        }
    }
}
