//! The translucent mover scene of the reactive mask measurement. It is the disocclusion scene
//! (checkerboard, grazing stripe plane) with its near bar replaced by a translucent bar: inside the
//! bar rectangle the colour is the opaque scene mixed toward `TINT` by `ALPHA`. The bar writes the
//! depth and the zero motion of the opaque surface behind it, which is what a translucent draw
//! that writes neither depth nor velocity leaves in the targets. Old colour therefore stays in the
//! history where the bar has been.
use crate::temporal_disocclusion_scene as opaque;

/// Colour the bar mixes toward, outside the checkerboard range 0.3 to 0.7.
pub(crate) const TINT: f64 = 1.0;
/// Share of `TINT` in the bar's pixels.
pub(crate) const ALPHA: f64 = 0.7;

/// Scene-referred value at canvas position `p` with the bar's left edge at `left`.
pub(crate) fn value(p: [f64; 2], left: f64) -> f64 {
    let behind = opaque::value(p, f64::NEG_INFINITY);
    let inside = (0..2).all(|i| {
        let origin = [left, opaque::BAR_TOP][i];
        p[i] >= origin && p[i] < origin + opaque::BAR_SIZE[i]
    });
    if inside {
        behind + ALPHA * (TINT - behind)
    } else {
        behind
    }
}

/// The opaque scene's WGSL with its bar moved off the canvas (the uniform's `bar.x` is passed as
/// `OFF_CANVAS`) and the translucent bar drawn over it at `bar.z`.
pub(crate) fn wgsl() -> String {
    let source = opaque::wgsl();
    let entry = "@fragment fn paint(@builtin(position) frag: vec4<f32>) -> Out {";
    assert!(
        source.contains(entry),
        "the opaque scene's entry point changed"
    );
    let source = source.replace(entry, "fn paint_opaque(frag: vec4<f32>) -> Out {");
    format!(
        "{source}
@fragment fn paint(@builtin(position) frag: vec4<f32>) -> Out {{
    var o = paint_opaque(frag);
    let p = (frag.xy - frame.jitter) / frame.render * vec2<f32>({w:?}, {h:?});
    if within(p, vec2<f32>(frame.bar.z, {top:?}), vec2<f32>({bw:?}, {bh:?})) {{
        o.color = vec4<f32>(mix(o.color.rgb, vec3<f32>({TINT:?}), {ALPHA:?}), 1.0);
    }}
    return o;
}}
",
        w = opaque::CANVAS[0],
        h = opaque::CANVAS[1],
        top = opaque::BAR_TOP,
        bw = opaque::BAR_SIZE[0],
        bh = opaque::BAR_SIZE[1],
    )
}

/// The `bar.x` value that keeps the opaque scene's own bar off the canvas.
pub(crate) const OFF_CANVAS: f32 = 1.0e6;
