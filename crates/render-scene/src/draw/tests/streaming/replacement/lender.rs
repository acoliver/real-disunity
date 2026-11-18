//! The terrain lender while a retained replacement freezes the previous source.
use super::*;
use crate::draw::TerrainChunkViewError;
use worldrunner_render_gpu::submission::capture::CaptureWrites;
use worldrunner_render_post::{FrameError, PrepareContext};

const VIEWPORT: [u32; 2] = [TARGET; 2];

#[test]
fn continuation_refuses_a_cover_its_prefix_replaced() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let (_pipelines, old, next) = tight_budget_pair(&context);
    let device = context.device();
    let mut keys = old.capture_write_keys(device).unwrap();
    keys.extend(next.capture_write_keys(device).unwrap());
    let writes = CaptureWrites::new(keys, [], []).unwrap();
    let mut prefix_draws = Vec::new();
    wait(&context);
    let prefix = context
        .submission()
        .begin_capture(device, writes, frame_from(pose()), |encoder, _, frame| {
            old.prepare_terrain_geometry(device, encoder, frame, VIEWPORT)?;
            prefix_draws = old
                .prepared_terrain_chunks(frame, VIEWPORT)
                .unwrap()
                .unwrap();
            let mut upload = PrepareContext::new(device, encoder)?;
            assert!(!next.prepare_retained_terrain(&mut upload)?);
            upload.finish();
            Ok::<_, FrameError>(())
        })
        .unwrap();
    assert!(!prefix_draws.is_empty());
    assert_eq!(resident_count(&old), 4);
    let continuation = prefix.submit().unwrap();
    assert_eq!(
        resident_count(&old),
        1,
        "the prefix coarsened the frozen source"
    );
    continuation
        .resume(
            device,
            |_, _| Ok::<_, FrameError>(()),
            |_, _, frame, ()| {
                assert_eq!(
                    old.prepared_terrain_chunks(frame, VIEWPORT),
                    Err(TerrainChunkViewError::Replaced)
                );
                Ok::<_, FrameError>(())
            },
        )
        .unwrap()
        .submit()
        .unwrap()
        .finish(device, |_, _| Ok::<_, FrameError>(()))
        .unwrap();
}

#[test]
fn camera_preparation_refuses_an_unfinished_replacement() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let (_pipelines, _old, next) = tight_budget_pair(&context);
    let device = context.device();
    let frame = frame_from(pose());
    let prepared = context
        .submission()
        .prepare(device, |encoder, _| {
            let error = next
                .prepare_terrain_geometry(device, encoder, &frame, VIEWPORT)
                .unwrap_err();
            assert!(
                matches!(
                    error,
                    FrameError::FeaturePreparation {
                        feature: "streaming terrain",
                        ..
                    }
                ),
                "{error}"
            );
            assert_eq!(
                next.prepared_terrain_chunks(&frame, VIEWPORT),
                Err(TerrainChunkViewError::Unprepared)
            );
            Ok::<_, FrameError>(())
        })
        .unwrap();
    prepared.cancel();
}

#[test]
fn frozen_preparation_refuses_a_second_view_in_one_recording() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let (_pipelines, old, _next) = tight_budget_pair(&context);
    let device = context.device();
    let frame = frame_from(pose());
    let prepared = context
        .submission()
        .prepare(device, |encoder, _| {
            old.prepare_terrain_geometry(device, encoder, &frame, VIEWPORT)?;
            let first = old
                .prepared_terrain_chunks(&frame, VIEWPORT)
                .unwrap()
                .unwrap();
            assert!(!first.is_empty());
            assert!(matches!(
                old.prepare_terrain_geometry(device, encoder, &frame, [TARGET + 1, TARGET]),
                Err(FrameError::StreamingSelectionConflict)
            ));
            assert_eq!(
                old.prepared_terrain_chunks(&frame, VIEWPORT)
                    .unwrap()
                    .unwrap(),
                first
            );
            Ok::<_, FrameError>(())
        })
        .unwrap();
    prepared.cancel();
}
