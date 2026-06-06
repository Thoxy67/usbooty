import QtQuick
import QtQuick.Controls

// Reusable split button: a main action + an attached dropdown.
Control {
    id: sb
    property string text: ""
    signal clicked()
    signal menuRequested()

    padding: 1
    implicitHeight: 32
    implicitWidth: splitRow.implicitWidth + 2

    background: Rectangle {
        radius: 4
        color: sb.palette.button
        border.color: sb.palette.mid
    }

    contentItem: Row {
        id: splitRow
        opacity: sb.enabled ? 1.0 : 0.5

        // Main action zone.
        Rectangle {
            height: sb.availableHeight
            width: mainText.implicitWidth + 26
            radius: 3
            color: mainArea.pressed ? Qt.darker(sb.palette.button, 1.25)
                 : mainArea.containsMouse ? Qt.lighter(sb.palette.button, 1.08)
                 : "transparent"
            Label {
                id: mainText
                anchors.centerIn: parent
                text: sb.text
                color: sb.palette.buttonText
            }
            MouseArea {
                id: mainArea
                anchors.fill: parent
                enabled: sb.enabled
                hoverEnabled: true
                onClicked: sb.clicked()
            }
        }
        // Divider.
        Rectangle {
            width: 1
            height: sb.availableHeight
            color: sb.palette.mid
        }
        // Dropdown-arrow zone.
        Rectangle {
            height: sb.availableHeight
            width: 24
            radius: 3
            color: arrowArea.pressed ? Qt.darker(sb.palette.button, 1.25)
                 : arrowArea.containsMouse ? Qt.lighter(sb.palette.button, 1.08)
                 : "transparent"
            Label {
                anchors.centerIn: parent
                text: "▾"
                color: sb.palette.buttonText
            }
            MouseArea {
                id: arrowArea
                anchors.fill: parent
                enabled: sb.enabled
                hoverEnabled: true
                onClicked: sb.menuRequested()
            }
        }
    }
}
