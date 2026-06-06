import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

// Reusable ComboBox that elides + lets itself shrink.
// Qt's default ComboBox sets implicitWidth from its widest item, which
// makes a long-translated entry overflow tight layouts (e.g. our
// 2-column GridLayout for Options). Overriding contentItem + delegate
// forces both the selected-value display and the dropdown items to
// elide on the right. Layout.minimumWidth: 0 lets the parent shrink
// the combo without hitting Qt's implicit minimum.
ComboBox {
    id: fc
    Layout.fillWidth: true
    Layout.minimumWidth: 0
    contentItem: Label {
        text: fc.displayText
        leftPadding: 10
        rightPadding: fc.indicator
            ? fc.indicator.width + fc.spacing : 30
        elide: Text.ElideRight
        color: fc.palette.text
        verticalAlignment: Text.AlignVCenter
    }
    // Cap the popup to the combo's width so dropdown items can't
    // overflow horizontally either.
    popup.width: fc.width
    delegate: ItemDelegate {
        width: fc.popup.width
        highlighted: fc.highlightedIndex === index
        contentItem: Label {
            text: modelData
            color: fc.palette.text
            elide: Text.ElideRight
            verticalAlignment: Text.AlignVCenter
        }
    }
}
