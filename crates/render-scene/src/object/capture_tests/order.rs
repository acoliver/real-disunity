//! Grouped and walked draws write the same bytes.
//!
//! A pass records its draws grouped by pipeline and material instead of as the selection walk
//! names them. Every scene here is drawn both ways from camera poses that move between frames,
//! and the colour attachments and depth plane of the two are compared byte for byte. The
//! scenes keep their cards at different depths, so no two draws tie at one depth, where the
//! order would decide the winner. The cascades are compared in `casters`.

use super::{Rig, Target, render, scenes};
use crate::CameraPose;
use crate::dynamic::capture_tests::{CONFIGS, build, context};
use crate::object::prepared::DrawOrder;
use worldrunner_render_plan::RenderScene;

/// Five poses 1.5 m apart along x, each turned a little further, with the camera backing off.
fn poses() -> impl Iterator<Item = CameraPose> {
    (0..5).map(|step| CameraPose {
        eye_millimeters: [-3_000 + 1_500 * step, -5_000 - 250 * step, 1_100],
        target_millimeters: [-600 * step, 0, 1_100],
    })
}

fn scene_list() -> Vec<(&'static str, RenderScene)> {
    vec![
        ("grid", scenes::grid(3, 5)),
        ("surfaces", scenes::scene(&scenes::surface_specs())),
        ("impostors", scenes::scene(&scenes::impostor_specs(false))),
    ]
}

#[test]
fn grouped_and_walked_draws_write_the_same_attachments_as_the_camera_moves() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    // Plain one-sample HDR and the velocity pass, whose pipelines read a previous-instance
    // stream in a third vertex slot.
    for config in [2, 4] {
        let built = build(context.device(), &CONFIGS[config]);
        let target = Target {
            formats: &built.formats,
            samples: 1,
        };
        for (name, scene) in scene_list() {
            let mut rig = Rig::new(&context, &built.pipelines, &scene);
            let mut previous = None;
            for pose in poses() {
                rig.frame = scenes::frame(pose);
                rig.gpu.set_frame(context.queue(), rig.frame);
                let mut drawn = |order| {
                    rig.gpu.set_draw_order(order);
                    rig.prime();
                    render(&context, target, &mut rig.gpu, |pass, gpu| {
                        gpu.draw_color(pass, &built.pipelines, None);
                    })
                };
                let walked = drawn(DrawOrder::Walk);
                let grouped = drawn(DrawOrder::Grouped);
                assert_eq!(
                    grouped, walked,
                    "{} {name}: the grouped draws wrote other bytes than the walk's",
                    CONFIGS[config].name
                );
                assert_ne!(
                    previous.replace(grouped.clone()),
                    Some(grouped),
                    "{} {name}: the camera moved and the frame did not change",
                    CONFIGS[config].name
                );
            }
        }
    }
}

#[test]
fn the_grid_draws_every_card_and_depth_decides_where_they_overlap() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let built = build(context.device(), &CONFIGS[2]);
    let target = Target {
        formats: &built.formats,
        samples: 1,
    };
    let mut rig = Rig::new(&context, &built.pipelines, &scenes::grid(3, 5));
    let drawn = render(&context, target, &mut rig.gpu, |pass, gpu| {
        gpu.draw_color(pass, &built.pipelines, None);
    });
    let untouched = render(&context, target, &mut rig.gpu, |_, _| {});
    let (colour, depth) = (&drawn[0], drawn.last().unwrap());
    assert_ne!(colour, &untouched[0], "the cards drew");
    // Fifteen cards in three rows at three depths leave several distinct depths in the plane.
    let mut depths: Vec<_> = depth.chunks_exact(4).map(<[u8]>::to_vec).collect();
    depths.sort();
    depths.dedup();
    assert!(depths.len() >= 4, "the cards left {} depths", depths.len());
}
