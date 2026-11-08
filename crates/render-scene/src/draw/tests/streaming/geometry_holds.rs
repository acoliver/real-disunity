//! The geometry the program runtime caches for streamed chunks holds their GPU buffers. The
//! streaming budget counts a chunk while it is resident or retired, so the cache may hold a
//! chunk's buffers no longer than the chunk is resident: the buffers it holds never outlast the
//! ones the residency holds, and a retired chunk's are released before the next uploads are
//! admitted.
use super::*;
use worldrunner_render_gpu::submission::PreparedFrame;

/// Prepares the chunks the frame selects and the program geometry for them, as a frame does.
fn prepare(
    context: &GpuContext,
    gpu: &SceneGpu,
    frame: &FrameUniform,
    completed: u64,
) -> PreparedFrame {
    context
        .submission()
        .prepare(context.device(), |encoder, _| {
            let mut upload =
                worldrunner_render_post::PrepareContext::new(context.device(), encoder)?;
            streaming(gpu).prepare_completed(&mut upload, frame, [TARGET; 2], completed)?;
            streaming(gpu).prepare_geometry(
                &upload.publications(),
                gpu.terrain.as_ref().expect("terrain"),
            );
            upload.finish();
            Ok::<_, worldrunner_render_post::FrameError>(())
        })
        .unwrap()
}

/// What the cache holds against what the residency holds, at one point of a walk.
fn check(gpu: &SceneGpu, budget: u32, at: &str) -> usize {
    let held = gpu.cached_chunk_buffers();
    let resident = streaming(gpu).resident_buffers();
    for buffer in &held {
        assert!(
            resident.contains(buffer),
            "{at}: the geometry cache holds a chunk that is neither committed nor being drawn \
             ({} cached, {} resident)",
            held.len(),
            resident.len()
        );
    }
    assert!(
        held.len() <= budget as usize,
        "{at}: the geometry cache holds {} chunks against a budget of {budget}",
        held.len()
    );
    held.len()
}

/// Splits and merges the terrain by alternating the camera, with completion withheld, then
/// released, then with a cancelled frame between.
fn walk(budget: u32) {
    let (context, gpu, mut frame, far) = withheld_completion_setup(budget);
    let near = frame.view_projection;
    let mut most = 0;
    let mut saw_retired = false;
    for index in 0..12 {
        frame.view_projection = if index % 2 == 0 { near } else { far };
        let prepared = prepare(&context, &gpu, &frame, 0);
        most = most.max(check(
            &gpu,
            budget,
            &format!("budget {budget} step {index} prepared"),
        ));
        prepared.submit().unwrap();
        most = most.max(check(
            &gpu,
            budget,
            &format!("budget {budget} step {index} committed"),
        ));
        saw_retired |= streaming(&gpu).allocation_counts().2 > 0;
    }
    assert!(saw_retired, "the walk retired no chunk");
    assert!(most > 1, "the walk cached no more than the root");
    wait(&context);
    let completed = context.submission().completed_serial();
    frame.view_projection = far;
    let prepared = prepare(&context, &gpu, &frame, completed);
    check(
        &gpu,
        budget,
        &format!("budget {budget} after completion prepared"),
    );
    prepared.submit().unwrap();
    let held = check(
        &gpu,
        budget,
        &format!("budget {budget} after completion committed"),
    );
    assert_eq!(
        held,
        resident_count(&gpu),
        "only resident chunks are cached"
    );

    frame.view_projection = near;
    let cancelled = prepare(&context, &gpu, &frame, completed);
    check(&gpu, budget, &format!("budget {budget} cancelled prepared"));
    cancelled.cancel();
    check(&gpu, budget, &format!("budget {budget} cancelled"));
    frame.view_projection = far;
    let prepared = prepare(&context, &gpu, &frame, completed);
    check(
        &gpu,
        budget,
        &format!("budget {budget} after cancellation prepared"),
    );
    prepared.submit().unwrap();
    wait(&context);
}

#[test]
fn cached_geometry_holds_only_buffers_the_residency_holds() {
    let _session = crate::test_support::SceneTestSession::acquire();
    walk(5);
    walk(9);
}
