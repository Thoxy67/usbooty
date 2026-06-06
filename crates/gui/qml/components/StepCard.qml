import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

// Reusable numbered section card. The three left-column steps use one each.
Frame {
    id: card
    property int step: 0
    property string heading: ""
    // Per-step accent: colours the round step badge and the card's left
    // edge so the three cards have a quick visual rhythm.
    property color accent: card.palette.highlight
    default property alias body: bodyColumn.data
    Layout.fillWidth: true
    padding: 10
    background: Rectangle {
        radius: 8
        color: card.palette.base
        border.color: card.palette.mid
        // 3-pixel coloured stripe down the left edge; clipped by `radius`
        // so it follows the card's rounded corners.
        Rectangle {
            anchors.left: parent.left
            anchors.top: parent.top
            anchors.bottom: parent.bottom
            width: 3
            color: card.accent
            opacity: 0.85
        }
    }
    contentItem: ColumnLayout {
        spacing: 8
        RowLayout {
            Layout.fillWidth: true
            spacing: 8
            Rectangle {
                width: 22
                height: 22
                radius: 11
                color: card.accent
                Label {
                    anchors.centerIn: parent
                    text: card.step
                    color: "white"
                    font.bold: true
                }
            }
            Label {
                text: card.heading
                font.bold: true
                font.pointSize: 11
                Layout.fillWidth: true
            }
        }
        ColumnLayout {
            id: bodyColumn
            Layout.fillWidth: true
            spacing: 6
        }
    }
}
