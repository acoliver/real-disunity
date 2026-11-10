//! Terrain lighting follows the full-resolution heightfield, whatever triangles draw it.

use super::{Case, Rig, Source, Target, capture, scenes};
use crate::dynamic::capture_tests::{CONFIGS, build, context, supported};
use worldrunner_render_plan::TerrainLodPolicy;

/// Mean absolute difference per colour channel, in 8-bit levels, over every pixel.
fn mean_difference(left: &[u8], right: &[u8]) -> f64 {
    let total: u32 = left
        .chunks_exact(4)
        .zip(right.chunks_exact(4))
        .flat_map(|(a, b)| (0..3).map(move |channel| u32::from(a[channel].abs_diff(b[channel]))))
        .sum();
    let channels = u32::try_from(left.len() / 4 * 3).expect("a capture-sized image");
    f64::from(total) / f64::from(channels)
}

#[test]
fn a_reduced_terrain_mesh_is_lit_like_the_full_resolution_one() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let config = &CONFIGS[0];
    assert!(supported(&context, config));
    let built = build(context.device(), config);
    let frame = scenes::frame(scenes::pose());
    let image = |scene, name| {
        let rig = Rig::new(&context, &built.pipelines, &scene, (Source::Plain, frame));
        let case = Case {
            name,
            source: Source::Plain.name(),
            target: Target {
                formats: &built.formats,
                samples: config.samples,
            },
            pipelines: &built.pipelines,
            screen: built.screen.as_ref(),
            shadow: None,
        };
        capture(&rig, &case).images.swap_remove(0)
    };
    // Both meshes draw the same surface, so only the lighting normal can tell them apart.
    let full = image(
        scenes::creased(TerrainLodPolicy::FullResolution),
        "pixel-normal-full",
    );
    let reduced = image(
        scenes::creased(TerrainLodPolicy::UniformSteps { steps: 1 }),
        "pixel-normal-reduced",
    );
    let difference = mean_difference(&full, &reduced);
    println!("TERRAIN-PIXEL-NORMAL mean difference {difference:.3} levels");
    assert!(
        difference < LIMIT,
        "a mesh at half resolution changed the lit terrain by {difference:.3} levels"
    );
}

/// Radeon (RADV): 0.008 levels with the heightfield normal and 1.201 with the interpolated
/// vertex normal, which lights the halved mesh across two-metre triangles.
const LIMIT: f64 = 0.1;
