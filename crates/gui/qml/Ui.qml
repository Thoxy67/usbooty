pragma Singleton
import QtQuick

// Shared UI helpers, hoisted out of main.qml so every QML file in the module
// (cards, dialogs, the window shell) can reach them without threading them
// through properties. Referenced as `Ui.trPhase(...)` after `import com.usbooty`.
//
// The qsTr() literals below live in this file's translation context ("Ui");
// `data/translations/update-translations.sh` scans the whole qml tree, so they
// stay in the catalog regardless of which file they sit in.
QtObject {
    // Translate one of the runtime phase strings emitted by the helper /
    // runner / bridge. The phases come from Rust as fixed English
    // identifiers, so we map them to static qsTr() literals here. That
    // lets lupdate extract them and the QTranslator actually find a
    // translation at runtime; `qsTr(app.phase)` would never match,
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
}
