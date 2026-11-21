//! Levels that share buffers write the same bytes as levels with buffers of their own.
//!
//! Uploading with a chunk limit of one byte gives every mesh level and card complement a vertex
//! buffer and an index buffer of its own, which is how levels were held before they shared
//! buffers. Each scene is drawn both ways from camera poses that move between frames, and the
//! colour attachments and depth plane are compared byte for byte, in the plain one-sample pass
//! and in the velocity pass whose pipelines read a third vertex slot. The cascades are compared
//! in `casters`.

use super::{Rig, Target, render, scenes};
use crate::CameraPose;
use crate::draw::with_chunk_limit;
use crate::dynamic::capture_tests::{CONFIGS, build, context};
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
fn shared_and_separate_buffers_write_the_same_attachments_as_the_camera_moves() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    for config in [2, 4] {
        let built = build(context.device(), &CONFIGS[config]);
        let target = Target {
            formats: &built.formats,
            samples: 1,
        };
        for (name, scene) in scene_list() {
            let mut shared = Rig::new(&context, &built.pipelines, &scene);
            let mut separate = with_chunk_limit(1, || Rig::new(&context, &built.pipelines, &scene));
            let mut previous = None;
            for pose in poses() {
                let drawn = |rig: &mut Rig<'_>| {
                    rig.frame = scenes::frame(pose);
                    rig.gpu.set_frame(context.queue(), rig.frame);
                    rig.prime();
                    render(&context, target, &mut rig.gpu, |pass, gpu| {
                        gpu.draw_color(pass, &built.pipelines, None);
                    })
                };
                let (a, b) = (drawn(&mut shared), drawn(&mut separate));
                assert_eq!(
                    a, b,
                    "{} {name}: shared buffers wrote other bytes than buffers of their own",
                    CONFIGS[config].name
                );
                assert_ne!(
                    previous.replace(a.clone()),
                    Some(a),
                    "{} {name}: the camera moved and the frame did not change",
                    CONFIGS[config].name
                );
            }
        }
    }
}
