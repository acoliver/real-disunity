//! Byte-level captures of dynamic quads that carry clip rectangles.
//!
//! The clip rectangle of a quad splits draws in both places that stage quads, and only the
//! retained-UI overlay scissors to it. So the scene's pass draws every quad whole: it has to
//! change the colour attachment, whatever the rectangles, and leave the attachments the material
//! does not write as they were.

use super::*;

const FULL: [u32; 4] = [0, 0, SIZE, SIZE];

/// Clip rectangles in lower-left pixels, per quad index of [`quads`].
struct Case {
    name: &'static str,
    quads: Vec<DynamicQuad>,
}

fn clipped(clip: impl Fn(usize) -> Option<[u32; 4]>) -> Vec<DynamicQuad> {
    quads()
        .into_iter()
        .enumerate()
        .map(|(index, quad)| DynamicQuad {
            clip_rect: clip(index),
            ..quad
        })
        .collect()
}

fn cases() -> Vec<Case> {
    let case = |name, quads| Case { name, quads };
    vec![
        case(
            "partial",
            clipped(|index| (index != 2).then_some([6, 6, 16, 20])),
        ),
        case(
            "disjoint",
            clipped(|index| {
                Some(if index % 2 == 0 {
                    [0, 0, 14, SIZE]
                } else {
                    [18, 0, 14, SIZE]
                })
            }),
        ),
        case("full", clipped(|_| Some(FULL))),
        case("oversized", clipped(|_| Some([8, 8, 100, 100]))),
        case("outside", clipped(|_| Some([40, 0, 8, 8]))),
        case(
            "mixed",
            clipped(|index| {
                Some(if index % 2 == 0 {
                    FULL
                } else {
                    [SIZE, 4, 4, 4]
                })
            }),
        ),
    ]
}

#[test]
fn clipped_dynamic_quads_draw_in_the_scene_pass_in_every_target_set() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let textures = textures();
    let context = context();
    for config in &CONFIGS {
        if !supported(&context, config) {
            println!(
                "UNLIT-CLIP-CAPTURE config={} skipped: format or samples unsupported",
                config.name
            );
            continue;
        }
        for case in cases() {
            let drawn = capture(&context, config, &textures, &case.quads);
            let adapter = context.adapter().get_info().name;
            for (index, image) in drawn.iter().enumerate() {
                println!(
                    "UNLIT-CLIP-CAPTURE adapter={adapter:?} config={} path=scene case={} attachment={index} bytes={} hash={:016x}",
                    config.name,
                    case.name,
                    image.len(),
                    digest(image)
                );
            }
        }
    }
}
