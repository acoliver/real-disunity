use super::*;
use crate::draw::TerrainChunkViewError;
use crate::terrain::streaming::TerrainChunk;

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
