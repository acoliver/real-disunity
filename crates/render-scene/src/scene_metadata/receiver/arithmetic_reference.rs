//! Test-only arithmetic from the parent implementation, including its zero-bias defect.
use super::ReceiverLayout;

pub(super) fn capture(source: &str, layout: ReceiverLayout, bias: f32) -> String {
    let mut source = source.to_owned();
    if bias == 0.0 {
        for side in 0..layout.sides() {
            let normal = if side == 0 { "normal" } else { "-normal" };
            for axis in ['x', 'y'] {
                source = source.replace(
                    &format!("let d{axis}_{side} = dpd{axis}(position);"),
                    &format!("let d{axis}_{side} = dpd{axis}(position) + dpd{axis}({normal}) * bitcast<f32>({}u);", bias.to_bits()),
                );
            }
        }
    }
    source
}

pub(crate) fn positions(source: &str) -> String {
    source.replace(
        "select(position + normal * shadow.bias.y, position, shadow.bias.y == 0.0)",
        "position + normal * shadow.bias.y",
    )
}
