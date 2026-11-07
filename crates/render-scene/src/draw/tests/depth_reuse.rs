//! Resolve-free byte comparison against the forward path before depth ownership.
use super::*;
use crate::renderer::depth::DepthMode;
#[path = "depth_reuse_capture.rs"]
mod capture;
#[path = "depth_reuse_fixtures.rs"]
mod fixtures;

#[test]
fn gpu_forward_depth_reuse_matches_frozen_all_attachment_bytes() {
    let context = context();
    let mut covered = 0;
    let mut moving = 0;
    for (name, scene, pose) in
        scenes()
            .into_iter()
            .chain(fixtures::scenes())
            .filter(|(_, scene, _)| {
                !scene
                    .resources()
                    .materials()
                    .iter()
                    .any(|material| crate::pipeline::PipelineKey::from_material(material).relief)
            })
    {
        for jitter in [0.0, 0.375] {
            let old = capture::frame(&context, (&scene, pose), (false, jitter));
            let new = capture::frame(&context, (&scene, pose), (true, jitter));
            for (index, (old, new)) in old.iter().zip(&new).enumerate() {
                assert_eq!(old, new, "{name}, jitter={jitter}, attachment={index}");
            }
            assert!(
                new[6].chunks_exact(4).any(|p| p != 1.0_f32.to_le_bytes()),
                "{name} must exercise visible coverage"
            );
            covered += new[6]
                .chunks_exact(4)
                .filter(|depth| *depth != 1.0_f32.to_le_bytes())
                .count();
            moving += new[3].iter().filter(|byte| **byte != 0).count();
        }
    }
    assert!(covered > 0, "differential fixtures must cover pixels");
    assert!(moving > 0, "wind fixtures must publish motion residuals");
}

#[test]
fn gpu_relief_depth_reuse_matches_frozen_all_attachment_bytes() {
    let context = context();
    for (name, scene, pose) in super::forward_rejection::fixtures::scenes() {
        for jitter in [0.0, 0.375] {
            let old = capture::frame(&context, (&scene, pose), (false, jitter));
            let new = capture::frame(&context, (&scene, pose), (true, jitter));
            for (index, (old, new)) in old.iter().zip(new).enumerate() {
                assert_eq!(*old, new, "{name}, jitter={jitter}, attachment={index}");
            }
        }
    }
}

#[test]
fn gpu_depth_reuse_preserves_first_tie_and_rejects_clear_depth() {
    let context = context();
    for (name, scene, pose) in fixtures::ties() {
        let transform = || {
            if name == "clear depth one" {
                capture::Transform::ClipSpace
            } else {
                capture::Transform::World
            }
        };
        let old = capture::transformed_frame(&context, (&scene, pose), (false, 0.0), transform());
        let new = capture::transformed_frame(&context, (&scene, pose), (true, 0.0), transform());
        for (index, (old, new)) in old.iter().zip(&new).enumerate() {
            assert_eq!(old, new, "{name}, attachment={index}");
        }
        if name == "clear depth one" {
            assert!(new[6].chunks_exact(4).all(|p| p == 1.0_f32.to_le_bytes()));
            assert!(
                new[..6]
                    .iter()
                    .all(|target| target.iter().all(|byte| *byte == 0))
            );
            let visible = fixtures::depth_boundary(999);
            let control = capture::transformed_frame(
                &context,
                (&visible, pose),
                (true, 0.0),
                capture::Transform::ClipSpace,
            );
            assert!(
                control[6]
                    .chunks_exact(4)
                    .any(|p| p != 1.0_f32.to_le_bytes())
            );
        } else {
            assert!(new[6].chunks_exact(4).any(|p| p != 1.0_f32.to_le_bytes()));
            let winner = fixtures::first_winner(&scene);
            let expected = capture::frame(&context, (&winner, pose), (false, 0.0));
            assert_eq!(
                old, expected,
                "{name}: first API-order surface must win the tie"
            );
        }
    }
}

#[test]
fn gpu_negative_scale_depth_reuse_matches_frozen_all_attachment_bytes() {
    let context = context();
    let scene = scene(SceneAlphaMode::Opaque, true, 1);
    let old = capture::transformed_frame(
        &context,
        (&scene, pose()),
        (false, 0.375),
        capture::Transform::Mirrored,
    );
    let new = capture::transformed_frame(
        &context,
        (&scene, pose()),
        (true, 0.375),
        capture::Transform::Mirrored,
    );
    assert_eq!(old, new);
    assert!(new[6].chunks_exact(4).any(|p| p != 1.0_f32.to_le_bytes()));
}

#[test]
fn gpu_terrain_pages_and_skirts_depth_reuse_matches_frozen_bytes() {
    let context = context();
    for source in [
        terrain_scene(2),
        layered_terrain::layered_scene(worldrunner_render_plan::TerrainSplatFilter::Bilinear),
    ] {
        for cached in [false, true] {
            let transform = || {
                if cached {
                    capture::Transform::CachedTerrain
                } else {
                    capture::Transform::World
                }
            };
            let old = capture::transformed_frame(
                &context,
                (&source, pose()),
                (false, 0.375),
                transform(),
            );
            let new =
                capture::transformed_frame(&context, (&source, pose()), (true, 0.375), transform());
            assert_eq!(old, new, "cached={cached}");
            assert!(new[6].chunks_exact(4).any(|p| p != 1.0_f32.to_le_bytes()));
        }
    }
    let base = terrain_scene(1);
    let terrain = base.terrain().unwrap().clone().with_streaming_policy(Some(
        worldrunner_render_plan::TerrainStreamingPolicy::new(1, 9, 4, 1000).unwrap(),
    ));
    let source = base
        .with_terrain(Some(std::sync::Arc::new(terrain)))
        .unwrap();
    assert_eq!(
        capture::frame(&context, (&source, pose()), (false, 0.0)),
        capture::frame(&context, (&source, pose()), (true, 0.0))
    );
}

#[test]
fn gpu_unlit_quads_keep_forward_only_depth_and_auxiliary_masks() {
    let context = context();
    let source = terrain_scene(1);
    let old = capture::transformed_frame(
        &context,
        (&source, pose()),
        (false, 0.375),
        capture::Transform::Quads,
    );
    let new = capture::transformed_frame(
        &context,
        (&source, pose()),
        (true, 0.375),
        capture::Transform::Quads,
    );
    assert_eq!(old, new);
    let without = capture::frame(&context, (&source, pose()), (false, 0.375));
    assert_ne!(
        old[0], without[0],
        "the forward-only quad must affect colour"
    );
    assert_eq!(
        old[1..],
        without[1..],
        "the quad must not write auxiliary targets or depth"
    );
}

#[path = "depth_reuse_frame.rs"]
mod frame_integration;

#[test]
fn gpu_depth_reuse_distinguishes_one_ulp_depth_separation() {
    let context = context();
    let source = fixtures::tie_scene(false);
    let old = capture::transformed_frame(
        &context,
        (&source, pose()),
        (false, 0.0),
        capture::Transform::OneUlpSeparated,
    );
    let new = capture::transformed_frame(
        &context,
        (&source, pose()),
        (true, 0.0),
        capture::Transform::OneUlpSeparated,
    );
    assert_eq!(old, new);
    assert!(new[6].chunks_exact(4).any(|p| p == 0.5_f32.to_le_bytes()));
}
