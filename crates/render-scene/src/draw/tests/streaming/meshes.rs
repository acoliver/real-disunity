//! Chunk meshes are built by a worker, and a frame only uploads the ones that are finished.
use super::*;
use crate::terrain::streaming::TerrainChunk;

/// Frames of the calling thread stop waiting for the builds they request, as in production.
fn unsynchronized() -> impl Drop {
    struct Restore(bool);
    impl Drop for Restore {
        fn drop(&mut self) {
            crate::draw::await_builds(self.0);
        }
    }
    Restore(crate::draw::await_builds(false))
}

fn resident_chunks(gpu: &SceneGpu) -> Vec<TerrainChunk> {
    streaming(gpu)
        .committed
        .borrow()
        .iter()
        .map(|(chunk, _)| *chunk)
        .collect()
}

fn finish_builds(gpu: &SceneGpu) {
    let mut builds = streaming(gpu).builds.borrow_mut();
    builds.wait_idle();
    builds.collect().unwrap();
}

struct Rig {
    context: GpuContext,
    pipelines: ScenePipelines,
    target: OffscreenTarget,
    gpu: SceneGpu,
}
impl Rig {
    fn new() -> Self {
        let context = context();
        let pipelines = ScenePipelines::new(context.device(), OffscreenTarget::COLOR_FORMAT);
        let gpu = SceneGpu::upload(
            context.device(),
            context.submission(),
            &streamed_scene(),
            &pipelines,
        )
        .unwrap();
        gpu.set_frame(context.queue(), frame_from(pose()));
        let target = target(&context);
        Self {
            context,
            pipelines,
            target,
            gpu,
        }
    }
    fn render(&mut self) -> crate::offscreen::RgbaImage {
        self.target
            .render(&self.context, &self.gpu, &self.pipelines)
            .unwrap()
    }
}

#[test]
fn unbuilt_children_keep_the_parent_drawn_until_every_child_is_ready() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let mut synchronous = Rig::new();
    let reference = synchronous.render();
    assert_eq!(resident_count(&synchronous.gpu), 4);
    let _unsynchronized = unsynchronized();
    let mut rig = Rig::new();
    let drawn = resident_chunks(&rig.gpu);
    rig.render();
    assert_eq!(resident_chunks(&rig.gpu), drawn);
    assert_eq!(streaming(&rig.gpu).allocation_counts(), (1, 0, 0));
    finish_builds(&rig.gpu);
    let converged = rig.render();
    assert_eq!(resident_count(&rig.gpu), 4);
    assert_eq!(converged, reference);
    wait(&rig.context);
    wait(&synchronous.context);
}

#[test]
fn builds_are_requested_within_the_bound_and_dropped_once_unwanted() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let _unsynchronized = unsynchronized();
    let mut rig = Rig::new();
    rig.render();
    {
        // The worker may already be done, so only the bound is deterministic.
        let builds = streaming(&rig.gpu).builds.borrow();
        assert!(builds.outstanding() <= builds.limit());
    }
    finish_builds(&rig.gpu);
    assert_eq!(streaming(&rig.gpu).builds.borrow().finished_count(), 4);
    rig.render();
    assert_eq!(resident_count(&rig.gpu), 4);
    // The uploaded meshes stay one more frame in case the frame is abandoned.
    assert_eq!(streaming(&rig.gpu).builds.borrow().finished_count(), 4);
    rig.render();
    assert_eq!(streaming(&rig.gpu).builds.borrow().finished_count(), 0);
    wait(&rig.context);
}

#[test]
fn an_abandoned_frame_reuses_the_finished_meshes() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let _unsynchronized = unsynchronized();
    let rig = Rig::new();
    let frame = frame_from(pose());
    prepare_completed(&rig.context, &rig.gpu, &frame, 0).cancel();
    finish_builds(&rig.gpu);
    prepare_completed(&rig.context, &rig.gpu, &frame, 0).cancel();
    assert_eq!(resident_count(&rig.gpu), 1);
    prepare_completed(&rig.context, &rig.gpu, &frame, 0)
        .submit()
        .unwrap();
    assert_eq!(resident_count(&rig.gpu), 4);
    assert_eq!(streaming(&rig.gpu).builds.borrow().outstanding(), 0);
    wait(&rig.context);
}

#[test]
fn dropping_the_gpu_scene_stops_the_mesh_worker() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let _unsynchronized = unsynchronized();
    let context = context();
    let pipelines = ScenePipelines::new(context.device(), OffscreenTarget::COLOR_FORMAT);
    let source = streamed_scene();
    let terrain = source.shared_terrain().unwrap().clone();
    let baseline = std::sync::Arc::strong_count(&terrain);
    let gpu =
        SceneGpu::upload(context.device(), context.submission(), &source, &pipelines).unwrap();
    let before = std::sync::Arc::strong_count(&terrain);
    prepare_completed(&context, &gpu, &frame_from(pose()), 0).cancel();
    assert_eq!(std::sync::Arc::strong_count(&terrain), before + 1);
    drop(gpu);
    // The worker holds the terrain until it exits, and dropping the source joins it.
    assert_eq!(std::sync::Arc::strong_count(&terrain), baseline);
    wait(&context);
}
