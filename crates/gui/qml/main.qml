import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Dialogs
import QtQuick.Window
import com.usbooty

ApplicationWindow {
    id: window
    visible: true
    width: 660
    // Height is content-driven (see binding below). Keep a generous floor
    // so a freshly-launched window doesn't snap to a sliver while QML is
    // still computing its first layout pass.
    // 600 controls-column floor + 24 px RowLayout margins.
    minimumWidth: 624
    minimumHeight: 360
    // Auto-fit the window vertically to the left column's actual content:
    // hidden banners and the absent progress frame contribute zero height,
    // so the window shrinks/grows as the UI gains or loses sections.
    height: Math.max(minimumHeight,
                     mainCol.implicitHeight
                     + 24  // RowLayout top + bottom margins
                     + (menuBar ? menuBar.height : 0))
    Behavior on height {
        // No animation during a job: progress phases and banners would
        // trigger window resizes every few seconds, producing a visible
        // wobble. Idle resizes (toggling the log, switching method) keep
        // the eased transition.
        enabled: !app.busy
        NumberAnimation { duration: 120; easing.type: Easing.OutCubic }
    }
    // Title reflects the job state so progress is visible even when the
    // window is in the background / minimized to the taskbar.
    title: {
        if (app.busy && app.progress > 0)
            return "USBooty: " + window.trPhase(app.phase) + " "
                 + Math.round(app.progress * 100) + " %"
        if (app.busy)
            return "USBooty: " + window.trPhase(app.phase !== "" ? app.phase : "Working") + "…"
        return qsTr("USBooty: Bootable USB Creator")
    }

    AppController {
        id: app
        Component.onCompleted: {
            app.refreshDevices()
            app.applyStartupArgs()
        }
        // Reset the cancelling latch on every busy→idle transition so the
        // next job's Cancel button starts in the active state again.
        onBusyChanged: if (!app.busy) window.cancelling = false
        onJobFinished: function(success, message) {
            resultDialog.title = success ? "Success" : "Failed"
            resultDialog.success = success
            resultLabel.text = message
            resultDialog.open()
        }
    }

    // Latched true between the moment the user clicks Cancel and the moment
    // the runner emits its terminal Done/Error. Drives the Cancel button's
    // label + enabled state so the user gets visible feedback instead of
    // wondering whether the click registered.
    property bool cancelling: false

    // Poll for new / removed block devices while idle. Cheap (one sysfs walk
    // every 2.5 s); skipped while a job runs so we don't churn the combo's
    // model mid-write.
    Timer {
        interval: 2500
        repeat: true
        running: !app.busy
        onTriggered: app.refreshDevices()
    }

    // Whether the user can launch a job right now. Format-only (method 2)
    // and FreeDOS (method 4) need no source image; Ventoy (method 3)
    // treats the ISO as optional.
    readonly property bool ready:
        !app.busy && app.selectedDevice >= 0
        && (app.method === 2
            || app.method === 4
            || (app.method === 3 && app.fitWarning === "")
            || (app.isoPath !== "" && app.fitWarning === ""))

    // A wall-clock elapsed counter, ticking once a second while a job runs.
    property int elapsedSecs: 0
    Timer {
        interval: 1000
        repeat: true
        running: app.busy
        onTriggered: window.elapsedSecs++
    }
    Connections {
        target: app
        function onBusyChanged() {
            if (app.busy)
                window.elapsedSecs = 0
        }
    }

    // Translate one of the runtime phase strings emitted by the helper /
    // runner / bridge. The phases come from Rust as fixed English
    // identifiers, so we map them to static qsTr() literals here. That
    // lets lupdate extract them and the QTranslator actually find a
    // translation at runtime — `qsTr(app.phase)` would never match,
    // because lupdate only sees the dynamic argument as a variable.
    function trPhase(p) {
        switch (p) {
        case "Starting":                     return qsTr("Starting")
        case "Analyzing":                    return qsTr("Analyzing")
        case "Decompressing":                return qsTr("Decompressing")
        case "Unwrapping VHD":               return qsTr("Unwrapping VHD")
        case "Partitioning":                 return qsTr("Partitioning")
        case "Formatting":                   return qsTr("Formatting")
        case "Erasing":                      return qsTr("Erasing")
        case "Writing":                      return qsTr("Writing")
        case "Reading":                      return qsTr("Reading")
        case "Copying":                      return qsTr("Copying")
        case "Copying ISO":                  return qsTr("Copying ISO")
        case "Copying FreeDOS files":        return qsTr("Copying FreeDOS files")
        case "Installing Syslinux":          return qsTr("Installing Syslinux")
        case "Installing FreeDOS boot sector": return qsTr("Installing FreeDOS boot sector")
        case "Applying distro fixes":        return qsTr("Applying distro fixes")
        case "Persistence":                  return qsTr("Persistence")
        case "Splitting install.wim":        return qsTr("Splitting install.wim")
        case "Verifying":                    return qsTr("Verifying")
        case "Writing samples":              return qsTr("Writing samples")
        case "Reading samples back":         return qsTr("Reading samples back")
        case "Flushing":                     return qsTr("Flushing")
        case "Downloading Windows ISO":      return qsTr("Downloading Windows ISO")
        case "Finished":                     return qsTr("Finished")
        case "Failed":                       return qsTr("Failed")
        case "Working":                      return qsTr("Working")
        }
        return p
    }

    // Translate a fixed status / ISO-summary string the Rust side emits in
    // English. Like trPhase, the qsTr() literals keep these in the catalog
    // for lupdate; interpolated / unknown messages (those carrying a path,
    // size, or error) fall through and stay in English.
    function trMsg(m) {
        switch (m) {
        case "Ready":                                   return qsTr("Ready")
        case "No image selected":                       return qsTr("No image selected")
        case "Cannot read that file":                   return qsTr("Cannot read that file")
        case "Analyzing source image…":                 return qsTr("Analyzing source image…")
        case "Decompressing source image…":             return qsTr("Decompressing source image…")
        case "Unwrapping fixed VHD…":                   return qsTr("Unwrapping fixed VHD…")
        case "Select an ISO and a target device first": return qsTr("Select an ISO and a target device first")
        case "Select a target device first":            return qsTr("Select a target device first")
        case "Contacting Microsoft…":                   return qsTr("Contacting Microsoft…")
        case "Fetching download options…":              return qsTr("Fetching download options…")
        case "Pick an output file for the backup":      return qsTr("Pick an output file for the backup")
        case "Cancelling…":                             return qsTr("Cancelling…")
        case "Select a device to boot-test first":      return qsTr("Select a device to boot-test first")
        case "No device selected":                      return qsTr("No device selected")
        }
        return m
    }

    // Format a second count as "1h 04m", "2m 12s" or "38s".
    function fmtTime(s) {
        if (s <= 0)
            return ""
        var h = Math.floor(s / 3600)
        var m = Math.floor((s % 3600) / 60)
        var sec = s % 60
        if (h > 0)
            return h + "h " + ("0" + m).slice(-2) + "m"
        if (m > 0)
            return m + "m " + ("0" + sec).slice(-2) + "s"
        return sec + "s"
    }

    // ---- Reusable numbered section card --------------------------------
    component StepCard: Frame {
        id: card
        property int step: 0
        property string heading: ""
        // Per-step accent — colours the round step badge and the card's left
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

    // ---- Reusable Windows logo (modern 2×2 squares, Microsoft blue) ----
    component WindowsLogo: Item {
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

    // ---- Reusable Linux mascot (a simplified Tux), the Linux counterpart
    // to WindowsLogo. Drawn with a Canvas (proportions scale to `size`) so it
    // needs no bundled asset. Tux is multi-colour by nature, so unlike the
    // monochrome WindowsLogo it ignores any tint.
    component LinuxLogo: Canvas {
        id: llogo
        property real size: 18
        implicitWidth: size
        implicitHeight: size
        width: size
        height: size
        onPaint: {
            var ctx = getContext("2d")
            var s = width
            ctx.reset()
            var black = "#2b2b2b"
            var white = "#ffffff"
            var orange = "#f6a623"

            // Feet first, so the body overlaps their tops and only the
            // outer edges peek out at the bottom.
            ctx.fillStyle = orange
            ctx.beginPath()
            ctx.ellipse(0.12 * s, 0.78 * s, 0.34 * s, 0.18 * s)
            ctx.ellipse(0.54 * s, 0.78 * s, 0.34 * s, 0.18 * s)
            ctx.fill()

            // Black body silhouette (egg shape).
            ctx.fillStyle = black
            ctx.beginPath()
            ctx.ellipse(0.16 * s, 0.06 * s, 0.68 * s, 0.86 * s)
            ctx.fill()

            // White belly.
            ctx.fillStyle = white
            ctx.beginPath()
            ctx.ellipse(0.30 * s, 0.40 * s, 0.40 * s, 0.50 * s)
            ctx.fill()

            // White eye patches.
            ctx.fillStyle = white
            ctx.beginPath()
            ctx.ellipse(0.36 * s, 0.18 * s, 0.13 * s, 0.20 * s)
            ctx.ellipse(0.51 * s, 0.18 * s, 0.13 * s, 0.20 * s)
            ctx.fill()

            // Black pupils.
            ctx.fillStyle = black
            ctx.beginPath()
            ctx.ellipse(0.41 * s, 0.24 * s, 0.06 * s, 0.10 * s)
            ctx.ellipse(0.53 * s, 0.24 * s, 0.06 * s, 0.10 * s)
            ctx.fill()

            // Orange beak between the eyes.
            ctx.fillStyle = orange
            ctx.beginPath()
            ctx.ellipse(0.42 * s, 0.34 * s, 0.16 * s, 0.10 * s)
            ctx.fill()
        }
    }

    // ---- Reusable coloured pill (used for OS chips, phase chips, …) ----
    component Pill: Rectangle {
        id: pill
        property string label: ""
        // Default falls back to the system highlight colour so a pill with no
        // explicit tint matches whatever Qt theme is in use.
        property color tint: window.palette.highlight
        property color ink: window.palette.highlightedText
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

    // ---- Reusable themed dialog header ----------------------------------
    // A 52-px coloured strip carrying an icon + title + subtitle. Every
    // top-level Dialog uses one so the app's modal surface feels coherent:
    // blue for Microsoft flows, red for destructive prompts, green/red for
    // result feedback, palette.highlight for neutral/info screens.
    component DialogHeader: Rectangle {
        id: dh
        property color tint: window.palette.highlight
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

    // ---- Reusable ComboBox that elides + lets itself shrink -------------
    // Qt's default ComboBox sets implicitWidth from its widest item, which
    // makes a long-translated entry overflow tight layouts (e.g. our
    // 2-column GridLayout for Options). Overriding contentItem + delegate
    // forces both the selected-value display and the dropdown items to
    // elide on the right. Layout.minimumWidth: 0 lets the parent shrink
    // the combo without hitting Qt's implicit minimum.
    component FormCombo: ComboBox {
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
                color: window.palette.text
                elide: Text.ElideRight
                verticalAlignment: Text.AlignVCenter
            }
        }
    }

    // ---- Reusable CheckBox whose label wraps on long translations -------
    // The default CheckBox keeps its label on a single line and lets the
    // text overflow when it doesn't fit. Override contentItem with a
    // Label that:
    //   * sets wrapMode: WordWrap;
    //   * binds its width to the control width so WordWrap actually
    //     triggers — without an explicit width the Label measures its
    //     own desired width and Qt never gives WordWrap a box to break
    //     against.
    // Layout.fillWidth + minimumWidth: 0 lets the control take the cell
    // width without refusing to shrink; the parent layout sizes it, the
    // contentItem then wraps inside.
    component WrapCheckBox: CheckBox {
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

    // ---- Reusable coloured advisory banner ------------------------------
    // Severity picks the colour pair; the actual hex is alpha-blended over
    // the system palette so the banner stays legible on both light and dark
    // Qt themes without hard-coding a theme.
    component Banner: Rectangle {
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
        color: Qt.tint(palette.base, Qt.rgba(_accent.line.r, _accent.line.g,
                                             _accent.line.b, _accent.alpha))
        border.color: _accent.line
        radius: 6
        Label {
            id: bannerLabel
            anchors.fill: parent
            anchors.margins: 9
            text: banner.message
            color: palette.windowText
            wrapMode: Text.Wrap
        }
    }

    // ---- Reusable split button: a main action + an attached dropdown ----
    component SplitButton: Control {
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

    // ---- Expanding side-by-side layout ---------------------------------
    // The activity log lives on the right and only appears when there is
    // something to read. The first log line auto-widens the window so the
    // controls keep their full width and the log gets its own column —
    // less disruptive than pushing the action button off-screen.
    readonly property int compactWidth: 660
    readonly property int expandedWidth: 1080
    property bool logExpanded: false
    // Hard floor for the controls column; the user can never drag the
    // separator narrower than this.
    readonly property int leftPanelMinWidth: 600
    // Current width of the controls column. Bound to the separator drag;
    // the log column (Layout.fillWidth) absorbs whatever is left over.
    property int leftPanelWidth: compactWidth - 24
    Behavior on width {
        NumberAnimation { duration: 220; easing.type: Easing.OutCubic }
    }
    // The width the window had right before the log column forced it wider.
    // Collapsing the log restores exactly this (e.g. the launch width) rather
    // than a fixed constant, so the window returns to where it actually was.
    property int preLogWidth: width
    // Resize the window to `w`, but never while it is maximized or
    // fullscreen: the user chose that geometry, and a width tweak would
    // either be silently ignored or snap the window to the wrong size the
    // moment they restore it. The log column still shows/hides via
    // logVisible; only the windowed-mode auto-resize is suppressed.
    function setWindowWidth(w) {
        if (window.visibility === Window.Maximized
                || window.visibility === Window.FullScreen)
            return
        window.width = w
    }
    // Grow to fit the log column, remembering the current width first (read
    // fresh each time, so a manual resize between toggles is honoured). No-op
    // and no remembering if the window is already wide enough.
    function growForLog() {
        if (window.width < window.expandedWidth) {
            window.preLogWidth = window.width
            window.setWindowWidth(window.expandedWidth)
        }
    }
    // Collapse the log column, restoring the remembered pre-log width.
    function shrinkAfterLog() {
        window.setWindowWidth(window.preLogWidth)
    }
    // Effective visibility of the activity log column: forced on by the
    // user setting, OR auto-expanded once the buffer holds something.
    readonly property bool logVisible: app.showLogsAlways || logExpanded
    Connections {
        target: app
        function onLogNonEmptyChanged() {
            if (app.logNonEmpty && !window.logExpanded) {
                window.logExpanded = true
                window.growForLog()
            }
        }
        function onShowLogsAlwaysChanged() {
            if (app.showLogsAlways) {
                // Flipped on: grow to make room for the log column.
                window.growForLog()
            } else if (!window.logExpanded) {
                // Flipped off and nothing else is keeping the panel open
                // (no log content auto-expanded it): reclaim the width.
                window.shrinkAfterLog()
            }
        }
    }
    Component.onCompleted: {
        // Honour the persisted "always show logs" choice on first paint.
        if (app.showLogsAlways)
            window.growForLog()
    }


    menuBar: MenuBar {
        Menu {
            title: qsTr("Device")
            MenuItem {
                text: qsTr("Quick check (fake-drive)…")
                enabled: !app.busy && app.selectedDevice >= 0
                onTriggered: checkConfirm.openFor(0)
            }
            MenuItem {
                text: qsTr("Full bad-blocks scan…")
                enabled: !app.busy && app.selectedDevice >= 0
                onTriggered: checkConfirm.openFor(1)
            }
            MenuSeparator { }
            MenuItem {
                text: qsTr("Save snapshot to file…")
                enabled: !app.busy && app.selectedDevice >= 0
                onTriggered: backupDialog.open()
            }
            // Only offered when the boot test can actually run: QEMU is
            // installed and KVM virtualization is available on this host.
            MenuSeparator {
                visible: app.qemuAvailable && app.qemuKvm
            }
            MenuItem {
                text: qsTr("Verify boot device (QEMU)…")
                visible: app.qemuAvailable && app.qemuKvm
                enabled: !app.busy && app.selectedDevice >= 0
                onTriggered: bootTestDialog.open()
            }
        }
        Menu {
            title: qsTr("Settings")
            MenuItem {
                // Checkable so the active state is visible at-a-glance.
                // Useful on non-English desktops to force the canonical
                // English strings (e.g. for screenshots / bug reports).
                text: qsTr("Force English")
                checkable: true
                checked: app.forceEnglish
                onTriggered: app.applyForceEnglish(checked)
            }
            MenuItem {
                text: qsTr("Always show activity log")
                checkable: true
                checked: app.showLogsAlways
                onTriggered: app.applyShowLogsAlways(checked)
            }
        }
        Menu {
            title: qsTr("?")
            MenuItem {
                text: qsTr("Dependencies")
                onTriggered: {
                    depsDialog.refresh()
                    depsDialog.open()
                }
            }
            MenuItem {
                text: qsTr("About USBooty")
                onTriggered: aboutDialog.open()
            }
        }
    }

    // Two columns: controls on the left (always), activity log on the
    // right (only after the first log line, with the window auto-widening
    // to make space).
    RowLayout {
        anchors.fill: parent
        anchors.margins: 12
        spacing: 12

        ColumnLayout {
            id: mainCol
            // Sized to its content (the window's `height` binding tracks
            // this implicitHeight). Hugged to the top so the log column
            // can be taller without dragging the controls down.
            Layout.alignment: Qt.AlignTop
            // Width follows the separator drag; the log column fills the rest.
            // Never narrower than leftPanelMinWidth (enforced both here and in
            // the drag handler so the layout can't squeeze it below the floor).
            Layout.preferredWidth: window.leftPanelWidth
            Layout.minimumWidth: window.leftPanelMinWidth
            // Pin the width while the log is visible: extra window width must
            // go to the log column (Layout.fillWidth), never to the controls.
            // The separator drag is the only thing that widens this column.
            // With the log hidden there is no second column, so let it fill.
            Layout.maximumWidth: window.logVisible
                ? window.leftPanelWidth : Number.POSITIVE_INFINITY
            spacing: 8

        // ---- Advisory banners ------------------------------------------
        Banner {
            // Missing external tools.
            severity: "warn"
            message: app.depWarning
        }
        Banner {
            // The ISO cannot fit on the chosen drive.
            severity: "error"
            message: app.method === 2 ? "" : app.fitWarning
        }
        Banner {
            // SBAT / DBX revocation hits from scanning the ISO's EFI
            // binaries. Promoted to "error" so it stands out as a real
            // boot risk — modern Secure-Boot-enforcing firmware will
            // *refuse* to load a revoked bootloader. Legacy BIOS or
            // firmware with stale SbatLevel still boots, so it's a
            // warning the user can ignore deliberately.
            severity: "error"
            message: app.revocationWarnings
            ToolTip.delay: 500
            ToolTip.visible: hovered
            ToolTip.text: qsTr("USBooty scanned this ISO's signed EFI binaries against the "
                + "Secure Boot revocation database (SBAT generations + the live UEFI Forum "
                + "DBX update). One or more bootloaders are flagged as obsolete. UEFI firmware "
                + "with current revocations will refuse to load them. Try a newer ISO, or "
                + "boot in legacy / non-Secure-Boot mode.")
            // `hovered` is a Banner-level alias for the Label's MouseArea.
            property bool hovered: bannerHoverArea.containsMouse
            MouseArea {
                id: bannerHoverArea
                anchors.fill: parent
                hoverEnabled: true
                acceptedButtons: Qt.NoButton
            }
        }
        Banner {
            // SMART probe result for the selected device; populated
            // asynchronously after select_device.
            severity: "error"
            message: app.smartWarning
                ? "SMART: " + app.smartWarning + ". Consider replacing this drive."
                : ""
        }

        // ---- Step 1: source image --------------------------------------
        StepCard {
            step: 1
            // Header reflects optionality so the user isn't blocked looking
            // for an ISO when they only want a plain format or a Ventoy
            // stick (which seeds itself empty if no ISO is given).
            heading: (app.method === 2 || app.method === 4)
                        ? qsTr("Source image (not used)")
                    : app.method === 3 ? qsTr("Source image (optional)")
                    : qsTr("Source image")
            accent: "#3498db"
            // Format-only and FreeDOS need no source image.
            enabled: app.method !== 2 && app.method !== 4

            RowLayout {
                Layout.fillWidth: true
                TextField {
                    id: isoField
                    Layout.fillWidth: true
                    readOnly: true
                    placeholderText:
                        app.method === 2 ? qsTr("Not used for a plain format")
                      : app.method === 4 ? qsTr("Not used: FreeDOS files are downloaded from upstream")
                      : app.method === 3 ? qsTr("Optional: Ventoy lets you drop ISOs onto the data partition later")
                      : qsTr("Choose an ISO image, or drag one onto the window…")
                    text: app.isoPath
                    ToolTip.delay: 500
                    ToolTip.visible: hovered && enabled
                    ToolTip.text: app.isoPath !== ""
                        ? app.isoPath
                        : qsTr("Drop an .iso / .img / .vhd / compressed image (.xz / .gz / .bz2 / "
                             + ".zst / .lzma / .zip / .Z) anywhere on the window, or use Browse… "
                             + "to pick one. Compressed and VHD images are unpacked into "
                             + "~/.cache/usbooty/ before writing.")
                }
                // Split button: "Browse…" plus a dropdown to download Windows.
                SplitButton {
                    id: sourceBtn
                    text: qsTr("Browse…")
                    enabled: !app.busy
                    onClicked: isoDialog.open()
                    onMenuRequested: sourceMenu.popup(sourceBtn, 0, sourceBtn.height)
                    Menu {
                        id: sourceMenu
                        MenuItem {
                            text: qsTr("Download a Windows ISO…")
                            onTriggered: winDialog.open()
                        }
                        MenuSeparator { }
                        MenuItem {
                            text: qsTr("Clear source image")
                            enabled: app.isoPath !== ""
                            onTriggered: app.clearIso()
                        }
                    }
                }
            }
            // ISO summary line with an OS-detection chip alongside it. The
            // chip surfaces the result of usbooty's ISO classification so the
            // user can confirm at a glance that their Windows / Linux ISO was
            // recognised (and thus the relevant options will be available).
            RowLayout {
                Layout.fillWidth: true
                spacing: 8
                Row {
                    spacing: 6
                    visible: app.windowsIso || app.linuxIso
                    WindowsLogo {
                        size: 14
                        visible: app.windowsIso
                        anchors.verticalCenter: parent.verticalCenter
                    }
                    Pill {
                        visible: app.windowsIso
                        label: qsTr("Windows")
                        tint: "#0078D4"
                    }
                    LinuxLogo {
                        size: 14
                        visible: app.linuxIso
                        anchors.verticalCenter: parent.verticalCenter
                    }
                    Pill {
                        visible: app.linuxIso
                        label: qsTr("Linux")
                        tint: "#E67E22"
                    }
                }
                Label {
                    text: window.trMsg(app.isoSummary)
                    color: palette.placeholderText
                    elide: Text.ElideMiddle
                    Layout.fillWidth: true
                }
            }
            ColumnLayout {
                // Hash computation is opt-in: streaming five hashers over a
                // multi-GiB ISO is CPU-heavy and disk-bound. Show a button
                // that triggers it on demand; once the hashes are filled in,
                // hide the button and expose the (selectable) values.
                visible: app.isoPath !== ""
                spacing: 4
                Layout.fillWidth: true

                readonly property bool anyHash: app.isoSha256 !== "" || app.isoMd5 !== ""
                    || app.isoSha1 !== "" || app.isoSha512 !== "" || app.isoBlake3 !== ""

                RowLayout {
                    Layout.fillWidth: true
                    visible: !app.hashing && !parent.anyHash
                    Button {
                        text: qsTr("Compute checksums")
                        enabled: !app.busy
                        onClicked: app.computeHashes()
                        ToolTip.delay: 500
                        ToolTip.visible: hovered
                        ToolTip.text: qsTr("Stream the ISO through MD5, SHA-1, SHA-256, SHA-512 and "
                            + "BLAKE3 in one pass. Disk-bound and CPU-heavy on a multi-GiB ISO; "
                            + "skip it unless you want to cross-check against a published hash.")
                    }
                    Label {
                        text: qsTr("Checksums skipped. Click to compute every digest.")
                        color: palette.placeholderText
                        font.pointSize: 9
                        elide: Text.ElideRight
                        Layout.fillWidth: true
                    }
                }

                // Computing: a single shared progress bar with the percentage.
                // The five hashes stream through one read pass and finish
                // together, so one bar is clearer than five spinners.
                RowLayout {
                    Layout.fillWidth: true
                    visible: app.hashing
                    Label {
                        text: qsTr("Computing checksums…")
                        color: palette.placeholderText
                        font.pointSize: 9
                    }
                    ProgressBar {
                        Layout.fillWidth: true
                        from: 0
                        to: 1
                        value: app.hashProgress
                    }
                    Label {
                        text: Math.round(app.hashProgress * 100) + " %"
                        color: palette.placeholderText
                        font.pointSize: 9
                    }
                }

                ColumnLayout {
                    visible: !app.hashing && parent.anyHash
                    spacing: 1
                    Layout.fillWidth: true

                    TextEdit {
                        text: "SHA-256:  " + app.isoSha256
                        readOnly: true
                        selectByMouse: true
                        wrapMode: TextEdit.WrapAnywhere
                        color: palette.placeholderText
                        font.family: "monospace"
                        font.pointSize: 8
                        Layout.fillWidth: true
                    }
                    TextEdit {
                        text: "SHA-1:    " + app.isoSha1
                        readOnly: true
                        selectByMouse: true
                        wrapMode: TextEdit.WrapAnywhere
                        color: palette.placeholderText
                        font.family: "monospace"
                        font.pointSize: 8
                        Layout.fillWidth: true
                    }
                    TextEdit {
                        text: "MD5:      " + app.isoMd5
                        readOnly: true
                        selectByMouse: true
                        wrapMode: TextEdit.WrapAnywhere
                        color: palette.placeholderText
                        font.family: "monospace"
                        font.pointSize: 8
                        Layout.fillWidth: true
                    }
                    TextEdit {
                        text: "SHA-512:  " + app.isoSha512
                        readOnly: true
                        selectByMouse: true
                        wrapMode: TextEdit.WrapAnywhere
                        color: palette.placeholderText
                        font.family: "monospace"
                        font.pointSize: 8
                        Layout.fillWidth: true
                    }
                    TextEdit {
                        text: "BLAKE3:   " + app.isoBlake3
                        readOnly: true
                        selectByMouse: true
                        wrapMode: TextEdit.WrapAnywhere
                        color: palette.placeholderText
                        font.family: "monospace"
                        font.pointSize: 8
                        Layout.fillWidth: true
                    }
                    // Green "verified" badge, shown only when the upstream
                    // SHA-1 database recognised this ISO.
                    Rectangle {
                        visible: app.isoAdguardBadge !== ""
                        Layout.fillWidth: true
                        Layout.topMargin: 4
                        color: "#1E7E34"
                        radius: 4
                        implicitHeight: badgeRow.implicitHeight + 8
                        RowLayout {
                            id: badgeRow
                            anchors.fill: parent
                            anchors.margins: 4
                            spacing: 6
                            Label {
                                text: "✓"
                                color: "white"
                                font.bold: true
                            }
                            Label {
                                Layout.fillWidth: true
                                text: qsTr("Verified: %1").arg(app.isoAdguardBadge)
                                color: "white"
                                wrapMode: Text.Wrap
                                font.pointSize: 9
                            }
                        }
                    }
                }
            }
        }

        // ---- Step 2: target device -------------------------------------
        StepCard {
            step: 2
            heading: qsTr("Target device")
            accent: "#E67E22"

            RowLayout {
                Layout.fillWidth: true
                FormCombo {
                    id: deviceBox
                    Layout.fillWidth: true
                    enabled: !app.busy && count > 0
                    model: app.devices.length > 0 ? app.devices.split("\n") : []
                    currentIndex: app.selectedDevice
                    onActivated: function(index) { app.selectDevice(index) }
                    displayText: count > 0 ? currentText
                                           : qsTr("No removable devices found")

                    // Two-line rows: hardware name above, capacity / bus /
                    // node below, with internal disks flagged in red.
                    delegate: ItemDelegate {
                        id: deviceDelegate
                        width: deviceBox.width
                        highlighted: deviceBox.highlightedIndex === index
                        // Split once per delegate, not per Label binding.
                        readonly property var deviceParts: modelData.split(" · ")
                        contentItem: ColumnLayout {
                            spacing: 1
                            Label {
                                text: deviceDelegate.deviceParts[0]
                                font.bold: true
                                elide: Text.ElideRight
                                Layout.fillWidth: true
                            }
                            Label {
                                text: deviceDelegate.deviceParts.length > 1
                                    ? deviceDelegate.deviceParts.slice(1).join(" · ")
                                    : ""
                                font.pointSize: 9
                                elide: Text.ElideRight
                                Layout.fillWidth: true
                                // Red token regardless of theme — internal-disk
                                // warnings should never blend into the row.
                                // Brightened a bit for dark themes via Qt.tint.
                                color: text.indexOf("Internal disk") >= 0
                                       ? Qt.tint(window.palette.windowText,
                                                 Qt.rgba(0.86, 0.21, 0.27, 0.85))
                                       : window.palette.placeholderText
                            }
                        }
                    }
                }
                Button {
                    text: qsTr("Refresh")
                    icon.name: "view-refresh"
                    display: icon.name
                        ? AbstractButton.IconOnly
                        : AbstractButton.TextOnly
                    enabled: !app.busy
                    onClicked: app.refreshDevices()
                    ToolTip.delay: 500
                    ToolTip.visible: hovered
                    ToolTip.text: qsTr("Re-scan /sys/block for connected drives. "
                        + "Usbooty already polls every few seconds while idle; "
                        + "use this if you just hotplugged a device and want it instantly.")
                }
            }
            WrapCheckBox {
                text: qsTr("Show non-removable (internal) disks")
                enabled: !app.busy
                checked: app.showFixedDisks
                onToggled: {
                    app.showFixedDisks = checked
                    app.refreshDevices()
                }
                ToolTip.delay: 500
                ToolTip.visible: hovered
                ToolTip.text: qsTr("Off by default. Internal SATA/NVMe disks are filtered out "
                    + "so they cannot be picked by mistake. Enable only when you really want "
                    + "to target a fixed disk (lab, dual-boot stick, image dump).")
            }
        }

        // ---- Step 3: options -------------------------------------------
        StepCard {
            step: 3
            heading: qsTr("Options")
            accent: "#16A085"

            GridLayout {
                Layout.fillWidth: true
                columns: 2
                columnSpacing: 12
                rowSpacing: 8

                Label { text: qsTr("Write method") }
                FormCombo {
                    Layout.fillWidth: true
                    enabled: !app.busy
                    model: [qsTr("DD image (raw copy)"),
                            qsTr("Partition & copy files"),
                            qsTr("Format only (no ISO)"),
                            qsTr("Ventoy (multi-boot USB)"),
                            qsTr("FreeDOS bootable USB")]
                    currentIndex: app.method
                    onActivated: function(index) { app.method = index }
                    ToolTip.delay: 500
                    ToolTip.visible: hovered
                    ToolTip.text: qsTr(
                        "DD: bit-for-bit copy of the ISO, no partitioning. Works for any "
                        + "isohybrid (most Linux ISOs).\n\n"
                        + "Partition & copy: USBooty creates a fresh partition table, formats it, "
                        + "and copies the ISO files. Required for Windows install media and for "
                        + "anything that needs persistence.\n\n"
                        + "Format only: wipe + new partition table, no ISO involved.\n\n"
                        + "Ventoy: install Ventoy so you can drop multiple ISOs on the data partition "
                        + "and pick one at boot.\n\n"
                        + "FreeDOS: download the latest FreeDOS kernel + shell from upstream and "
                        + "build a self-contained bootable DOS stick (no ISO needed). Useful for "
                        + "BIOS flashing utilities and legacy DOS tools.")
                }

                Label { text: qsTr("Filesystem") }
                FormCombo {
                    Layout.fillWidth: true
                    // The filesystem is chosen automatically when writing an
                    // image; it is only user-selectable for a plain format
                    // or for the FreeDOS method (which needs FAT16/FAT32).
                    enabled: !app.busy && (app.method === 2 || app.method === 4)
                    // Bound to the list of filesystems whose mkfs tools are
                    // installed on this host — keeps the user from picking a
                    // variant that would fail at format time. Filesystem
                    // names stay in their canonical form across every
                    // language, so no qsTr wrapping is needed.
                    model: app.availableFilesystems.length > 0
                        ? app.availableFilesystems.split("\n")
                        : ["FAT32"]
                    currentIndex: app.filesystem
                    onActivated: function(index) { app.filesystem = index }
                    ToolTip.visible: hovered && !enabled
                    ToolTip.text: qsTr("When writing an image, the filesystem is chosen automatically.")
                }

                Label { text: qsTr("Partition scheme") }
                FormCombo {
                    Layout.fillWidth: true
                    // Meaningful for the partition and format methods; a raw
                    // DD copy keeps the ISO's own embedded table.
                    enabled: !app.busy && app.method !== 0
                    // Order is mirrored by `filesystem_kind_from_index` on
                    // the Rust side — keep them aligned when adding entries.
                    model: [
                        qsTr("GPT (UEFI)"),
                        qsTr("MBR (BIOS)"),
                        qsTr("MBR (BIOS+UEFI)"),
                        qsTr("Hybrid MBR+GPT (BIOS+UEFI)")
                    ]
                    currentIndex: app.table
                    onActivated: function(index) { app.table = index }
                    ToolTip.delay: 500
                    ToolTip.visible: hovered
                    ToolTip.text: !enabled
                        ? qsTr("The DD method preserves the ISO's own partition table.")
                        : qsTr("How the disk is laid out for the firmware that boots it.\n\n"
                             + "• GPT (UEFI): modern default. Boots only on UEFI firmware. Required "
                             + "for disks larger than 2 TiB and for more than 4 partitions.\n\n"
                             + "• MBR (BIOS): legacy 1980s table. Boots only on BIOS / CSM. Pick this "
                             + "when the target PC's firmware truly is BIOS-only.\n\n"
                             + "• MBR (BIOS+UEFI): same on-disk layout as MBR, plus a bootable FAT "
                             + "partition with /EFI/BOOT/BOOTx64.EFI so UEFI firmware finds it via the "
                             + "fallback path. Simplest dual-firmware stick.\n\n"
                             + "• Hybrid MBR+GPT (BIOS+UEFI): real GPT + a synthesised MBR mirror "
                             + "of the data partition (Apple-style). Maximum compatibility, but some "
                             + "buggy firmwares dislike hybrid MBRs entirely. Use only if MBR(BIOS+UEFI) "
                             + "doesn't boot on a specific machine.")
                }

                // Ventoy names its own data partition — no label field for it.
                Label {
                    text: qsTr("Volume label")
                    visible: app.method !== 3
                }
                TextField {
                    id: labelField
                    Layout.fillWidth: true
                    visible: app.method !== 3
                    enabled: !app.busy && app.method !== 0
                    placeholderText: qsTr("Drive label")
                    text: app.label
                    // onTextEdited (not onTextChanged) avoids a binding loop:
                    // it fires only for user edits, not the pre-fill above.
                    onTextEdited: app.label = text
                    ToolTip.delay: 400
                    ToolTip.visible: hovered && app.label !== ""
                    // Show the user *exactly* what will land on disk after
                    // each filesystem's length / case / charset trimming.
                    ToolTip.text: {
                        if (!enabled)
                            return qsTr("The label is sanitized to each filesystem's limits.")
                        var sanitized = app.sanitizedLabel()
                        if (sanitized === app.label)
                            return qsTr("Will be written as “%1” (fits the chosen filesystem).")
                                       .arg(sanitized)
                        return qsTr("Will be written as “%1” (trimmed for the chosen filesystem)")
                                       .arg(sanitized)
                    }
                }
            }

            WrapCheckBox {
                text: qsTr("Full format: erase the whole device first (slow)")
                // DD overwrites every sector anyway, and Ventoy does its own
                // partitioning and formatting.
                enabled: !app.busy && (app.method === 1 || app.method === 2)
                checked: app.fullFormat
                onToggled: app.fullFormat = checked
                ToolTip.delay: 500
                ToolTip.visible: hovered
                ToolTip.text: qsTr("Zeroes every sector before writing. Slow (tens of minutes "
                    + "on a 64 GB stick), but it wipes any prior partition layout / hidden "
                    + "partitions and gives a clean slate. The quick path skips this and "
                    + "only writes the new layout.")
            }

            WrapCheckBox {
                // Windows ISO with oversized install.wim, partition method.
                // The default is the UEFI:NTFS two-partition layout; ticking
                // this asks USBooty to split install.wim into <4 GiB chunks
                // via wimlib-imagex and keep a single FAT32 partition.
                visible: app.windowsIso && app.method === 1
                text: qsTr("Split install.wim onto FAT32 (needs wimlib-imagex): broader firmware support than UEFI:NTFS")
                enabled: !app.busy
                checked: app.splitWim
                onToggled: app.splitWim = checked
                ToolTip.delay: 500
                ToolTip.visible: hovered
                ToolTip.text: qsTr("Windows ISOs with install.wim larger than 4 GiB cannot live on a single "
                    + "FAT32 partition as-is. The default layout is UEFI:NTFS (a small FAT32 + a big NTFS "
                    + "partition with a signed UEFI loader). This alternative uses wimlib-imagex to split "
                    + "install.wim into install.swm chunks Windows Setup loads natively, leaving you with "
                    + "a single FAT32 partition that boots on more firmware.")
            }

            WrapCheckBox {
                text: qsTr("Verify after writing: read the data back and check it")
                // A plain format / Ventoy install writes no verifiable payload.
                enabled: !app.busy && app.method < 2
                checked: app.verify
                onToggled: app.verify = checked
                ToolTip.delay: 500
                ToolTip.visible: hovered
                ToolTip.text: qsTr("Re-reads the entire device after writing and compares it "
                    + "to a BLAKE3 hash captured during the write. Roughly doubles the job "
                    + "time but catches counterfeit / failing flash that silently corrupts data.")
            }

            // Ventoy options — only for the Ventoy write method.
            ColumnLayout {
                Layout.fillWidth: true
                visible: app.method === 3
                spacing: 4
                WrapCheckBox {
                    text: qsTr("Update an existing Ventoy install (keeps your ISOs)")
                    enabled: !app.busy
                    checked: app.ventoyUpdate
                    onToggled: app.ventoyUpdate = checked
                    ToolTip.delay: 500
                    ToolTip.visible: hovered
                    ToolTip.text: qsTr("Upgrade the Ventoy bootloader in-place. The existing data "
                        + "partition (with your ISOs) is preserved; only the small EFI partition "
                        + "and the Ventoy boot files get rewritten.")
                }
                WrapCheckBox {
                    text: qsTr("Secure Boot support")
                    enabled: !app.busy
                    checked: app.ventoySecureBoot
                    onToggled: app.ventoySecureBoot = checked
                    ToolTip.delay: 500
                    ToolTip.visible: hovered
                    ToolTip.text: qsTr("Install Ventoy with the Microsoft-signed shim so the stick "
                        + "boots on UEFI machines that have Secure Boot enabled. Off → smaller "
                        + "footprint and no MOK enrollment, but Secure Boot must be disabled.")
                }
                Label {
                    text: qsTr("Ventoy makes a USB you drop ISOs onto and boot directly. "
                             + "A loaded ISO above (optional) is copied onto it.")
                    color: palette.placeholderText
                    font.pointSize: 8
                    wrapMode: Text.Wrap
                    Layout.fillWidth: true
                }
            }

            // Persistence — only for Linux live ISOs that support it.
            // Partition-based variants show a size slider; inline-folder
            // variants (Slax) show a simple on/off checkbox because the
            // changes directory lives inside the main data partition.
            // The slider section also disappears when no device is plugged
            // in / selected — there is nothing meaningful to slide against
            // until the user picks the target drive.
            ColumnLayout {
                Layout.fillWidth: true
                visible: app.persistenceSupported
                         && app.method === 1
                         && (app.persistenceInline || (app.selectedDevice >= 0 && app.persistenceMaxMib > 0))
                spacing: 2
                Label {
                    visible: !app.persistenceInline
                    // Below 1 GiB: show MiB. At or above 1 GiB: GiB with one
                    // decimal place; snapping makes that decimal meaningful.
                    text: {
                        if (app.persistenceSize <= 0)
                            return qsTr("Persistent storage:  off")
                        if (app.persistenceSize < 1024)
                            return qsTr("Persistent storage:  %1 MiB").arg(app.persistenceSize)
                        return qsTr("Persistent storage:  %1 GiB")
                                   .arg((app.persistenceSize / 1024).toFixed(1))
                    }
                    font.bold: true
                }
                RowLayout {
                    Layout.fillWidth: true
                    spacing: 6
                    visible: !app.persistenceInline
                    Slider {
                        id: persistenceSlider
                        Layout.fillWidth: true
                        enabled: !app.busy && app.persistenceMaxMib > 0
                        from: 0
                        // Always exactly the room the selected device has
                        // left — recomputed by AppController whenever the
                        // device or ISO changes.
                        to: app.persistenceMaxMib
                        // 256 MiB steps below 1 GiB (fine-grained for small
                        // overlays), 512 MiB steps above (matches the displayed
                        // 0.5 GiB precision so the label never lies).
                        stepSize: value < 1024 ? 256 : 512
                        value: app.persistenceSize
                        onMoved: app.persistenceSize = value
                        ToolTip.visible: pressed
                        ToolTip.text: value <= 0 ? qsTr("Off")
                                    : value < 1024 ? qsTr("%1 MiB").arg(value)
                                    : qsTr("%1 GiB").arg((value / 1024).toFixed(1))
                    }
                    Button {
                        text: qsTr("Max")
                        enabled: !app.busy && app.persistenceMaxMib > 0
                        onClicked: app.persistenceSize = app.persistenceMaxMib
                        ToolTip.delay: 500
                        ToolTip.visible: hovered
                        ToolTip.text: qsTr("Set the overlay to fill the device: uses every byte "
                            + "the chosen drive has left after the ISO and a small partition-table margin.")
                    }
                }
                // Inline-folder persistence (currently Slax): a single
                // toggle. Non-zero `persistenceSize` is how the job builder
                // detects the request; the value itself is ignored.
                CheckBox {
                    visible: app.persistenceInline
                    enabled: !app.busy
                    text: qsTr("Enable persistent changes")
                    checked: app.persistenceSize > 0
                    onToggled: app.persistenceSize = checked ? 1 : 0
                    ToolTip.delay: 500
                    ToolTip.visible: hovered
                    ToolTip.text: qsTr("Persistence lives on the writable boot stick, with no "
                        + "separate partition. Slax saves to /slax/changes/ automatically; Alpine "
                        + "runs from RAM and persists with lbu, so run `lbu commit` inside Alpine to "
                        + "save the apkovl (an apk cache folder is prepared for you).")
                }
                Label {
                    text: app.persistenceInline
                        ? qsTr("Changes are saved to the writable boot stick; no separate "
                             + "partition is created.")
                        : qsTr("Keeps your files and settings across reboots of this live USB.")
                    color: palette.placeholderText
                    font.pointSize: 8
                    wrapMode: Text.Wrap
                    Layout.fillWidth: true
                }
                // The distro family usbooty matched, so the user can tell at
                // a glance why a particular scheme was selected.
                Label {
                    visible: app.distroLabel.length > 0
                    text: qsTr("Detected distribution: %1").arg(app.distroLabel)
                    color: palette.placeholderText
                    font.pointSize: 8
                    Layout.fillWidth: true
                }
            }

            // Linux ISO whose distribution has no partition-persistence support.
            Label {
                Layout.fillWidth: true
                visible: app.linuxIso && !app.persistenceSupported && app.method === 1
                text: app.distroLabel.length > 0
                    ? qsTr("Persistent storage isn't supported for %1.").arg(app.distroLabel)
                    : qsTr("Persistent storage isn't supported for this distribution.")
                color: palette.placeholderText
                font.pointSize: 8
                wrapMode: Text.Wrap
            }

            // Persistence is available but the user hasn't picked a device
            // yet — explain why the slider isn't there so it doesn't look
            // like the feature is missing.
            Label {
                Layout.fillWidth: true
                visible: app.persistenceSupported
                         && !app.persistenceInline
                         && app.method === 1
                         && (app.selectedDevice < 0 || app.persistenceMaxMib <= 0)
                text: app.selectedDevice < 0
                    ? qsTr("Plug in or select a target device to set the persistence size.")
                    : qsTr("The selected device has no room left for a persistence partition once the ISO is written.")
                color: palette.placeholderText
                font.pointSize: 8
                wrapMode: Text.Wrap
            }
        }

        // ---- Action -----------------------------------------------------
        Button {
            Layout.fillWidth: true
            Layout.preferredHeight: 44
            // While the user has clicked Cancel but the runner has not yet
            // acknowledged, show "Cancelling…" disabled, so a rage-click on
            // a non-responsive cancel doesn't leave the user guessing.
            text: window.cancelling ? qsTr("Cancelling…")
                                    : (app.busy ? qsTr("Cancel") : qsTr("Start"))
            highlighted: true
            font.bold: true
            enabled: !window.cancelling && (app.busy || window.ready)
            ToolTip.delay: 500
            ToolTip.visible: hovered
            ToolTip.text: app.busy
                ? qsTr("Ask the running helper to stop. The current sector finishes writing, then "
                     + "the partition table is left in whatever state the helper had got to. "
                     + "Expect a partially-written drive.")
                : (app.windowsIso && app.method === 1
                    ? qsTr("Opens the Windows-setup dialog first (TPM/Secure-Boot/RAM bypasses, "
                         + "local account, debloat, …); the actual write begins after you click OK there.")
                    : qsTr("Confirm and start writing. All data on the selected device is erased."))
            onClicked: {
                if (app.busy) {
                    window.cancelling = true
                    app.cancel()
                } else if (app.windowsIso && app.method === 1) {
                    // Windows installer, partition method: offer the setup
                    // options first. (DD is a raw copy — it cannot apply an
                    // autounattend.xml, so no setup dialog there.)
                    windowsSetupDialog.open()
                } else {
                    confirmDialog.open()
                }
            }
        }

        // ---- Progress ---------------------------------------------------
        Frame {
            id: progressFrame
            Layout.fillWidth: true
            visible: app.busy || app.progress > 0
            padding: 12
            // Lower-cased phase name, computed once and shared by the phase
            // colour below and the ProgressBar's indeterminate test.
            readonly property string phaseLower: app.phase.toLowerCase()
            // A colour for the active phase, so the user can tell at a glance
            // whether we are still writing or already verifying the bytes back.
            readonly property color phaseColor: {
                var p = progressFrame.phaseLower
                if (p.indexOf("verif") >= 0)  return "#27AE60" // green
                if (p.indexOf("flush") >= 0)  return "#F39C12" // amber
                if (p.indexOf("format") >= 0) return "#8E44AD" // violet
                if (p.indexOf("ventoy") >= 0) return "#16A085" // teal
                if (p.indexOf("download") >= 0) return "#0078D4" // Windows blue
                if (p.indexOf("decompress") >= 0) return "#9B59B6" // light violet
                return "#3498DB"                                 // default blue
            }
            background: Rectangle {
                radius: 8
                color: palette.base
                border.color: palette.mid
                // A thin coloured stripe along the top edge mirroring the
                // active phase. Reads as "this is currently a Writing pass"
                // even before the user looks at the chip below.
                Rectangle {
                    anchors.left: parent.left
                    anchors.right: parent.right
                    anchors.top: parent.top
                    height: 3
                    radius: parent.radius
                    color: progressFrame.phaseColor
                    opacity: app.busy ? 0.95 : 0.45
                }
            }
            contentItem: ColumnLayout {
                spacing: 8
                ProgressBar {
                    id: phaseBar
                    Layout.fillWidth: true
                    from: 0
                    to: 1
                    value: app.progress
                    // Some phases never emit progress events (wimlib split,
                    // mkfs, syslinux install, ext4 persistence creation):
                    // switch to the indeterminate animation while we're in
                    // them so the bar doesn't sit frozen for minutes.
                    indeterminate: app.busy
                        && (progressFrame.phaseLower.indexOf("splitting") >= 0
                            || progressFrame.phaseLower.indexOf("syslinux") >= 0
                            || progressFrame.phaseLower.indexOf("extlinux") >= 0
                            || progressFrame.phaseLower.indexOf("persistence") >= 0
                            || progressFrame.phaseLower.indexOf("formatting") >= 0)
                    // Re-tint the fill bar to match the phase. The Qt default
                    // contentItem is a Rectangle, so we override it cleanly.
                    contentItem: Item {
                        implicitHeight: 6
                        Rectangle {
                            visible: !phaseBar.indeterminate
                            width: phaseBar.visualPosition * parent.width
                            height: parent.height
                            radius: 3
                            color: progressFrame.phaseColor
                        }
                        // Indeterminate sweep: a 35 %-wide block ping-pongs
                        // across the track. Activated only while
                        // `phaseBar.indeterminate` is true.
                        Rectangle {
                            id: sweep
                            visible: phaseBar.indeterminate
                            width: parent.width * 0.35
                            height: parent.height
                            radius: 3
                            color: progressFrame.phaseColor
                            x: 0
                            SequentialAnimation on x {
                                running: phaseBar.indeterminate
                                loops: Animation.Infinite
                                NumberAnimation {
                                    from: 0
                                    to: sweep.parent.width - sweep.width
                                    duration: 1200
                                    easing.type: Easing.InOutQuad
                                }
                                NumberAnimation {
                                    from: sweep.parent.width - sweep.width
                                    to: 0
                                    duration: 1200
                                    easing.type: Easing.InOutQuad
                                }
                            }
                        }
                    }
                    background: Rectangle {
                        implicitHeight: 6
                        // palette.mid is a midtone that contrasts against
                        // both light (slightly dark) and dark (slightly
                        // light) Qt themes.
                        color: palette.mid
                        opacity: 0.4
                        radius: 3
                    }
                }
                RowLayout {
                    Layout.fillWidth: true
                    spacing: 8
                    Pill {
                        visible: app.phase !== ""
                        label: window.trPhase(app.phase)
                        tint: progressFrame.phaseColor
                    }
                    Label {
                        text: app.phase === "" ? qsTr("Working") : ""
                        font.bold: true
                        visible: text !== ""
                    }
                    Item { Layout.fillWidth: true }
                    Label {
                        text: app.progress > 0
                              ? Math.round(app.progress * 100) + " %" : ""
                        font.bold: true
                        color: progressFrame.phaseColor
                    }
                }
                // Live transfer statistics: speed · ETA · elapsed.
                Label {
                    Layout.fillWidth: true
                    visible: text !== ""
                    color: palette.placeholderText
                    text: {
                        var parts = []
                        if (app.speed !== "")
                            parts.push("⬆ " + app.speed)
                        if (app.eta !== "")
                            parts.push(qsTr("ETA %1").arg(app.eta))
                        if (app.busy) {
                            var e = window.fmtTime(window.elapsedSecs)
                            if (e !== "")
                                parts.push(qsTr("%1 elapsed").arg(e))
                        }
                        return parts.join("     ·     ")
                    }
                }
                Label {
                    text: window.trMsg(app.status)
                    color: palette.placeholderText
                    elide: Text.ElideRight
                    Layout.fillWidth: true
                }
            }
        }

        }

        // ---- Resizable separator ---------------------------------------
        // Drag to redistribute width between the controls column and the
        // log column. Only present while the log column itself is shown.
        Rectangle {
            id: logSplitter
            visible: window.logVisible
            Layout.fillHeight: true
            Layout.preferredWidth: 6
            // The grip line; brightens on hover/drag for affordance.
            color: (splitterMouse.containsMouse || splitterMouse.pressed)
                ? palette.highlight : palette.mid
            radius: 3
            // Keep the lower bound for the log column in one place: its own
            // minimum (340) plus the two RowLayout gaps and the splitter.
            readonly property int logMinWidth: 340
            MouseArea {
                id: splitterMouse
                anchors.fill: parent
                // Widen the hit area beyond the visible 6 px line so it is
                // easy to grab without enlarging the drawn separator.
                anchors.leftMargin: -4
                anchors.rightMargin: -4
                hoverEnabled: true
                cursorShape: Qt.SplitHCursor
                property real pressX: 0
                property int startWidth: 0
                onPressed: function(mouse) {
                    pressX = mapToItem(window.contentItem, mouse.x, mouse.y).x
                    startWidth = window.leftPanelWidth
                }
                onPositionChanged: function(mouse) {
                    if (!pressed)
                        return
                    var curX = mapToItem(window.contentItem, mouse.x, mouse.y).x
                    var proposed = startWidth + (curX - pressX)
                    // Upper bound: leave the log column at least logMinWidth.
                    // 54 = 24 outer margins + 2×12 RowLayout gaps + 6 splitter.
                    var maxWidth = window.width - 54 - logSplitter.logMinWidth
                    window.leftPanelWidth = Math.max(
                        window.leftPanelMinWidth,
                        Math.min(maxWidth, proposed))
                }
            }
        }

        // ---- Activity log (right column, lazy) -------------------------
        Frame {
            id: logFrame
            visible: window.logVisible
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.minimumWidth: 340
            padding: 10
            background: Rectangle {
                radius: 8
                color: palette.base
                border.color: palette.mid
            }
            // The TextArea renders a potentially large RichText document.
            // Don't build it until the panel is actually shown; when the user
            // collapses the log we unload it again (it repopulates from
            // app.logHtmlSnapshot() on the next load).
            contentItem: Loader {
                active: window.logVisible
                sourceComponent: ColumnLayout {
                spacing: 6
                RowLayout {
                    Layout.fillWidth: true
                    Label {
                        text: qsTr("Activity log")
                        font.bold: true
                        Layout.fillWidth: true
                    }
                    Button {
                        // Freedesktop theme name — Breeze / Adwaita /
                        // Papirus all ship `document-save`. When the icon
                        // is available we hide the text label so the
                        // button stays compact; on icon-less themes the
                        // text appears as a fallback (Qt drops to
                        // TextOnly automatically when icon.source/name
                        // resolves to nothing).
                        text: qsTr("Save…")
                        icon.name: "document-save"
                        display: icon.name
                            ? AbstractButton.IconOnly
                            : AbstractButton.TextOnly
                        flat: true
                        enabled: app.logNonEmpty
                        onClicked: saveLogDialog.open()
                        ToolTip.delay: 500
                        ToolTip.visible: hovered
                        ToolTip.text: qsTr("Write the current activity log to a text file. Useful "
                            + "for bug reports. Attach the file instead of pasting in the panel.")
                    }
                    Button {
                        text: qsTr("Clear")
                        icon.name: "edit-clear"
                        display: icon.name
                            ? AbstractButton.IconOnly
                            : AbstractButton.TextOnly
                        flat: true
                        enabled: app.logNonEmpty && !app.busy
                        ToolTip.delay: 500
                        ToolTip.visible: hovered
                        ToolTip.text: qsTr("Empty the activity log panel.")
                        onClicked: {
                            app.clearLog()
                            // Collapsing back also shrinks the window —
                            // the user explicitly asked for the screen
                            // estate back, so honour that. Unless they
                            // turned on "always show logs", in which
                            // case the panel stays put.
                            if (!app.showLogsAlways) {
                                window.logExpanded = false
                                window.shrinkAfterLog()
                            }
                        }
                    }
                }
                ScrollView {
                    id: logScroll
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    clip: true
                    ScrollBar.vertical.policy: ScrollBar.AlwaysOn
                    ScrollBar.horizontal.policy: ScrollBar.AlwaysOff

                    TextArea {
                        id: logArea
                        // Binding the width to the viewport makes lines wrap;
                        // without it the text runs off the side and the view
                        // never gains the height it needs to scroll.
                        width: logScroll.availableWidth
                        readOnly: true
                        wrapMode: TextArea.Wrap
                        font.family: "monospace"
                        font.pointSize: 9
                        placeholderText: qsTr("Job output will appear here.")
                        // RichText lets the runner colour warnings (amber),
                        // errors (red), and phase headers (blue/bold) via
                        // inline HTML; plain info lines stay the default colour.
                        textFormat: TextEdit.RichText
                        // The log buffer lives Rust-side and is streamed line by
                        // line via `appendLogHtml` (so appends stay cheap and the
                        // whole document is never re-parsed). This panel is in a
                        // Loader that unloads when collapsed, so on (re)load we
                        // repopulate from the snapshot, then append live lines.
                        Component.onCompleted: text = app.logHtmlSnapshot()
                        Connections {
                            target: app
                            function onAppendLogHtml(html) {
                                logArea.append(html)
                            }
                            function onLogNonEmptyChanged() {
                                if (!app.logNonEmpty)
                                    logArea.clear()
                            }
                        }
                        // Smart autoscroll: only snap to the bottom when the
                        // user was already at (or near) the bottom before the
                        // append, so scrolling up to inspect an earlier line
                        // is not yanked back by every new log entry.
                        property bool followTail: true
                        onContentHeightChanged: {
                            if (followTail)
                                cursorPosition = length
                        }
                        Connections {
                            target: logScroll.ScrollBar.vertical
                            function onPositionChanged() {
                                var sb = logScroll.ScrollBar.vertical
                                // "At the bottom" means within 2 % of the end;
                                // small margin so a near-bottom scroll counts.
                                logArea.followTail =
                                    sb.position + sb.size >= 0.98
                            }
                        }
                    }
                }
                }
            }
        }
    }

    // ---- Drag-and-drop overlay -----------------------------------------
    // Predicate shared with the file dialog: accepts plain disk images and
    // any of the compressed variants the decompressor recognises.
    function looksLikeImage(url) {
        var u = url.toString().toLowerCase()
        return u.endsWith(".iso") || u.endsWith(".img") || u.endsWith(".vhd")
            || u.endsWith(".iso.xz") || u.endsWith(".img.xz") || u.endsWith(".xz")
            || u.endsWith(".iso.gz") || u.endsWith(".img.gz") || u.endsWith(".gz")
            || u.endsWith(".iso.bz2") || u.endsWith(".img.bz2") || u.endsWith(".bz2")
            || u.endsWith(".iso.zst") || u.endsWith(".img.zst") || u.endsWith(".zst")
            || u.endsWith(".iso.lzma") || u.endsWith(".img.lzma") || u.endsWith(".lzma")
            || u.endsWith(".iso.zip") || u.endsWith(".img.zip") || u.endsWith(".zip")
            || u.endsWith(".iso.z") || u.endsWith(".img.z") || u.endsWith(".z")
    }
    DropArea {
        id: isoDrop
        anchors.fill: parent
        // Name of the first file the user is dragging, used to make the
        // overlay feel responsive. Cleared when the drag leaves.
        property string hoverName: ""
        property bool hoverIsoLike: false
        onEntered: function(drag) {
            if (drag.hasUrls && drag.urls.length > 0) {
                var u = drag.urls[0].toString()
                hoverName = u.substring(u.lastIndexOf("/") + 1)
                var l = hoverName.toLowerCase()
                hoverIsoLike = l.endsWith(".iso") || l.endsWith(".img") || l.endsWith(".vhd")
            }
        }
        onExited: { hoverName = ""; hoverIsoLike = false }
        onDropped: function(drop) {
            hoverName = ""; hoverIsoLike = false
            if (app.busy || !drop.hasUrls)
                return
            if (looksLikeImage(drop.urls[0]))
                app.setIso(drop.urls[0])
        }
        Rectangle {
            anchors.fill: parent
            anchors.margins: 6
            visible: isoDrop.containsDrag && !app.busy
            // Tint the system base colour with the highlight hue so the
            // overlay reads well on both light and dark themes. A red tint
            // signals "I won't accept this file" for non-ISO drops.
            color: {
                var c = isoDrop.hoverIsoLike
                    ? palette.highlight
                    : Qt.rgba(0.86, 0.21, 0.27, 1.0)
                return Qt.tint(palette.base, Qt.rgba(c.r, c.g, c.b, 0.18))
            }
            border.color: isoDrop.hoverIsoLike ? palette.highlight
                                               : Qt.rgba(0.86, 0.21, 0.27, 1.0)
            border.width: 2
            radius: 10
            ColumnLayout {
                anchors.centerIn: parent
                spacing: 4
                Label {
                    Layout.alignment: Qt.AlignHCenter
                    text: isoDrop.hoverIsoLike ? qsTr("Drop to load")
                                               : qsTr("Unsupported file")
                    font.pointSize: 18
                    font.bold: true
                    color: isoDrop.hoverIsoLike ? palette.highlight
                                                : Qt.rgba(0.86, 0.21, 0.27, 1.0)
                }
                Label {
                    Layout.alignment: Qt.AlignHCenter
                    visible: isoDrop.hoverName !== ""
                    text: isoDrop.hoverName
                    font.pointSize: 11
                    color: palette.windowText
                }
                Label {
                    Layout.alignment: Qt.AlignHCenter
                    visible: !isoDrop.hoverIsoLike && isoDrop.hoverName !== ""
                    text: qsTr("Only .iso and .img files are accepted")
                    font.pointSize: 9
                    color: palette.placeholderText
                }
            }
        }
    }

    // ---- Dialogs --------------------------------------------------------
    FileDialog {
        id: isoDialog
        title: qsTr("Select an ISO image")
        nameFilters: [
            qsTr("Disk images (*.iso *.img *.vhd *.iso.xz *.iso.gz *.iso.bz2 *.iso.zst *.iso.lzma *.iso.zip *.iso.Z *.img.xz *.img.gz *.img.bz2 *.img.zst *.img.lzma *.img.zip *.img.Z)"),
            qsTr("Compressed (*.xz *.gz *.bz2 *.zst *.lzma *.zip *.Z)"),
            qsTr("All files (*)")
        ]
        onAccepted: app.setIso(selectedFile)
    }

    FileDialog {
        // Snapshot output. The extension picks the compressor; the helper
        // resolves it. `.img` / no extension → raw.
        id: backupDialog
        title: qsTr("Save device snapshot")
        fileMode: FileDialog.SaveFile
        defaultSuffix: "img"
        nameFilters: [
            qsTr("Raw images (*.img)"),
            qsTr("Compressed images (*.img.gz *.img.xz *.img.zst *.img.bz2)"),
            qsTr("All files (*)")
        ]
        onAccepted: app.startBackup(selectedFile)
    }

    FileDialog {
        // Save the activity-log buffer to a user-chosen text file.
        id: saveLogDialog
        title: qsTr("Save activity log")
        fileMode: FileDialog.SaveFile
        defaultSuffix: "log"
        nameFilters: [qsTr("Log files (*.log *.txt)"), qsTr("All files (*)")]
        onAccepted: app.saveLogTo(selectedFile)
    }

    // Lightweight confirmation for the destructive device checks (both
    // patterns write every sector). `mode` is 0=Quick, 1=Full.
    Dialog {
        id: checkConfirm
        anchors.centerIn: parent
        // Clamp to the window's current width so the dialog never overflows
        // when the user shrinks the window; the 40 px buffer leaves room
        // for the modal backdrop and any window-manager shadow.
        width: Math.min(480, window.width - 40)
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

    // Shown when Start is pressed on a Windows ISO — optional install tweaks.
    // All settings flow into a generated `autounattend.xml` on the USB root.
    Dialog {
        id: windowsSetupDialog
        anchors.centerIn: parent
        width: window.width - 60
        // Match the window height (minus ~80px of chrome for the dialog
        // header, footer, and outer margin). The inner ScrollView keeps
        // overflowing content scrollable, so on small displays the dialog
        // shrinks gracefully, and on tall displays it grows to show more
        // checkboxes at once instead of being capped at a fixed value.
        height: window.height - 80
        modal: true
        // Inset the contentItem from the dialog frame on every side; the
        // header and footer carry their own spacing.
        topPadding: 14
        bottomPadding: 14
        leftPadding: 18
        rightPadding: 18
        // Microsoft-blue header with the Windows flag immediately identifies
        // this as the Windows installer tweak panel.
        header: DialogHeader {
            tint: "#0078D4"
            iconComponent: WindowsLogo { size: 24; tint: "white" }
            title: qsTr("Windows setup")
            subtitle: qsTr("Optional install tweaks (written to autounattend.xml)")
        }
        standardButtons: Dialog.Ok | Dialog.Cancel
        onAccepted: confirmDialog.open()
        // The Windows-setup ScrollView contains ~25 checkboxes + text
        // fields and is only opened when the user starts a Windows ISO
        // job. Defer everything until visible — the biggest single win
        // in this file.
        contentItem: Loader {
            active: windowsSetupDialog.visible
            sourceComponent: ScrollView {
            id: setupScroll
            clip: true
            ScrollBar.vertical.policy: ScrollBar.AsNeeded
            ScrollBar.horizontal.policy: ScrollBar.AlwaysOff
            ColumnLayout {
                // Reserve room on the right so the vertical scrollbar never
                // sits on top of long checkbox labels or section dividers.
                width: setupScroll.availableWidth - 14
                spacing: 10
            Label {
                text: qsTr("Customize the installation below, or just press OK to skip. "
                         + "Every option is optional.")
                color: palette.placeholderText
                wrapMode: Text.Wrap
                Layout.fillWidth: true
            }

            // --- Setup-time tweaks --------------------------------------
            Label { text: qsTr("Setup"); font.bold: true; Layout.topMargin: 6 }
            Rectangle { Layout.fillWidth: true; height: 1; color: palette.mid; opacity: 0.5 }
            WrapCheckBox {
                text: qsTr("Bypass Windows 11 hardware checks: TPM, Secure Boot, RAM, Storage, CPU, Disk")
                checked: app.bypassTpm
                onToggled: {
                    app.bypassTpm = checked
                    app.bypassSecureboot = checked
                    app.bypassRam = checked
                    app.bypassStorage = checked
                    app.bypassCpu = checked
                    app.bypassDisk = checked
                }
                ToolTip.delay: 500
                ToolTip.visible: hovered
                ToolTip.text: qsTr("Lets Windows 11 install on hardware that fails its requirements check: "
                    + "no TPM 2.0, Secure Boot disabled, less than 8 GB RAM, system drive smaller than 64 GB, "
                    + "an older / non-allowlisted CPU, or unusual disk geometry. Sets the six LabConfig "
                    + "registry flags during Setup. Has no effect on Windows 10 (which doesn't check any of these).")
            }
            WrapCheckBox {
                text: qsTr("Auto-accept the Setup EULA")
                checked: app.acceptEula
                onToggled: app.acceptEula = checked
                ToolTip.delay: 500
                ToolTip.visible: hovered
                ToolTip.text: qsTr("Pre-clicks 'Accept' on the licence-agreement page that appears near "
                    + "the start of Windows Setup, so the install proceeds without waiting for the user to "
                    + "scroll and tick the box.")
            }
            WrapCheckBox {
                text: qsTr("Enable .NET Framework 3.5 from the install media")
                checked: app.enableDotnet35
                onToggled: app.enableDotnet35 = checked
                ToolTip.delay: 500
                ToolTip.visible: hovered
                ToolTip.text: qsTr("Installs the legacy .NET Framework 3.5 runtime alongside the modern "
                    + ".NET 4.x that Windows ships with by default. Many older desktop apps (games, accounting "
                    + "software, in-house tools from the 2000s) refuse to run without it. The files are pulled "
                    + "from the install media itself, so no internet is needed.")
            }
            RowLayout {
                Layout.fillWidth: true
                Label { text: qsTr("Product key"); Layout.minimumWidth: 110 }
                TextField {
                    Layout.fillWidth: true
                    // Qt sets implicitWidth from the placeholder; without
                    // a 0 minimum the field refuses to shrink below it
                    // and a long translated placeholder pushes the parent
                    // RowLayout past the dialog width.
                    Layout.minimumWidth: 0
                    placeholderText: qsTr("Optional, e.g. VK7JG-NPHTM-C97JM-9MPGT-3V66T (Win 11 Pro)")
                    text: app.productKey
                    onTextEdited: app.productKey = text
                }
            }
            WrapCheckBox {
                text: qsTr("Force the edition picker at boot (OEM PCs)")
                checked: app.forceEditionPicker
                onToggled: app.forceEditionPicker = checked
                ToolTip.delay: 500
                ToolTip.visible: hovered
                ToolTip.text: qsTr("On an OEM PC with a firmware MSDM/SLIC key (typical of "
                    + "laptops sold with Windows Home Familiale pre-installed), Setup "
                    + "normally reads that key on boot and silently installs the matching "
                    + "edition. This option drops a sources/ei.cfg on the USB that tells "
                    + "Setup to ignore the firmware key, so you can pick a different "
                    + "edition (Pro, Enterprise, …) from Setup's built-in edition picker. "
                    + "Activation is a separate step. Install in the chosen edition first, "
                    + "then activate from inside Windows (e.g. with Microsoft Activation "
                    + "Scripts). Leave Product key empty above to get straight to the picker.")
            }

            // --- OOBE (first-boot) skips --------------------------------
            Label { text: qsTr("Out-of-box experience"); font.bold: true; Layout.topMargin: 6 }
            Rectangle { Layout.fillWidth: true; height: 1; color: palette.mid; opacity: 0.5 }
            WrapCheckBox {
                text: qsTr("Skip Microsoft-account requirement (works on Win 10 and all Win 11)")
                checked: app.skipMsaccount
                onToggled: app.skipMsaccount = checked
                ToolTip.delay: 500
                ToolTip.visible: hovered
                ToolTip.text: qsTr("Lets you create a *local* Windows account during first-boot setup, "
                    + "instead of being forced to sign in with (or create) a Microsoft account. Works on "
                    + "every supported Windows version: Win 10, Win 11 pre-24H2, and Win 11 24H2+ all use "
                    + "different mechanisms, this option applies whichever one is needed.")
            }
            WrapCheckBox {
                text: qsTr("Disable network during OOBE: force local account on Win 11 24H2+")
                checked: app.disableNetworkDuringOobe
                onToggled: app.disableNetworkDuringOobe = checked
                ToolTip.delay: 500
                ToolTip.visible: hovered
                ToolTip.text: qsTr("Hard-disables every network adapter for the duration of first-boot "
                    + "setup, so Windows physically can't reach Microsoft's servers to force online "
                    + "sign-in. Network is re-enabled automatically after the first sign-in. The most "
                    + "reliable local-account workaround on recent Win 11 builds where the regular "
                    + "'skip Microsoft account' flags are silently ignored.")
            }
            WrapCheckBox {
                text: qsTr("Skip the \"connect to a network\" Wi-Fi screen")
                checked: app.hideWirelessSetup
                onToggled: app.hideWirelessSetup = checked
                ToolTip.delay: 500
                ToolTip.visible: hovered
                ToolTip.text: qsTr("Skips the 'Let's connect you to a network' page during first boot. "
                    + "Useful if the machine already has wired Ethernet (no Wi-Fi password to enter), or "
                    + "if you'd rather finish OOBE first and configure Wi-Fi inside Windows after.")
            }
            WrapCheckBox {
                text: qsTr("Hide the OEM-registration screen")
                checked: app.hideOemRegistration
                onToggled: app.hideOemRegistration = checked
                ToolTip.delay: 500
                ToolTip.visible: hovered
                ToolTip.text: qsTr("Skips the OEM-registration / product-activation pages that appear "
                    + "during first boot on factory-restore images (Dell, HP, Lenovo). Has no effect on "
                    + "clean Microsoft ISOs. There's no OEM page to hide.")
            }
            WrapCheckBox {
                text: qsTr("Pre-answer the network-type prompt as \"Work\" (private/trusted)")
                checked: app.networkLocationWork
                onToggled: app.networkLocationWork = checked
                ToolTip.delay: 500
                ToolTip.visible: hovered
                ToolTip.text: qsTr("Tells Windows the network you connect to during setup is "
                    + "private / trusted, no 'Is this a home, work or public network?' prompt. The result "
                    + "is the same firewall profile a home or office LAN gets: file sharing and network "
                    + "discovery enabled. Pick this on a LAN you control; skip it on cafés / hotels.")
            }
            WrapCheckBox {
                text: qsTr("Disable data-collection / telemetry prompts")
                checked: app.disableTelemetry
                onToggled: app.disableTelemetry = checked
                ToolTip.delay: 500
                ToolTip.visible: hovered
                ToolTip.text: qsTr("Pre-selects the most privacy-conscious answers on the OOBE "
                    + "'Choose privacy settings for your device' screen: minimum required diagnostic "
                    + "data, no inking & typing telemetry, no advertising ID, no tailored experiences, "
                    + "no Find-my-device. Equivalent to clicking 'No' on every toggle and submitting.")
            }

            // --- Local account ------------------------------------------
            Label { text: qsTr("Local account"); font.bold: true; Layout.topMargin: 6 }
            Rectangle { Layout.fillWidth: true; height: 1; color: palette.mid; opacity: 0.5 }
            RowLayout {
                Layout.fillWidth: true
                Label { text: qsTr("Name"); Layout.minimumWidth: 110 }
                TextField {
                    Layout.fillWidth: true
                    // Qt sets implicitWidth from the placeholder; without
                    // a 0 minimum the field refuses to shrink below it
                    // and a long translated placeholder pushes the parent
                    // RowLayout past the dialog width.
                    Layout.minimumWidth: 0
                    placeholderText: qsTr("Optional, leave empty to keep the OOBE prompt")
                    text: app.localAccount
                    onTextEdited: app.localAccount = text
                }
            }
            RowLayout {
                Layout.fillWidth: true
                Label { text: qsTr("Password"); Layout.minimumWidth: 110 }
                TextField {
                    Layout.fillWidth: true
                    // Qt sets implicitWidth from the placeholder; without
                    // a 0 minimum the field refuses to shrink below it
                    // and a long translated placeholder pushes the parent
                    // RowLayout past the dialog width.
                    Layout.minimumWidth: 0
                    placeholderText: qsTr("Optional, sets a password and enables one-shot auto-logon")
                    echoMode: TextInput.Password
                    text: app.localAccountPassword
                    onTextEdited: app.localAccountPassword = text
                }
            }

            // --- System identity ----------------------------------------
            Label { text: qsTr("System"); font.bold: true; Layout.topMargin: 6 }
            Rectangle { Layout.fillWidth: true; height: 1; color: palette.mid; opacity: 0.5 }
            RowLayout {
                Layout.fillWidth: true
                Label { text: qsTr("Computer name"); Layout.minimumWidth: 110 }
                TextField {
                    Layout.fillWidth: true
                    // Qt sets implicitWidth from the placeholder; without
                    // a 0 minimum the field refuses to shrink below it
                    // and a long translated placeholder pushes the parent
                    // RowLayout past the dialog width.
                    Layout.minimumWidth: 0
                    placeholderText: qsTr("Optional, up to 15 characters, no whitespace")
                    text: app.computerName
                    onTextEdited: app.computerName = text
                }
                Button {
                    icon.name: "view-refresh"
                    display: icon.name
                        ? AbstractButton.IconOnly
                        : AbstractButton.TextOnly
                    text: qsTr("Random name")
                    onClicked: {
                        // PC- + 6 uppercase alphanumerics: 9 chars total,
                        // comfortably under the 15-char NETBIOS hostname
                        // limit and visually distinct from the Windows
                        // DESKTOP-* default.
                        var alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789"
                        var s = ""
                        for (var i = 0; i < 6; i++) {
                            s += alphabet.charAt(Math.floor(Math.random() * alphabet.length))
                        }
                        app.computerName = "PC-" + s
                    }
                    ToolTip.delay: 500
                    ToolTip.visible: hovered
                    ToolTip.text: qsTr("Generate a random PC-XXXXXX name. "
                        + "Useful when you don't care what the host is "
                        + "called and just want something unique.")
                }
            }
            RowLayout {
                Layout.fillWidth: true
                Label { text: qsTr("Locale"); Layout.minimumWidth: 110 }
                TextField {
                    Layout.fillWidth: true
                    // Qt sets implicitWidth from the placeholder; without
                    // a 0 minimum the field refuses to shrink below it
                    // and a long translated placeholder pushes the parent
                    // RowLayout past the dialog width.
                    Layout.minimumWidth: 0
                    placeholderText: qsTr("Optional, e.g. en-US, fr-FR, de-DE")
                    text: app.locale
                    onTextEdited: app.locale = text
                }
            }
            RowLayout {
                Layout.fillWidth: true
                Label { text: qsTr("Time zone"); Layout.minimumWidth: 110 }
                FormCombo {
                    id: timezoneCombo
                    Layout.fillWidth: true
                    // Parallel newline-separated lists built once in Rust from
                    // the Microsoft TimeZone catalog, sorted by UTC offset.
                    readonly property var tzIds: app.timezoneIds.split("\n")
                    model: app.timezoneLabels.split("\n")
                    // Restore the previously-chosen ID on dialog open, and
                    // re-sync whenever `app.timezone` changes (e.g. the
                    // 'Copy from system' button below).
                    Component.onCompleted: currentIndex = Math.max(0, tzIds.indexOf(app.timezone))
                    onActivated: app.timezone = tzIds[currentIndex] || ""
                    Connections {
                        target: app
                        function onTimezoneChanged() {
                            var i = timezoneCombo.tzIds.indexOf(app.timezone)
                            if (i >= 0) timezoneCombo.currentIndex = i
                        }
                    }
                }
            }
            RowLayout {
                Layout.fillWidth: true
                Layout.topMargin: 2
                Item { Layout.preferredWidth: 110 }
                Button {
                    text: qsTr("Copy from system")
                    onClicked: app.replicateRegionalFromHost()
                    ToolTip.delay: 500
                    ToolTip.visible: hovered
                    ToolTip.text: qsTr("Reads the host's $LANG and /etc/localtime, picks the "
                        + "matching Microsoft TimeZone ID, and fills the Locale + Time zone "
                        + "fields. Saves typing en-US / Pacific Standard Time by hand.")
                }
            }

            // --- Debloat ------------------------------------------------
            Label { text: qsTr("Privacy & debloat"); font.bold: true; Layout.topMargin: 6 }
            Rectangle { Layout.fillWidth: true; height: 1; color: palette.mid; opacity: 0.5 }
            WrapCheckBox {
                text: qsTr("Disable automatic BitLocker device encryption")
                checked: app.disableBitlocker
                onToggled: app.disableBitlocker = checked
                ToolTip.delay: 500
                ToolTip.visible: hovered
                ToolTip.text: qsTr("Stops Windows 11 24H2+ from silently encrypting the system "
                    + "drive on first sign-in. Without this, recent installs auto-turn-on BitLocker "
                    + "and the user is never asked, leaving anyone who later mounts the disk from "
                    + "Linux or another Windows install staring at an unreadable partition. "
                    + "Recommended for dual-boot, lab, and IT-imaged systems.")
            }
            WrapCheckBox {
                text: qsTr("Install Windows CA 2023 Secure Boot policy")
                checked: app.windowsCa2023
                onToggled: app.windowsCa2023 = checked
                ToolTip.delay: 500
                ToolTip.visible: hovered
                ToolTip.text: qsTr("Lets older UEFI firmware boot recent Windows installers that "
                    + "are signed by Microsoft's new 'Windows UEFI CA 2023' chain. If your motherboard "
                    + "hasn't received the new CA via Windows Update yet (common on workstations / "
                    + "servers that don't run Windows), Secure-Boot will otherwise refuse the install. "
                    + "Needs wimlib-imagex on the host; the option silently no-ops on older Windows ISOs.")
            }
            WrapCheckBox {
                id: debloatBox
                text: qsTr("Apply debloat profile")
                checked: app.applyDebloat
                onToggled: app.applyDebloat = checked
                ToolTip.delay: 500
                ToolTip.visible: hovered
                ToolTip.text: qsTr("Turns off the Windows 'features' most people uninstall by hand: "
                    + "Cortana voice assistant, Copilot, Recall (the AI screenshot history), the "
                    + "News & Interests taskbar widget, Bing / web suggestions in Start search, "
                    + "lockscreen ads + 'suggested' apps + suggested toast notifications, the "
                    + "advertising ID, tailored experiences, online speech model downloads, the "
                    + "Phone-Link prompt, and the Feedback-Hub frequency popups. Telemetry is "
                    + "knocked down to the minimum the OS still requires. Applied both machine-wide "
                    + "and for every new user account. Win-11-only entries silently no-op on Win 10. "
                    + "Click the box again to see the full per-item list.")
            }
            Label {
                visible: debloatBox.checked
                Layout.fillWidth: true
                Layout.leftMargin: 24
                wrapMode: Text.Wrap
                color: palette.placeholderText
                font.pointSize: 9
                textFormat: Text.RichText
                text: qsTr(
                    "<b>Applied machine-wide (HKLM Group Policy):</b><br>"
                    + "&nbsp;• News &amp; Interests feed (taskbar widget)<br>"
                    + "&nbsp;• Consumer-feature ads: suggested Store apps, OEM-style inserts<br>"
                    + "&nbsp;• Activity History sync to Microsoft<br>"
                    + "&nbsp;• Cortana in Search<br>"
                    + "&nbsp;• Windows Copilot service<br>"
                    + "&nbsp;• Windows Recall: the rolling-screenshot AI history (Win 11 24H2+)<br>"
                    + "&nbsp;• Diagnostic data: set to Required only<br>"
                    + "<br>"
                    + "<b>Applied to the default user profile (inherited by every new account):</b><br>"
                    + "&nbsp;• Bing / web suggestions in Start &amp; Search<br>"
                    + "&nbsp;• File extensions shown (instead of hidden)<br>"
                    + "&nbsp;• Copilot, Task View, Widgets and \"People\" buttons hidden from the taskbar<br>"
                    + "&nbsp;• Sync-provider ads in Explorer suppressed<br>"
                    + "&nbsp;• Start menu \"recommendations\" and Iris suggestions disabled<br>"
                    + "&nbsp;• ContentDeliveryManager: lock-screen rotation ads, pre-installed-app suggestions, \"subscribed content\" tiles<br>"
                    + "&nbsp;• Cortana / Bing inside per-user Search<br>"
                    + "&nbsp;• Advertising ID disabled<br>"
                    + "&nbsp;• \"Tailored experiences\" derived from diagnostic data<br>"
                    + "&nbsp;• \"Suggested\" toast notifications<br>"
                    + "&nbsp;• Phone Link / \"use your mobile with Windows\" prompts<br>"
                    + "&nbsp;• Online speech recognition (voice stays local)<br>"
                    + "&nbsp;• Contact harvesting for input personalization<br>"
                    + "&nbsp;• Feedback Hub frequency set to Never<br>"
                    + "&nbsp;• \"Finish setting up your device\" prompts<br>"
                    + "<br>"
                    + "Windows 11-only keys (Copilot, Widgets, News &amp; Interests, Recall) "
                    + "are silently ignored on Windows 10.")
            }

            // --- Post-install desktop helpers ---------------------------
            Label {
                text: qsTr("Post-install desktop helpers")
                font.bold: true
                Layout.topMargin: 6
            }
            Rectangle { Layout.fillWidth: true; height: 1; color: palette.mid; opacity: 0.5 }
            WrapCheckBox {
                id: desktopHelpersBox
                text: qsTr("Drop a USBooty folder on the user's Desktop with ready-to-run scripts")
                checked: app.desktopHelpers
                onToggled: app.desktopHelpers = checked
                ToolTip.delay: 500
                ToolTip.visible: hovered
                ToolTip.text: qsTr("After install, the user finds a USBooty folder on their "
                    + "Desktop with right-click → \"Run as administrator\" .bat scripts: "
                    + "Win11Debloat (Raphire), Chris Titus winutil (stable + dev), Microsoft "
                    + "Activation Scripts (Massgrave), an OneDrive remover, an OfficeTool "
                    + "downloader, one-click installers for Chocolatey, Scoop and winget, a "
                    + "Windows-AI stripper (Copilot / Recall / generative Paint+Notepad), "
                    + "Winhance, FR33THY's Ultimate gaming/latency tweaks, PowerToys, system "
                    + "tweaks (Fast Startup off, long paths on), VC++ Redistributables 2015-"
                    + "2022 and DirectX legacy runtimes, plus an interactive browser-installer "
                    + "menu (Chrome, Firefox, Brave, Zen, LibreWolf, Floorp, Waterfox, Opera, "
                    + "Opera GX, Vivaldi, Arc). The folder is copied to the Default user "
                    + "profile during Windows setup, so every account created at OOBE inherits "
                    + "it.")
            }
            Label {
                visible: desktopHelpersBox.checked
                Layout.fillWidth: true
                Layout.leftMargin: 24
                wrapMode: Text.Wrap
                color: palette.placeholderText
                font.pointSize: 9
                textFormat: Text.RichText
                // Backslashes in literal Windows paths confuse lupdate's
                // QML lexer (it eats `\U` from "C:\Users\" as an escape).
                // Use the &#x5C; HTML entity — RichText renders it as `\`.
                text: qsTr(
                    "Lands in <code>C:&#x5C;Users&#x5C;&lt;NewUser&gt;&#x5C;Desktop&#x5C;USBooty&#x5C;</code>:<br>"
                    + "&nbsp;• <b>1-Win11Debloat.bat</b>: Raphire's debloat (debloat.raphi.re)<br>"
                    + "&nbsp;• <b>2-ChrisTitus-Winutil.bat</b>: Chris Titus winutil, stable channel<br>"
                    + "&nbsp;• <b>2.1-ChrisTitus-Winutil-Dev.bat</b>: same tool, dev channel<br>"
                    + "&nbsp;• <b>3-Massgravel-Activator.bat</b>: Microsoft Activation Scripts (MAS)<br>"
                    + "&nbsp;• <b>4-Remove-OneDrive.bat</b>: kill + uninstall OneDrive (x64 &amp; WoW64)<br>"
                    + "&nbsp;• <b>5-OfficeTool.bat</b>: download OfficeTool runtime<br>"
                    + "&nbsp;• <b>6-Install-Chocolatey.bat</b>: install Chocolatey (machine-wide, admin)<br>"
                    + "&nbsp;• <b>7-Install-Scoop.bat</b>: install Scoop (per-user, no admin)<br>"
                    + "&nbsp;• <b>8-Install-Winget.bat</b>: install / repair winget (asheroto)<br>"
                    + "&nbsp;• <b>9-Remove-Windows-AI.bat</b>: strip Copilot / Recall / AI features (zoicware)<br>"
                    + "&nbsp;• <b>10-Winhance.bat</b>: Winhance (debloat / privacy / optimise GUI)<br>"
                    + "&nbsp;• <b>11-FR33THY-Ultimate.bat</b>: FR33THY's Ultimate gaming / latency tweaks<br>"
                    + "&nbsp;• <b>12-Install-PowerToys.bat</b>: Microsoft PowerToys via winget<br>"
                    + "&nbsp;• <b>13-Disable-FastStartup.bat</b>: clear HiberbootEnabled (dual-boot fix)<br>"
                    + "&nbsp;• <b>14-Enable-LongPaths.bat</b>: set LongPathsEnabled=1 (developer)<br>"
                    + "&nbsp;• <b>15-Install-VCRedist.bat</b>: VC++ Redistributable 2015-2022, x64 + x86<br>"
                    + "&nbsp;• <b>16-Install-DirectX.bat</b>: legacy DirectX runtime (older games)<br>"
                    + "&nbsp;• <b>17-Install-Browser.bat</b>: menu: Chrome, Firefox, Brave, Zen, LibreWolf, Floorp, Waterfox, Opera, Opera GX, Vivaldi, Arc<br>"
                    + "<br>"
                    + "Each script fetches code from the public internet on first run.")
            }
            }
        }
        }
    }

    // Final go/no-go before the helper touches the device. Styled in the
    // same banner-headed visual language as the Windows Setup / Download
    // dialogs, but in a danger-red palette since this is destructive.
    // Field values are pushed in via onOpened so the bindings can't go
    // stale (invokables don't emit change signals).
    Dialog {
        id: confirmDialog
        anchors.centerIn: parent
        width: Math.min(480, window.width - 40)
        modal: true
        topPadding: 14
        bottomPadding: 14
        leftPadding: 18
        rightPadding: 18
        // The Rust invokables don't emit change signals, so a plain
        // binding to `app.selectedX()` would never refresh. Gating on
        // `visible` re-evaluates the binding every time the dialog
        // opens — same effect as the imperative onOpened we used to run,
        // but free of an extra signal handler.
        property bool internalDisk: confirmDialog.visible && app.selectedIsInternal()
        property string busLabel: confirmDialog.visible ? app.selectedBus() : ""
        property string serialLabel: confirmDialog.visible ? app.selectedSerial() : ""
        header: DialogHeader {
            tint: "#C0392B"
            iconGlyph: "⚠"
            title: qsTr("Erase device?")
            subtitle: qsTr("All data on the target will be permanently lost")
        }
        standardButtons: Dialog.Ok | Dialog.Cancel
        onAccepted: app.start()
        // The card + the three labels carry no state at startup. Only
        // build them when the dialog opens — the Loader re-instantiates
        // on each open so the text bindings to invokables re-evaluate
        // with the current selection.
        contentItem: Loader {
            active: confirmDialog.visible
            sourceComponent: ColumnLayout {
            spacing: 12
            // Target device card.
            Rectangle {
                Layout.fillWidth: true
                radius: 8
                color: confirmDialog.internalDisk
                    ? Qt.tint(palette.base, Qt.rgba(0.75, 0.23, 0.17, 0.14))
                    : Qt.tint(palette.base, Qt.rgba(palette.highlight.r,
                                                   palette.highlight.g,
                                                   palette.highlight.b, 0.10))
                border.color: confirmDialog.internalDisk
                    ? "#C0392B" : palette.mid
                implicitHeight: cardCol.implicitHeight + 24
                ColumnLayout {
                    id: cardCol
                    anchors.fill: parent
                    anchors.margins: 12
                    spacing: 2
                    Label {
                        // Each open re-instantiates the Loader, so the
                        // invokable is re-called with a fresh selection.
                        text: app.selectedModel()
                        font.bold: true
                        font.pointSize: 13
                        elide: Text.ElideRight
                        Layout.fillWidth: true
                    }
                    RowLayout {
                        Layout.fillWidth: true
                        spacing: 10
                        Label {
                            text: app.selectedSizeText()
                            color: palette.windowText
                            font.pointSize: 11
                        }
                        Pill {
                            visible: confirmDialog.busLabel !== ""
                            label: confirmDialog.busLabel
                            tint: palette.mid
                            ink: palette.windowText
                        }
                        Item { Layout.fillWidth: true }
                        Label {
                            text: app.selectedPath()
                            color: palette.placeholderText
                            font.family: "monospace"
                            font.pointSize: 10
                            elide: Text.ElideMiddle
                            horizontalAlignment: Text.AlignRight
                        }
                    }
                    Label {
                        visible: confirmDialog.serialLabel !== ""
                        text: qsTr("Serial: %1").arg(confirmDialog.serialLabel)
                        color: palette.placeholderText
                        font.family: "monospace"
                        font.pointSize: 9
                        elide: Text.ElideRight
                        Layout.fillWidth: true
                    }
                    Label {
                        visible: confirmDialog.internalDisk
                        Layout.fillWidth: true
                        Layout.topMargin: 6
                        text: qsTr("⚠ This is an INTERNAL (non-removable) disk. "
                                 + "Make absolutely sure it is the device you mean to erase.")
                        // Red-shifted from the theme's main text colour so
                        // the warning pops on both light and dark themes.
                        color: Qt.tint(window.palette.windowText,
                                       Qt.rgba(0.86, 0.21, 0.27, 0.85))
                        wrapMode: Text.Wrap
                        font.bold: true
                        font.pointSize: 9
                    }
                }
            }
            // Secondary action — opens the read-only Inspect modal (lsblk /
            // udevadm / smartctl dump). Kept inside the contentItem rather
            // than the dialog footer because mixing custom footer buttons
            // with Dialog.Ok / Dialog.Cancel breaks the accept signal in
            // Qt 6 Quick Controls.
            Button {
                flat: true
                Layout.alignment: Qt.AlignLeft
                text: qsTr("🔍  Inspect device details…")
                ToolTip.delay: 500
                ToolTip.visible: hovered
                ToolTip.text: qsTr("Open lsblk + udevadm + smartctl output for this device "
                    + "in a read-only panel. Useful if anything above looks off.")
                onClicked: {
                    // Kick the worker, then open the dialog right away.
                    // The TextArea binds to app.inspectText, so it shows
                    // the placeholder while lsblk/udevadm/smartctl run.
                    app.requestInspect()
                    inspectDialog.open()
                }
            }
            Label {
                text: app.method === 3 && app.ventoyUpdate
                    ? qsTr("Ventoy will be updated. Your existing ISOs on the data partition are kept.")
                    : qsTr("All data on this device will be permanently erased.")
                wrapMode: Text.Wrap
                Layout.fillWidth: true
            }
            Label {
                text: qsTr("This cannot be undone.")
                font.bold: true
            }
            }
        }
    }

    // Read-only dump of `lsblk -O` + `udevadm info` for the device that's
    // about to be erased — opened from the confirm dialog so the user can
    // sanity-check what the system actually thinks the device is.
    Dialog {
        id: inspectDialog
        anchors.centerIn: parent
        width: Math.min(window.width - 60, 720)
        height: Math.min(window.height - 80, 560)
        modal: true
        topPadding: 14
        bottomPadding: 14
        leftPadding: 18
        rightPadding: 18
        header: DialogHeader {
            tint: palette.highlight
            iconGlyph: "ⓘ"
            title: qsTr("Device details")
            subtitle: qsTr("Read-only: lsblk + udevadm output for the chosen device")
        }
        standardButtons: Dialog.Close
        // The monospace TextArea only matters when the user has opened
        // the inspect panel; until then it doesn't need to exist.
        contentItem: Loader {
            active: inspectDialog.visible
            sourceComponent: ScrollView {
            id: inspectScroll
            clip: true
            ScrollBar.vertical.policy: ScrollBar.AsNeeded
            TextArea {
                id: inspectText
                width: inspectScroll.availableWidth
                readOnly: true
                wrapMode: TextArea.NoWrap
                font.family: "monospace"
                font.pointSize: 9
                selectByMouse: true
                // Worker output lands here via the qproperty. The body of
                // the bind shows the placeholder while the children run.
                text: app.inspectText
            }
            }
        }
    }

    // Every dependency (required + optional), grouped, with a live
    // available/missing status. Re-probed each time it opens.
    Dialog {
        id: depsDialog
        anchors.centerIn: parent
        width: Math.min(560, window.width - 40)
        height: Math.min(window.height - 80, 600)
        modal: true
        topPadding: 14
        bottomPadding: 14
        leftPadding: 18
        rightPadding: 18
        // Parsed rows from app.dependencyReport(); refreshed on open.
        property var rows: []
        function refresh() {
            var out = []
            var lines = app.dependencyReport().split("\n")
            for (var i = 0; i < lines.length; i++) {
                if (lines[i] === "")
                    continue
                var f = lines[i].split("")
                out.push({ present: f[0] === "1", group: f[1],
                           name: f[2], pkg: f[3], purpose: f[4] })
            }
            rows = out
        }
        readonly property int presentCount:
            rows.filter(function(d) { return d.present }).length
        header: DialogHeader {
            tint: palette.highlight
            iconGlyph: "ⓘ"
            title: qsTr("Dependencies")
            subtitle: qsTr("%1 of %2 present").arg(depsDialog.presentCount)
                                              .arg(depsDialog.rows.length)
        }
        standardButtons: Dialog.Close
        contentItem: ScrollView {
            id: depsScroll
            clip: true
            ScrollBar.vertical.policy: ScrollBar.AsNeeded
            ScrollBar.horizontal.policy: ScrollBar.AlwaysOff
            ColumnLayout {
                width: depsScroll.availableWidth
                spacing: 12
                Repeater {
                    model: [
                        { key: "required",   title: qsTr("Required") },
                        { key: "filesystem", title: qsTr("Filesystem formatters") },
                        { key: "feature",    title: qsTr("Feature backends") },
                        { key: "boot",       title: qsTr("Boot test (QEMU)") },
                        { key: "qol",        title: qsTr("Quality-of-life") }
                    ]
                    delegate: ColumnLayout {
                        id: section
                        // The category key this section filters the dep rows by.
                        readonly property string catKey: modelData.key
                        Layout.fillWidth: true
                        spacing: 4
                        Label {
                            text: modelData.title
                            font.bold: true
                            font.pointSize: 11
                            color: palette.highlight
                            Layout.fillWidth: true
                        }
                        Repeater {
                            model: depsDialog.rows.filter(function(d) {
                                return d.group === section.catKey
                            })
                            delegate: RowLayout {
                                id: deprow
                                Layout.fillWidth: true
                                Layout.leftMargin: 6
                                spacing: 8
                                Label {
                                    text: modelData.present ? "✓" : "✗"
                                    color: modelData.present ? "#27AE60" : "#E74C3C"
                                    font.bold: true
                                    font.pointSize: 11
                                    Layout.alignment: Qt.AlignTop
                                }
                                ColumnLayout {
                                    Layout.fillWidth: true
                                    spacing: 0
                                    RowLayout {
                                        Layout.fillWidth: true
                                        spacing: 8
                                        Label {
                                            text: modelData.name
                                            font.bold: true
                                            elide: Text.ElideRight
                                        }
                                        Label {
                                            text: modelData.pkg
                                            font.family: "monospace"
                                            font.pointSize: 8
                                            color: palette.placeholderText
                                            elide: Text.ElideRight
                                        }
                                        Item { Layout.fillWidth: true }
                                        Label {
                                            text: modelData.present
                                                ? qsTr("available") : qsTr("missing")
                                            color: modelData.present ? "#27AE60" : "#E74C3C"
                                            font.pointSize: 8
                                        }
                                    }
                                    Label {
                                        text: modelData.purpose
                                        color: palette.placeholderText
                                        font.pointSize: 8
                                        wrapMode: Text.Wrap
                                        Layout.fillWidth: true
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Boot-test the selected device in QEMU (BIOS/MBR or UEFI). Minimal
    // config: firmware, memory, KVM. The device is opened in snapshot mode so
    // the test never writes back to it.
    Dialog {
        id: bootTestDialog
        anchors.centerIn: parent
        width: Math.min(460, window.width - 40)
        modal: true
        topPadding: 14
        bottomPadding: 14
        leftPadding: 18
        rightPadding: 18
        // Selectable RAM sizes (MiB) and the current pick.
        readonly property var memValues: [1024, 2048, 4096, 8192]
        property int memIndex: 1
        // Default to UEFI when the firmware is available, else BIOS.
        property bool uefi: app.qemuUefi
        onAboutToShow: uefi = app.qemuUefi
        header: DialogHeader {
            // Teal, matching the "Ventoy" phase accent and distinct from the
            // blue (Microsoft), red (erase) and green/red (result) headers.
            tint: "#16A085"
            iconGlyph: "▶"
            title: qsTr("Verify boot device")
            subtitle: qsTr("Boot the selected device in QEMU — it is not modified")
        }
        footer: DialogButtonBox {
            standardButtons: DialogButtonBox.Cancel
            Button {
                text: qsTr("Launch")
                enabled: app.qemuAvailable
                DialogButtonBox.buttonRole: DialogButtonBox.AcceptRole
                onClicked: {
                    app.verifyBoot(bootTestDialog.memValues[bootTestDialog.memIndex],
                                   bootTestDialog.uefi,
                                   kvmCheck.checked)
                    bootTestDialog.close()
                }
            }
        }
        contentItem: ColumnLayout {
            spacing: 10
            // Target device path.
            RowLayout {
                Layout.fillWidth: true
                Label {
                    text: qsTr("Device")
                    font.bold: true
                }
                Item { Layout.fillWidth: true }
                Label {
                    text: app.selectedPath()
                    color: palette.placeholderText
                    font.family: "monospace"
                    font.pointSize: 10
                    elide: Text.ElideMiddle
                }
            }
            // Hard stop when QEMU itself is missing.
            Label {
                visible: !app.qemuAvailable
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                text: qsTr("⚠ qemu-system-x86_64 was not found. Install the 'qemu-full' "
                    + "(Arch) or 'qemu-system-x86' (Debian/Ubuntu) package to use this.")
                color: Qt.tint(window.palette.windowText, Qt.rgba(0.86, 0.21, 0.27, 0.85))
                font.bold: true
            }
            // Firmware: BIOS/MBR vs UEFI.
            RowLayout {
                Layout.fillWidth: true
                Label {
                    text: qsTr("Firmware")
                    Layout.preferredWidth: 90
                }
                RadioButton {
                    text: qsTr("BIOS / MBR")
                    checked: !bootTestDialog.uefi
                    onClicked: bootTestDialog.uefi = false
                }
                RadioButton {
                    text: qsTr("UEFI")
                    enabled: app.qemuUefi
                    checked: bootTestDialog.uefi
                    onClicked: bootTestDialog.uefi = true
                }
                Item { Layout.fillWidth: true }
            }
            Label {
                visible: !app.qemuUefi
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                text: qsTr("UEFI test needs OVMF firmware (install 'edk2-ovmf' / 'ovmf').")
                color: palette.placeholderText
                font.pointSize: 8
            }
            // Memory.
            RowLayout {
                Layout.fillWidth: true
                Label {
                    text: qsTr("Memory")
                    Layout.preferredWidth: 90
                }
                FormCombo {
                    id: memCombo
                    model: ["1 GiB", "2 GiB", "4 GiB", "8 GiB"]
                    currentIndex: bootTestDialog.memIndex
                    onActivated: bootTestDialog.memIndex = currentIndex
                }
            }
            // Hardware acceleration.
            CheckBox {
                id: kvmCheck
                text: qsTr("Hardware acceleration (KVM)")
                enabled: app.qemuKvm
                checked: app.qemuKvm
            }
            Label {
                visible: !app.qemuKvm
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                text: qsTr("KVM is unavailable (/dev/kvm missing); the VM will run under "
                    + "slower software emulation.")
                color: palette.placeholderText
                font.pointSize: 8
            }
            Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                text: qsTr("Opens in snapshot mode, so nothing is written back to the device. "
                    + "Admin rights are required to read the raw device.")
                color: palette.placeholderText
                font.pointSize: 8
            }
        }
    }

    Dialog {
        id: resultDialog
        anchors.centerIn: parent
        width: Math.min(480, window.width - 40)
        modal: true
        topPadding: 14
        bottomPadding: 14
        leftPadding: 18
        rightPadding: 18
        // Set by the AppController.onJobFinished handler; drives the header
        // colour and the title text.
        property bool success: true
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
            wrapMode: Text.Wrap
        }
    }

    Dialog {
        id: aboutDialog
        anchors.centerIn: parent
        // Tight cap; the dialog has to fit comfortably inside the 660 px
        // compact window so it never spills past the parent.
        width: Math.min(500, window.width - 40)
        modal: true
        topPadding: 14
        bottomPadding: 12
        leftPadding: 18
        rightPadding: 18
        header: DialogHeader {
            // Deep blurple — close to Discord's brand mark (#5865F2) but a
            // few shades darker, so the white USBooty logo + title pop
            // without competing with the Microsoft-blue / red Erase headers.
            tint: "#4752C4"
            iconComponent: Image {
                source: "qrc:/icons/usbooty.svg"
                sourceSize.width: 28
                sourceSize.height: 28
                width: 28
                height: 28
                fillMode: Image.PreserveAspectFit
                smooth: true
            }
            title: "USBooty"
            subtitle: qsTr("Bootable USB Creator · Version %1").arg(app.appVersion)
        }
        standardButtons: Dialog.Ok
        // The 64 px SVG logo, the GridLayout, the link-handling Labels and
        // the three external-launch Buttons cost nothing at startup if
        // they don't exist yet.
        contentItem: Loader {
            active: aboutDialog.visible
            sourceComponent: ColumnLayout {
            spacing: 12

            // Centred logo + one-line tagline; small enough to keep the
            // dialog short on the compact 660 px window.
            Image {
                source: "qrc:/icons/usbooty.svg"
                sourceSize.width: 64
                sourceSize.height: 64
                Layout.preferredWidth: 64
                Layout.preferredHeight: 64
                Layout.alignment: Qt.AlignHCenter
                Layout.topMargin: 4
                fillMode: Image.PreserveAspectFit
                smooth: true
            }
            Label {
                text: qsTr("Create bootable USB drives from ISO images.")
                wrapMode: Text.Wrap
                horizontalAlignment: Text.AlignHCenter
                Layout.fillWidth: true
            }

            // Author + License + Source on one tidy row each.
            GridLayout {
                columns: 2
                columnSpacing: 14
                rowSpacing: 3
                Layout.fillWidth: true
                Layout.topMargin: 4
                Label { text: qsTr("Author"); font.bold: true }
                Label { text: "Thoxy" }
                Label { text: qsTr("License"); font.bold: true }
                Label {
                    Layout.fillWidth: true
                    text: "<a href=\"https://www.gnu.org/licenses/gpl-3.0.html\">GPL-3.0-or-later</a>"
                    textFormat: Text.RichText
                    onLinkActivated: function(link) { Qt.openUrlExternally(link) }
                }
                Label { text: qsTr("Source"); font.bold: true }
                Label {
                    Layout.fillWidth: true
                    elide: Text.ElideRight
                    text: "<a href=\"https://git.thoxy.xyz/thoxy/usbooty\">git.thoxy.xyz/thoxy/usbooty</a>"
                    textFormat: Text.RichText
                    onLinkActivated: function(link) { Qt.openUrlExternally(link) }
                }
            }

            // Single-line "what it does" — replaces the four-column
            // feature grid that was overflowing the compact window.
            Label {
                Layout.fillWidth: true
                Layout.topMargin: 4
                color: palette.placeholderText
                font.pointSize: 9
                wrapMode: Text.Wrap
                text: qsTr("DD raw / partition+copy / format / Ventoy / FreeDOS · "
                    + "FAT16-32, NTFS, exFAT, UDF, ext2/3/4, Btrfs, XFS, F2FS · "
                    + "Linux persistence · Windows 11 setup customisation · "
                    + "BLAKE3 verify · SBAT + DBX revocation · SMART probe.")
            }

            // Quick action row — Docs / Source-code / Report-issue.
            RowLayout {
                Layout.fillWidth: true
                Layout.topMargin: 2
                spacing: 4
                Button {
                    text: qsTr("Docs")
                    icon.name: "help-contents"
                    display: icon.name
                        ? AbstractButton.TextBesideIcon
                        : AbstractButton.TextOnly
                    flat: true
                    onClicked: Qt.openUrlExternally(
                        "https://git.thoxy.xyz/thoxy/usbooty/wiki")
                }
                Button {
                    text: qsTr("Source code")
                    icon.name: "applications-development"
                    display: icon.name
                        ? AbstractButton.TextBesideIcon
                        : AbstractButton.TextOnly
                    flat: true
                    onClicked: Qt.openUrlExternally("https://git.thoxy.xyz/thoxy/usbooty")
                }
                Button {
                    text: qsTr("Report an issue")
                    icon.name: "tools-report-bug"
                    display: icon.name
                        ? AbstractButton.TextBesideIcon
                        : AbstractButton.TextOnly
                    flat: true
                    onClicked: Qt.openUrlExternally("https://git.thoxy.xyz/thoxy/usbooty/issues")
                }
                Item { Layout.fillWidth: true }
            }
            }
        }
    }

    Dialog {
        id: winDialog
        anchors.centerIn: parent
        width: Math.min(500, window.width - 40)
        modal: true
        // Frame the contents away from the coloured header so the layout
        // mirrors the Windows Setup dialog above.
        topPadding: 14
        bottomPadding: 14
        leftPadding: 18
        rightPadding: 18
        header: DialogHeader {
            tint: "#0078D4"
            iconComponent: WindowsLogo { size: 24; tint: "white" }
            title: qsTr("Download a Windows ISO")
            subtitle: qsTr("Pull an official image directly from Microsoft")
        }
        standardButtons: Dialog.Close
        // Only build the three combo boxes + the Microsoft-fetch buttons
        // when the dialog actually opens. Nothing inside this contentItem
        // is needed at startup.
        contentItem: Loader {
            active: winDialog.visible
            sourceComponent: ColumnLayout {
            spacing: 10
            Label {
                text: qsTr("Fetch an official ISO from Microsoft. Each step queries "
                    + "Microsoft and may take a few seconds.")
                wrapMode: Text.Wrap
                Layout.fillWidth: true
            }

            // Step 1 — choose the Windows release.
            RowLayout {
                Layout.fillWidth: true
                FormCombo {
                    id: winVersion
                    Layout.fillWidth: true
                    enabled: !app.busy
                    // Windows release names are brand identifiers — leave untranslated.
                    model: ["Windows 11", "Windows 10"]
                }
                Button {
                    text: qsTr("List languages")
                    enabled: !app.busy
                    onClicked: app.winFetchLanguages(winVersion.currentIndex)
                }
            }

            // Step 2 — choose a language.
            RowLayout {
                Layout.fillWidth: true
                FormCombo {
                    id: winLang
                    Layout.fillWidth: true
                    enabled: !app.busy && app.winLanguages !== ""
                    model: app.winLanguages !== "" ? app.winLanguages.split("\n") : []
                }
                Button {
                    text: qsTr("List downloads")
                    enabled: !app.busy && app.winLanguages !== ""
                    onClicked: app.winFetchOptions(winLang.currentIndex)
                }
            }

            // Step 3 — choose an edition/architecture and download.
            RowLayout {
                Layout.fillWidth: true
                FormCombo {
                    id: winOpt
                    Layout.fillWidth: true
                    enabled: !app.busy && app.winOptions !== ""
                    model: app.winOptions !== "" ? app.winOptions.split("\n") : []
                }
                Button {
                    text: qsTr("Download")
                    highlighted: true
                    enabled: !app.busy && app.winOptions !== ""
                    onClicked: {
                        app.winDownload(winOpt.currentIndex)
                        winDialog.close()
                    }
                }
            }

            Label {
                text: window.trMsg(app.status)
                color: palette.placeholderText
                wrapMode: Text.Wrap
                Layout.fillWidth: true
            }

            Label {
                text: qsTr("If Microsoft's anti-bot system rejects the request "
                    + "(common on VPNs and some networks), download manually:")
                color: palette.placeholderText
                wrapMode: Text.Wrap
                Layout.fillWidth: true
            }
            Button {
                text: qsTr("Open Microsoft download page")
                Layout.fillWidth: true
                onClicked: app.openMicrosoftPage(winVersion.currentIndex)
            }
            }
        }
    }
}
