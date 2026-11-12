use super::ReceiverLayout;

/// Both relief entry points open with this text. The reject goes after the impostor guard, so
/// the body still begins with the relief depth call that the relief dependency check expects.
const ENTRY: &str = "    let depth = scene_relief_depth(input);\n    if SCENE_IMPOSTOR {";
const REJECT: &str = "\n        scene_receiver_pixel = vec2<i32>(input.clip_position.xy);\n        if scene_receiver_hidden(depth.value) {\n            discard;\n        }";

/// The object consumer source whose impostor cards discard whole quads that the read-only
/// Opaque depth test would refuse anyway.
pub(super) fn card_reject_source(source: &str, layout: ReceiverLayout) -> String {
    let consumer = super::shader::consumer(source, layout, false);
    format!(
        "{}\n{}",
        insert(&consumer),
        include_str!("card_reject.wgsl")
    )
}

fn insert(source: &str) -> String {
    assert_eq!(
        source.matches(ENTRY).count(),
        2,
        "the opaque and masked relief entry points each open with the relief depth call",
    );
    source.replace(ENTRY, &format!("{ENTRY}{REJECT}"))
}

#[cfg(test)]
mod tests {
    use super::{ReceiverLayout, card_reject_source};

    #[test]
    fn the_reject_enters_both_relief_entry_points_once() {
        for layout in [ReceiverLayout::Front, ReceiverLayout::Opposing] {
            for reflection in [false, true] {
                for velocity in [false, true] {
                    let (object, _) = crate::shadows::receiver_sources(true, reflection, velocity);
                    let source = card_reject_source(&object, layout);
                    assert_eq!(
                        source.matches("scene_receiver_hidden(depth.value)").count(),
                        2
                    );
                    assert!(source.contains("clamp(value, 0.0, 1.0) > stored"));
                    assert!(source.contains("@group(2) @binding(5) var receiver_scene_depth"));
                }
            }
        }
    }
}
