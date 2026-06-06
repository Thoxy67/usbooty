import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import com.usbooty

Dialog {
    id: bootTestDialog
    required property var app
    required property var host
        anchors.centerIn: parent
        width: Math.min(460, host.width - 40)
        modal: true
        topPadding: 14
        bottomPadding: 14
        leftPadding: 18
        rightPadding: 18
        // vCPU and memory picks, set by the sliders below. Defaults: half the
        // host cores, and 4 GiB capped to host RAM.
        property int cpus: Math.max(1, Math.floor(app.qemuCpusMax / 2))
        property int memMb: Math.max(512, Math.min(4096, app.qemuRamMax))
        // Firmware: 0 = BIOS (SeaBIOS), 1 = UEFI (OVMF), 2 = UEFI + Secure Boot.
        // Default to UEFI when OVMF is available, else BIOS.
        property int firmware: app.qemuUefi ? 1 : 0
        readonly property bool isUefi: firmware !== 0
        onAboutToShow: firmware = app.qemuUefi ? 1 : 0
        header: DialogHeader {
            // Teal, matching the "Ventoy" phase accent and distinct from the
            // blue (Microsoft), red (erase) and green/red (result) headers.
            tint: "#16A085"
            iconGlyph: "▶"
            title: qsTr("Verify boot device")
            subtitle: qsTr("Boot the selected device in QEMU; it is not modified")
        }
        footer: DialogButtonBox {
            standardButtons: DialogButtonBox.Cancel
            Button {
                text: qsTr("Launch")
                enabled: app.qemuAvailable
                DialogButtonBox.buttonRole: DialogButtonBox.AcceptRole
                onClicked: {
                    app.verifyBoot(bootTestDialog.memMb,
                                   bootTestDialog.cpus,
                                   bootTestDialog.firmware,
                                   machineCombo.currentIndex === 1,
                                   audioCheck.checked,
                                   kvmCheck.checked,
                                   networkCheck.checked,
                                   snapshotCheck.checked)
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
                color: Qt.tint(host.palette.windowText, Qt.rgba(0.86, 0.21, 0.27, 0.85))
                font.bold: true
            }
            // Firmware: BIOS (SeaBIOS) / UEFI (OVMF) / UEFI + Secure Boot.
            RowLayout {
                Layout.fillWidth: true
                Label {
                    text: qsTr("Firmware")
                    Layout.preferredWidth: 90
                }
                ComboBox {
                    id: fwCombo
                    Layout.fillWidth: true
                    textRole: "text"
                    valueRole: "fw"
                    model: {
                        var m = [{ text: qsTr("BIOS / MBR (SeaBIOS)"), fw: 0 }]
                        if (app.qemuUefi)
                            m.push({ text: qsTr("UEFI (OVMF)"), fw: 1 })
                        if (app.qemuSecureboot)
                            m.push({ text: qsTr("UEFI + Secure Boot (OVMF)"), fw: 2 })
                        return m
                    }
                    Component.onCompleted: currentIndex = indexOfValue(bootTestDialog.firmware)
                    onActivated: bootTestDialog.firmware = currentValue
                }
            }
            // Machine type / chipset.
            RowLayout {
                Layout.fillWidth: true
                Label {
                    text: qsTr("Machine")
                    Layout.preferredWidth: 90
                }
                ComboBox {
                    id: machineCombo
                    Layout.fillWidth: true
                    // Index 0 = i440fx (legacy "pc"), 1 = q35 (modern). q35
                    // default, better for Windows 11 and modern guests.
                    model: ["i440fx (legacy)", "q35 (modern)"]
                    currentIndex: 1
                }
            }
            Label {
                visible: !app.qemuUefi
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                text: qsTr("UEFI test needs OVMF firmware (install 'edk2-ovmf' / 'ovmf').")
                color: palette.placeholderText
                font.pointSize: 8
            }
            // TPM status for UEFI boots; Windows 11 OOBE needs a TPM 2.0.
            Label {
                visible: bootTestDialog.isUefi
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                text: app.qemuTpm
                    ? qsTr("✓ A virtual TPM 2.0 (swtpm) will be attached, needed for Windows 11 OOBE.")
                    : qsTr("⚠ swtpm not installed: no TPM 2.0 will be attached, so Windows 11 OOBE may "
                        + "loop on \"Why did my PC restart?\". Install the 'swtpm' package.")
                color: app.qemuTpm
                    ? palette.placeholderText
                    : Qt.tint(host.palette.windowText, Qt.rgba(0.86, 0.21, 0.27, 0.85))
                font.pointSize: 8
            }
            // Memory: slider from 512 MiB up to the host's total RAM.
            RowLayout {
                Layout.fillWidth: true
                Label {
                    text: qsTr("Memory")
                    Layout.preferredWidth: 90
                }
                Slider {
                    id: memSlider
                    Layout.fillWidth: true
                    from: 512
                    to: Math.max(1024, app.qemuRamMax)
                    stepSize: 256
                    snapMode: Slider.SnapAlways
                    value: bootTestDialog.memMb
                    onMoved: bootTestDialog.memMb = value
                }
                SpinBox {
                    id: memSpin
                    // Size to content (text + spin buttons) so longer values
                    // like "32005 MiB" aren't clipped, with a sane floor.
                    Layout.preferredWidth: Math.max(140, implicitWidth)
                    editable: true
                    from: 512
                    to: Math.max(1024, app.qemuRamMax)
                    stepSize: 256
                    value: bootTestDialog.memMb
                    onValueModified: bootTestDialog.memMb = value
                    textFromValue: function(v) { return v + " MiB" }
                    valueFromText: function(t) { return parseInt(t) }
                }
            }
            // Processors: slider from 1 up to the host's logical CPU count.
            RowLayout {
                Layout.fillWidth: true
                Label {
                    text: qsTr("Processors")
                    Layout.preferredWidth: 90
                }
                Slider {
                    id: cpuSlider
                    Layout.fillWidth: true
                    from: 1
                    to: Math.max(1, app.qemuCpusMax)
                    stepSize: 1
                    snapMode: Slider.SnapAlways
                    value: bootTestDialog.cpus
                    onMoved: bootTestDialog.cpus = value
                }
                SpinBox {
                    id: cpuSpin
                    // Size to content (text + spin buttons), with a sane floor.
                    Layout.preferredWidth: Math.max(130, implicitWidth)
                    editable: true
                    from: 1
                    to: Math.max(1, app.qemuCpusMax)
                    stepSize: 1
                    value: bootTestDialog.cpus
                    onValueModified: bootTestDialog.cpus = value
                    textFromValue: function(v) { return v + qsTr(" vCPU") }
                    valueFromText: function(t) { return parseInt(t) }
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
            // Networking.
            CheckBox {
                id: networkCheck
                text: qsTr("Network access (user-mode networking)")
                checked: false
                ToolTip.delay: 500
                ToolTip.visible: hovered
                ToolTip.text: qsTr("Attach a virtual network card with QEMU user-mode networking "
                    + "(no root or bridge needed) so the guest can reach the internet, useful "
                    + "for testing Windows OOBE / activation. Off runs the VM with no network.")
            }
            // Guest audio.
            CheckBox {
                id: audioCheck
                text: qsTr("Guest audio (Intel HD Audio)")
                checked: false
                ToolTip.delay: 500
                ToolTip.visible: hovered
                ToolTip.text: qsTr("Attach an emulated Intel HD Audio device routed to your host's "
                    + "PipeWire/PulseAudio, so the guest can play sound. Off runs the VM silently.")
            }
            // Snapshot mode. On (default) discards all writes; off persists them
            // so a multi-reboot flow like Windows OOBE can run to completion.
            CheckBox {
                id: snapshotCheck
                text: qsTr("Snapshot mode (discard writes, device not modified)")
                checked: true
                ToolTip.delay: 500
                ToolTip.visible: hovered
                ToolTip.text: qsTr("On: every write goes to a throwaway overlay and the real device "
                    + "is never touched. Off: writes persist to the device, needed to run Windows "
                    + "OOBE to completion across its reboots (and to keep the logs it writes), but it "
                    + "WILL modify the drive.")
            }
            Label {
                visible: !snapshotCheck.checked
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                text: qsTr("⚠ Snapshot is off: this boot test will write to and modify the real device.")
                color: Qt.tint(host.palette.windowText, Qt.rgba(0.86, 0.21, 0.27, 0.85))
                font.bold: true
                font.pointSize: 8
            }
            Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                text: snapshotCheck.checked
                    ? qsTr("Opens in snapshot mode, so nothing is written back to the device. "
                        + "Admin rights are required to read the raw device.")
                    : qsTr("Admin rights are required to read the raw device.")
                color: palette.placeholderText
                font.pointSize: 8
            }
        }
    }
