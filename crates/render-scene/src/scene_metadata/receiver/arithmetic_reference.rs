//! Test-only zero-bias arithmetic defect with the production geometry displacement held fixed.
use super::ReceiverLayout;

pub(super) fn capture(source: &str, layout: ReceiverLayout, bias: f32) -> String {
    let mut source = source.to_owned();
    if bias == 0.0 {
        for side in 0..layout.sides() {
            let normal = if side == 0 { "normal" } else { "-normal" };
            for axis in ['x', 'y'] {
                let position = if source.contains("input: TerrainVertexOutput") {
                    format!("scene_receiver_d{axis}")
                } else {
                    format!("dpd{axis}(position)")
                };
                let original = format!("let d{axis}_{side} = {position};");
                assert_eq!(source.matches(&original).count(), 1);
                source = source.replace(
                    &original,
                    &format!("let d{axis}_{side} = {position} + dpd{axis}({normal}) * bitcast<f32>({}u);", bias.to_bits()),
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

// Hardware terrain displacement is a sensitivity control, never a production receiver.
pub(super) fn hardware_terrain(source: &str) -> String {
    if !source.contains("fn scene_prepare_terrain_receiver(") {
        return source.to_owned();
    }
    let mut source = source.to_owned();
    for axis in ['x', 'y'] {
        for (analytic, hardware) in [
            (
                format!("select(bitcast<vec3<u32>>(scene_receiver_d{axis} +"),
                format!("select(bitcast<vec3<u32>>(dpd{axis}(position) +"),
            ),
            (
                format!(", bitcast<vec3<u32>>(scene_receiver_d{axis}), shadow.bias.y"),
                format!(", bitcast<vec3<u32>>(dpd{axis}(position)), shadow.bias.y"),
            ),
        ] {
            assert_eq!(source.matches(&analytic).count(), 2);
            source = source.replace(&analytic, &hardware);
        }
    }
    source
}
