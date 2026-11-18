use super::*;
use crate::draw::TerrainChunkViewError;
use crate::terrain::streaming::TerrainChunk;
use worldrunner_render_gpu::submission::capture::CaptureWrites;
use worldrunner_render_post::FrameError;

#[test]
fn lender_uses_the_current_frame_admission_and_rejects_cancelled_preparation() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let (context, gpu, frame, _) = withheld_completion_setup(9);
    let viewport = [TARGET; 2];
    let root = streaming(&gpu).committed.borrow()[0].0;
    let mut admitted = Vec::new();
    let prepared = context
        .submission()
        .prepare(context.device(), |encoder, _| {
            assert!(matches!(gpu.prepared_terrain_chunks(&frame, viewport), Err(TerrainChunkViewError::Unprepared)));
            gpu.prepare_terrain_geometry(context.device(), encoder, &frame, viewport)?;
            let keys = gpu.prepared_terrain_chunks(&frame, viewport).unwrap().unwrap();
            admitted.extend(keys.iter().map(|draw| draw.chunk));
            assert!(matches!(gpu.prepared_terrain_chunks(&frame, [TARGET + 1, TARGET]), Err(TerrainChunkViewError::ViewMismatch)));
            assert!(matches!(gpu.prepared_terrain_chunks(&frame, viewport), Ok(Some(keys)) if keys.iter().map(|draw| draw.chunk).eq(admitted.iter().copied())));
            Ok::<_, worldrunner_render_post::FrameError>(())
        })
        .unwrap();
    assert_eq!(resident_count(&gpu), 1);
    assert!(!admitted.is_empty());
    assert!(
        admitted
            .iter()
            .all(|chunk| root.children(1).unwrap().contains(chunk))
    );
    assert!(
        admitted
            .iter()
            .all(|chunk: &TerrainChunk| chunk.stride < root.stride)
    );
    prepared.cancel();
    let _next = context
        .submission()
        .prepare(context.device(), |_, _| {
            assert!(matches!(
                gpu.prepared_terrain_chunks(&frame, viewport),
                Err(TerrainChunkViewError::Unprepared)
            ));
            assert_eq!(resident_count(&gpu), 1);
            Ok::<_, worldrunner_render_post::FrameError>(())
        })
        .unwrap();
}

#[test]
fn lender_reuses_the_prefix_selection_and_neighbors_in_a_read_only_suffix() {
    use crate::draw::TerrainChunkEdge::{East, North, South, West};

    let _session = crate::test_support::SceneTestSession::acquire();
    let (context, gpu, frame, _) = withheld_completion_setup(9);
    let device = context.device();
    let authority = context.submission();
    let viewport = [TARGET; 2];
    let writes = CaptureWrites::new(gpu.capture_write_keys(device).unwrap(), [], []).unwrap();
    let chunks = |frame: &FrameUniform| {
        gpu.prepared_terrain_chunks(frame, viewport)
            .unwrap()
            .unwrap()
    };
    let mut prefix_draws = Vec::new();
    wait(&context);
    let prefix = authority
        .begin_capture(device, writes, frame, |encoder, _, frame| {
            gpu.prepare_terrain_geometry(device, encoder, frame, viewport)?;
            prefix_draws = chunks(frame);
            Ok::<_, FrameError>(())
        })
        .unwrap();
    assert!(prefix_draws.len() > 1);
    assert!(prefix_draws.iter().any(|draw| {
        [West, East, South, North]
            .into_iter()
            .any(|edge| !draw.neighbors(edge).is_empty())
    }));
    assert_eq!(resident_count(&gpu), 1);
    let continuation = prefix.submit().unwrap();
    assert_eq!(resident_count(&gpu), 4);
    let suffix = continuation
        .resume(
            device,
            |_, _| Ok::<_, FrameError>(()),
            |_, _, frame, ()| {
                assert_eq!(chunks(frame), prefix_draws);
                Ok::<_, FrameError>(())
            },
        )
        .unwrap();
    suffix
        .submit()
        .unwrap()
        .finish(device, |_, _| Ok::<_, FrameError>(()))
        .unwrap();
    assert_unrelated_capture_has_no_selection(&context, &gpu, &frame, viewport);
}

fn assert_unrelated_capture_has_no_selection(
    context: &GpuContext,
    gpu: &SceneGpu,
    frame: &FrameUniform,
    viewport: [u32; 2],
) {
    let unrelated = context
        .submission()
        .begin_capture(
            context.device(),
            CaptureWrites::new([], [], []).unwrap(),
            *frame,
            |_, _, frame| {
                assert_eq!(
                    gpu.prepared_terrain_chunks(frame, viewport),
                    Err(TerrainChunkViewError::Unprepared)
                );
                Ok::<_, FrameError>(())
            },
        )
        .unwrap();
    drop(unrelated);
}
