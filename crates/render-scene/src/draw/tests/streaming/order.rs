use super::*;
use crate::draw::streaming::schedule_tests;
use crate::terrain::streaming::TerrainChunk;

const SIZE: u32 = 128;

fn source() -> RenderScene {
    let base = terrain_scene(1);
    base.with_terrain(Some(std::sync::Arc::new(schedule_tests::terrain())))
        .unwrap()
        .with_objects_and_effects(Vec::new(), base.effects().cloned())
        .unwrap()
}

fn frame() -> FrameUniform {
    let mut frame = frame_from(CameraPose {
        eye_millimeters: [32_000, -8_000, 8_000],
        target_millimeters: [32_000, 32_000, 0],
    });
    frame.view_projection = schedule_tests::matrix();
    frame
}

fn settle(context: &GpuContext, gpu: &SceneGpu, delay: usize) -> Vec<usize> {
    let initial = context.submission().completed_serial();
    let mut serials = Vec::new();
    let mut uploads = Vec::new();
    for iteration in 0..256_usize {
        let completed = iteration
            .checked_sub(delay + 1)
            .map_or(initial, |index| serials[index]);
        let prepared = context
            .submission()
            .prepare(context.device(), |encoder, _| {
                let mut upload =
                    worldrunner_render_post::PrepareContext::new(context.device(), encoder)?;
                streaming(gpu).prepare_completed(&mut upload, &frame(), [SIZE; 2], completed)?;
                upload.finish();
                Ok::<_, worldrunner_render_post::FrameError>(())
            })
            .unwrap();
        let (resident, pending, retired) = streaming(gpu).allocation_counts();
        assert!(resident + pending + retired <= 41);
        uploads.push(pending);
        prepared.submit().unwrap();
        serials.push(context.submission().submitted_serial());
        wait(context);
        if pending == 0 && retired == 0 {
            return uploads;
        }
    }
    panic!("terrain did not settle");
}

fn keys(gpu: &SceneGpu) -> Vec<TerrainChunk> {
    streaming(gpu)
        .committed
        .borrow()
        .iter()
        .map(|(key, _)| *key)
        .collect()
}

#[test]
fn terrain_pixels_are_identical_after_different_completion_schedules() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let pipelines = ScenePipelines::new(context.device(), COLOR_FORMAT);
    let source = source();
    let immediate =
        SceneGpu::upload(context.device(), context.submission(), &source, &pipelines).unwrap();
    let delayed =
        SceneGpu::upload(context.device(), context.submission(), &source, &pipelines).unwrap();
    let first = settle(&context, &immediate, 0);
    let second = settle(&context, &delayed, 4);
    assert_ne!(first, second, "completion schedules must change admission");
    assert_eq!(keys(&immediate), keys(&delayed));
    assert!(keys(&immediate).is_sorted());
    assert_eq!(keys(&immediate).len(), 40);
    let mut target = OffscreenTarget::new(
        context.device(),
        Viewport {
            width: SIZE,
            height: SIZE,
        },
    )
    .unwrap();
    immediate.set_frame(context.queue(), frame());
    let first = target.render(&context, &immediate, &pipelines).unwrap();
    delayed.set_frame(context.queue(), frame());
    let second = target.render(&context, &delayed, &pipelines).unwrap();
    assert!(
        lit_pixels(&first.pixels) > 0,
        "terrain must reach the target"
    );
    assert_eq!(first, second);
    assert_eq!(immediate.statistics(), delayed.statistics());
    let Some(crate::draw::TerrainGeometryStatistics::Streaming(stats)) =
        immediate.statistics().terrain_geometry
    else {
        panic!("submitted streaming terrain statistics");
    };
    assert!(stats.complete);
    assert_eq!(stats.resident_leaves, 40);
    assert_eq!(immediate.statistics().passes.terrain.calls, 40);
    wait(&context);
}
