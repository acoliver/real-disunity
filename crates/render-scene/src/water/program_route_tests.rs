//! The body draw's use of the program runtime: which frames it accepts, and how much it builds.
//!
//! The captures in `capture_tests` establish that the program route writes the bytes the frozen
//! fixed path writes. These tests establish what the captures cannot see: that a feature refuses
//! a frame of another submission authority, that each body prepares exactly one draw per
//! feature, and that programs, modules and pipelines are built once per key and reused by later
//! features.

use super::*;
use worldrunner_render_gpu::submission::{LeaseError, SubmissionAuthority};
use worldrunner_render_program::error::ProgramError;

fn installed(
    context: &GpuContext,
    bodies: Vec<SceneWaterBody>,
) -> crate::compositor::FrameRenderer {
    let mut renderer = regressions::renderer(context, 1);
    renderer
        .install_water_bodies(context.device(), context.submission(), bodies)
        .unwrap();
    renderer
}

fn frame() -> FrameUniform {
    FrameUniform::new(&fixture_view(), None).unwrap()
}

/// A second authority over the same queue hands out frames the water runtime must refuse: the
/// queue is shared, but the exclusion and the retirement of reservations are the authority's.
#[test]
fn a_feature_refuses_a_frame_of_another_authority_over_the_same_queue() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = GpuContext::shared_for_test().unwrap();
    let renderer = installed(&context, vec![settings()]);
    let water = renderer.water.as_ref().unwrap();
    let other = SubmissionAuthority::new(context.queue().clone());
    assert!(!other.same_authority(context.submission()));
    let foreign = other.begin(context.device()).unwrap();
    let refused = water.feature_with_depth(
        (context.device(), &*foreign),
        &frame(),
        (0.0, 1),
        (None, None),
        DepthMode::Independent,
    );
    assert!(
        matches!(
            refused,
            Err(ProgramError::Lease(LeaseError::ForeignAuthority))
        ),
        "a frame of another authority is refused before anything is recorded: {:?}",
        refused.as_ref().err()
    );
    drop(foreign);
    let own = context.submission().begin(context.device()).unwrap();
    assert!(
        water
            .feature_with_depth(
                (context.device(), &*own),
                &frame(),
                (0.0, 1),
                (None, None),
                DepthMode::Independent,
            )
            .is_ok(),
        "the installing authority's own frame is accepted"
    );
}

/// One draw for each body, and no program, module or pipeline built again for a key a feature
/// has already drawn with.
#[test]
fn each_feature_prepares_one_draw_per_body_and_reuses_built_programs() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = GpuContext::shared_for_test().unwrap();
    let mut second_body = settings();
    second_body.east_min_millimeters = 20_000;
    let renderer = installed(&context, vec![settings(), second_body]);
    let water = renderer.water.as_ref().unwrap();
    assert!(water.bodies.iter().all(|body| body.vertices != 0));
    let stats = || water.programs.borrow().runtime().stats();
    let before = stats();
    let encoder = context.submission().begin(context.device()).unwrap();
    let first = water.feature(
        (context.device(), &*encoder),
        &frame(),
        (0.0, 1),
        (None, None),
    );
    let after_first = stats();
    assert_eq!(
        first.draws.iter().map(Option::is_some).collect::<Vec<_>>(),
        [true, true]
    );
    assert_eq!(after_first.draws_prepared - before.draws_prepared, 2);
    assert_eq!(
        water.programs.borrow().built(),
        1,
        "both bodies share one key"
    );
    assert_eq!(after_first.modules_built - before.modules_built, 1);
    assert_eq!(after_first.pipelines_built - before.pipelines_built, 1);
    let second = water.feature(
        (context.device(), &*encoder),
        &frame(),
        (0.0, 2),
        (None, None),
    );
    let after_second = stats();
    assert_eq!(after_second.draws_prepared - after_first.draws_prepared, 2);
    assert_eq!(after_second.modules_built, after_first.modules_built);
    assert_eq!(after_second.pipelines_built, after_first.pipelines_built);
    assert_eq!(
        after_second.sets_prepared, before.sets_prepared,
        "water prepares no reusable sets: each draw builds its own group from imports"
    );
    drop((first, second));
}

/// The vertex stage carries `WATER_TERMS` when the key's term bits need it there (wind 128 and
/// vertex seeds 512) and nothing otherwise, and the fragment stage always carries the specular
/// options. No capture has wind, the only bit the vertex stage reads, so only this test sees the
/// vertex overrides.
#[test]
fn programs_carry_the_override_constants_of_both_stages() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = GpuContext::shared_for_test().unwrap();
    let renderer = installed(&context, vec![settings()]);
    let water = renderer.water.as_ref().unwrap();
    let mut programs = water.programs.borrow_mut();
    // The runtime holds overrides ordered by name, so both sides are compared in that order.
    let held = |overrides: &[(String, f64)]| {
        let mut sorted: Vec<_> = overrides
            .iter()
            .map(|(name, value)| (name.clone(), value.to_bits()))
            .collect();
        sorted.sort();
        sorted
    };
    let expected = |constants: Vec<(&str, f64)>| {
        let mut sorted: Vec<_> = constants
            .into_iter()
            .map(|(name, value)| (name.to_owned(), value.to_bits()))
            .collect();
        sorted.sort();
        sorted
    };
    for terms in [0, 19, 128, 128 | 64, 512 | 64] {
        let options = [1, 0, 1, 0, terms];
        let program = programs.program((options, false)).unwrap();
        assert_eq!(
            held(program.vertex().overrides()),
            expected(super::super::terms::vertex_constants(options)),
            "vertex overrides for term bits {terms}"
        );
        assert_eq!(
            held(program.fragment().unwrap().overrides()),
            expected(super::super::terms::constants(options)),
            "fragment overrides for term bits {terms}"
        );
        assert_eq!(
            program.vertex().overrides().is_empty(),
            terms & (128 | 512) == 0,
            "WATER_TERMS reaches the vertex stage exactly when wind or vertex seeds ask for it"
        );
    }
}
