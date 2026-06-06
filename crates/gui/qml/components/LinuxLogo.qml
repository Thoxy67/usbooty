import QtQuick

// Reusable Linux mascot (a simplified Tux), the Linux counterpart to
// WindowsLogo. Drawn with a Canvas (proportions scale to `size`) so it
// needs no bundled asset. Tux is multi-colour by nature, so unlike the
// monochrome WindowsLogo it ignores any tint.
Canvas {
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
