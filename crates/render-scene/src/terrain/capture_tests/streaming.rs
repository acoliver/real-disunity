//! Streamed terrain: one draw per resident chunk, over residency that grows, shrinks and has
//! gaps, in several target sets.

use wgpu::CommandEncoder;

use super::{Case, Rig, Source, Target, capture, scenes};
use crate::device::GpuContext;
use crate::draw::SceneGpu;
use crate::dynamic::capture_tests::{Built, CONFIGS, Config, SIZE, build, context, supported};
use crate::frame::FrameUniform;

/// Prepares the streamed chunks the frame selects in `rounds` submitted frames, which commit
/// them.
pub(in crate::terrain) fn settle(
    context: &GpuContext,
    gpu: &SceneGpu,
    frame: &FrameUniform,
    rounds: usize,
) {
    let (device, authority) = (context.device(), context.submission());
    for _ in 0..rounds {
        let mut transaction = authority.begin(device).unwrap();
        let encoder: &mut CommandEncoder = &mut transaction;
        gpu.prepare_terrain_geometry(device, encoder, frame, [SIZE; 2])
            .unwrap();
        transaction.finish().submit().unwrap();
        device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: Some(std::time::Duration::from_secs(30)),
            })
            .unwrap();
    }
}

/// The draw calls of one capture of the committed chunks.
fn calls(rig: &Rig<'_>, (config, built): (&Config, &Built), label: &str) -> u64 {
    let captured = capture(
        rig,
        &Case {
            name: label,
            source: Source::Plain.name(),
            target: Target {
                formats: &built.formats,
                samples: config.samples,
            },
            pipelines: &built.pipelines,
            screen: built.screen.as_ref(),
            shadow: None,
        },
    );
    captured.scene.calls
}

#[test]
fn streamed_terrain_matches_the_fixed_pipeline_over_changing_residency() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let config = &CONFIGS[2];
    assert!(supported(&context, config));
    let built = build(context.device(), config);
    let near = scenes::frame(scenes::pose());
    let far = scenes::frame(crate::CameraPose {
        eye_millimeters: [0, -14_000, 16_000],
        target_millimeters: [0, 0, 0],
    });
    let aside = scenes::frame(crate::CameraPose {
        eye_millimeters: [3_000, -2_000, 2_500],
        target_millimeters: [3_000, 3_000, 0],
    });
    let rig = Rig::new(
        &context,
        &built.pipelines,
        &scenes::streamed(9),
        (Source::Plain, near),
    );
    let mut seen = Vec::new();
    // Only the root chunk is resident before anything is prepared.
    seen.push(calls(&rig, (config, &built), "stream-root"));
    settle(&context, &rig.gpu, &near, 1);
    seen.push(calls(&rig, (config, &built), "stream-first-upload"));
    settle(&context, &rig.gpu, &near, 6);
    seen.push(calls(&rig, (config, &built), "stream-converged"));
    // A camera elsewhere retires some chunks and uploads others, leaving different gaps.
    settle(&context, &rig.gpu, &aside, 3);
    seen.push(calls(&rig, (config, &built), "stream-aside"));
    settle(&context, &rig.gpu, &far, 6);
    seen.push(calls(&rig, (config, &built), "stream-far"));
    println!("TERRAIN-CAPTURE-SUMMARY set=streaming chunks={seen:?}");
    assert_eq!(seen[0], 1, "the root chunk alone");
    assert!(
        seen.iter().any(|&chunks| chunks > 1),
        "no residency state drew more than one chunk: {seen:?}"
    );
    assert!(
        seen.windows(2).any(|pair| pair[0] != pair[1]),
        "residency never changed: {seen:?}"
    );
}

#[test]
fn streamed_terrain_matches_the_fixed_pipeline_in_every_target_set() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let near = scenes::frame(scenes::pose());
    let mut run = 0;
    for config in &CONFIGS {
        if !supported(&context, config) {
            continue;
        }
        let built = build(context.device(), config);
        let rig = Rig::new(
            &context,
            &built.pipelines,
            &scenes::streamed(9),
            (Source::Plain, near),
        );
        settle(&context, &rig.gpu, &near, 6);
        let chunks = calls(&rig, (config, &built), &format!("stream-{}", config.name));
        assert!(chunks > 1, "{}: {chunks} chunks", config.name);
        run += 1;
    }
    println!("TERRAIN-CAPTURE-SUMMARY set=streaming-targets compared={run}");
    assert!(run > 0);
}

#[test]
fn a_tight_chunk_budget_draws_fewer_chunks_than_a_loose_one_and_both_match_the_fixed_pipeline() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let config = &CONFIGS[2];
    let built = build(context.device(), config);
    let near = scenes::frame(scenes::pose());
    let mut drawn = Vec::new();
    for budget in [5, 9] {
        let rig = Rig::new(
            &context,
            &built.pipelines,
            &scenes::streamed(budget),
            (Source::Plain, near),
        );
        settle(&context, &rig.gpu, &near, 8);
        drawn.push(calls(
            &rig,
            (config, &built),
            &format!("stream-budget{budget}"),
        ));
    }
    println!("TERRAIN-CAPTURE-SUMMARY set=streaming-budget chunks={drawn:?}");
    assert!(drawn[0] < drawn[1], "{drawn:?}");
}
