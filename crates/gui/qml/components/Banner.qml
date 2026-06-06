import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

// Reusable coloured advisory banner.
// Severity picks the colour pair; the actual hex is alpha-blended over
// the system palette so the banner stays legible on both light and dark
// Qt themes without hard-coding a theme.
Rectangle {
    id: banner
    property string message: ""
    property string severity: "warn" // "info" | "warn" | "error"
    readonly property var _accents: ({
        "info":  { line: Qt.rgba(0.13, 0.59, 0.95, 1.0), alpha: 0.12 },
        "warn":  { line: Qt.rgba(0.88, 0.66, 0.00, 1.0), alpha: 0.16 },
        "error": { line: Qt.rgba(0.86, 0.21, 0.27, 1.0), alpha: 0.16 },
    })
    readonly property var _accent: _accents[severity] || _accents["warn"]
    Layout.fillWidth: true
    visible: message !== ""
    implicitHeight: visible ? bannerLabel.implicitHeight + 18 : 0
    // Tint the system base colour with the severity hue. Works on both
    // dark and light themes because the underlying base shifts with the
    // theme and the tint is just a hint on top of it.
    color: Qt.tint(banner.palette.base, Qt.rgba(_accent.line.r, _accent.line.g,
                                         _accent.line.b, _accent.alpha))
    border.color: _accent.line
    radius: 6
    Label {
        id: bannerLabel
        anchors.fill: parent
        anchors.margins: 9
        text: banner.message
        color: banner.palette.windowText
        wrapMode: Text.Wrap
    }
}
