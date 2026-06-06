import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

// Reusable themed dialog header. A 52-px coloured strip carrying an icon +
// title + subtitle. Every top-level Dialog uses one so the app's modal
// surface feels coherent: blue for Microsoft flows, red for destructive
// prompts, green/red for result feedback, palette.highlight for neutral/info
// screens.
Rectangle {
    id: dh
    property color tint: dh.palette.highlight
    property string title: ""
    property string subtitle: ""
    // Either a Unicode glyph (⚠ ✓ ✕ ⓘ) OR a Component to instantiate
    // for fully-custom marks (used by the Microsoft dialogs to render
    // their 2×2 WindowsLogo). If `iconComponent` is set it wins.
    property string iconGlyph: ""
    property Component iconComponent: null
    color: tint
    // The header grows with its content so long translated subtitles
    // wrap instead of clipping. 52 px stays the floor for the
    // single-line English case.
    implicitHeight: Math.max(52, headerRow.implicitHeight + 16)
    Layout.fillWidth: true
    RowLayout {
        id: headerRow
        anchors.fill: parent
        anchors.leftMargin: 16
        anchors.rightMargin: 16
        anchors.topMargin: 8
        anchors.bottomMargin: 8
        spacing: 12
        Loader {
            active: dh.iconComponent !== null
            sourceComponent: dh.iconComponent
            visible: active
            Layout.alignment: Qt.AlignVCenter
        }
        Label {
            visible: dh.iconComponent === null && dh.iconGlyph !== ""
            text: dh.iconGlyph
            color: "white"
            font.pointSize: 22
            font.bold: true
            Layout.alignment: Qt.AlignVCenter
        }
        ColumnLayout {
            spacing: 0
            Layout.fillWidth: true
            Label {
                text: dh.title
                color: "white"
                font.bold: true
                font.pointSize: 12
                Layout.fillWidth: true
                elide: Text.ElideRight
            }
            Label {
                text: dh.subtitle
                visible: text !== ""
                color: Qt.rgba(1, 1, 1, 0.82)
                font.pointSize: 8
                // Subtitle is the most likely string to balloon in
                // translation; wrap rather than elide so the whole
                // sentence remains visible (the header just grows).
                Layout.fillWidth: true
                wrapMode: Text.WordWrap
            }
        }
    }
}
