//! Claims about single features of the object family, each compared with the fixed pipeline:
//! alpha to coverage, relief depth and the previous-instance stream.

use super::scenes::{self, Alpha, Spec};
use super::{Captured, Case, Legacy, Rig, Target, capture, frozen_source, recipe};
use crate::dynamic::capture_tests::{CONFIGS, VELOCITY, build, context};
use crate::renderer::legacy_objects::Recipe;

/// Captures the cards of `specs` in the target set `index` of the forward sets, drawing through
/// the scene's own entry point.
fn one(index: usize, specs: &[Spec], moving: bool) -> Captured {
    let context = context();
    let config = &CONFIGS[index];
    let built = build(context.device(), config);
    let source = frozen_source(&built.pipelines, recipe(config.kind));
    let mut rig = Rig::moving(&context, &built.pipelines, &scenes::scene(specs), moving);
    let legacy = Legacy::new(&context, &rig.selected(&built.pipelines), &source);
    capture(
        &mut rig,
        &legacy,
        &Case {
            name: config.name,
            scene: "variant",
            target: Target {
                formats: &built.formats,
                samples: config.samples,
            },
            pipelines: &built.pipelines,
            source: &source,
            screen: built.screen.as_ref(),
            shadow: None,
            mask: 0,
        },
    )
}

/// Three cutout cards, with or without coverage.
fn cutouts(coverage: bool) -> Vec<Spec> {
    vec![
        Spec {
            alpha: if coverage {
                Alpha::Coverage
            } else {
                Alpha::Cutout
            },
            ..Spec::default()
        };
        3
    ]
}

const HDR_1X: usize = 2;
const HDR_4X: usize = 3;
const VELOCITY_1X: usize = 4;
const VELOCITY_4X: usize = 5;

/// Coverage turns the alpha of a cutout card into covered samples, so at four samples the card's
/// edge resolves to a partial colour that the hard cutoff cannot make. At one sample coverage is
/// off, so a card that asks for it draws the cutoff's pixels. Each is also what the fixed
/// pipeline draws.
#[test]
fn masked_multisample_coverage_matches_the_fixed_pipeline_at_four_samples() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let covered = one(HDR_4X, &cutouts(true), true);
    let cutoff = one(HDR_4X, &cutouts(false), true);
    assert!(
        covered.images[0] != cutoff.images[0],
        "coverage changed no resolved sample at four samples"
    );
    let single_covered = one(HDR_1X, &cutouts(true), true);
    let single_cutoff = one(HDR_1X, &cutouts(false), true);
    assert!(
        single_covered.images == single_cutoff.images,
        "coverage must not change a single-sample draw"
    );
}

/// Relief writes the depth of the surface it finds in the card's depth map, not the card's own,
/// so a relief impostor's depth plane differs from a flat impostor's. The depth plane is what
/// the fixed pipeline wrote, byte for byte, and it varies across the card.
#[test]
fn relief_depth_output_matches_the_fixed_pipeline() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let relief = one(HDR_1X, &scenes::impostor_specs(true), true);
    let flat = one(HDR_1X, &scenes::impostor_specs(false), true);
    let depth = |captured: &Captured| captured.images.last().expect("a depth plane").clone();
    let (relief, flat) = (depth(&relief), depth(&flat));
    assert!(relief != flat, "relief wrote the card's own depth");
    let texels: std::collections::BTreeSet<_> = relief.chunks_exact(4).collect();
    assert!(
        texels.len() > 4,
        "relief depth is nearly constant: {}",
        texels.len()
    );
}

/// A velocity pipeline reads each instance's previous transform from its third vertex buffer, so
/// moving the cards after the previous transforms were recorded changes the velocity target and
/// leaving them still does not. Both are what the fixed pipeline wrote.
#[test]
fn the_previous_instance_stream_decides_the_velocity_target_like_the_fixed_pipeline() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let velocity = |captured: &Captured, index: usize| captured.images[index].clone();
    for index in [VELOCITY_1X, VELOCITY_4X] {
        let formats = &CONFIGS[index].name;
        let slot = build(context().device(), &CONFIGS[index])
            .formats
            .iter()
            .position(|&format| format == VELOCITY)
            .expect("a velocity target");
        let moving = one(index, &scenes::surface_specs(), true);
        let still = one(index, &scenes::surface_specs(), false);
        assert!(
            velocity(&moving, slot) != velocity(&still, slot),
            "{formats}: moving the cards left the velocity target alone"
        );
    }
}

#[test]
fn a_recipe_with_velocity_declares_the_previous_instance_stream() {
    let source = crate::renderer::legacy_objects::compose(Recipe {
        velocity: true,
        ..Recipe::default()
    });
    assert!(source.contains(crate::velocity::MARKER));
    assert!(
        !crate::renderer::legacy_objects::compose(Recipe::default())
            .contains(crate::velocity::MARKER)
    );
}
