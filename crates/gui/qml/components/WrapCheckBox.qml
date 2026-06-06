import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

// Reusable CheckBox whose label wraps on long translations.
// The default CheckBox keeps its label on a single line and lets the
// text overflow when it doesn't fit. Override contentItem with a
// Label that:
//   * sets wrapMode: WordWrap;
//   * binds its width to the control width so WordWrap actually
//     triggers; without an explicit width the Label measures its
//     own desired width and Qt never gives WordWrap a box to break
//     against.
// Layout.fillWidth + minimumWidth: 0 lets the control take the cell
// width without refusing to shrink; the parent layout sizes it, the
// contentItem then wraps inside.
CheckBox {
    id: wcb
    Layout.fillWidth: true
    Layout.minimumWidth: 0
    contentItem: Label {
        text: wcb.text
        font: wcb.font
        color: wcb.palette.windowText
        wrapMode: Text.WordWrap
        verticalAlignment: Text.AlignVCenter
        leftPadding: wcb.indicator.width + wcb.spacing
        width: wcb.width
    }
}
