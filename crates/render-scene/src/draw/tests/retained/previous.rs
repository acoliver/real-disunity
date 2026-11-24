//! Velocity pipelines keep each instance's previous-frame transform beside the current one.
use super::*;
use crate::test_support::upload_scene;

const HDR: TextureFormat = TextureFormat::Rgba16Float;
const STRIDE: usize = crate::geometry::INSTANCE_BYTES;

struct Fixture {
    context: GpuContext,
    gpu: SceneGpu,
    handle: crate::draw::InstanceHandle,
    pipelines: ScenePipelines,
    renderer: crate::compositor::FrameRenderer,
}

fn fixture() -> Fixture {
    let context = context();
    let base = scene(SceneAlphaMode::Opaque, false, 2);
    let effects = SceneEffects::new(SceneEffectsParts {
        anti_alias: SceneAntiAlias::taa(900_000),
        ..effect_parts()
    })
    .unwrap();
    let source = RenderScene::new(SceneParts {
        resources: base.resources().clone(),
        geometry: base.geometry().clone(),
        terrain: None,
        effects: Some(effects),
    })
    .unwrap();
    let pipelines = ScenePipelines::with_taa_velocity(context.device(), HDR, 1).unwrap();
    let gpu = upload_scene(context.device(), context.submission(), &source, &pipelines).unwrap();
    let renderer = crate::compositor::FrameRenderer::hdr(
        &context,
        [TARGET, TARGET],
        COLOR_FORMAT,
        1,
        worldrunner_render_post::ToneMap::Reinhard,
        source.effects(),
    )
    .unwrap();
    let handle = gpu.instance_handles().next().unwrap();
    let mut fixture = Fixture {
        context,
        gpu,
        handle,
        pipelines,
        renderer,
    };
    fixture.flush();
    fixture
}

impl Fixture {
    /// The current and previous bytes of the first batch, one instance stride per slot.
    fn buffers(&self) -> (Vec<u8>, Vec<u8>) {
        let batch = self.gpu.instances.batches.values().next().unwrap();
        let length = batch.bytes.len() as u64;
        let read = |buffer: Option<&wgpu::Buffer>| {
            super::scheduler::buffer_bytes(&self.context, buffer.unwrap(), length)
        };
        (
            read(batch.buffer.as_deref()),
            read(batch.previous.as_deref()),
        )
    }

    fn flush(&mut self) {
        self.color_frame(true);
    }

    fn color_frame(&mut self, submit: bool) {
        use worldrunner_render_post::{AttachmentView, ResourceRef};
        let color = AttachmentView::new(
            ResourceRef::new("scene color"),
            self.renderer.targets().scene().texture(),
        )
        .unwrap();
        let depth = AttachmentView::new(
            ResourceRef::new("scene depth"),
            self.renderer.targets().depth().texture(),
        )
        .unwrap();
        let mut encoder = self
            .context
            .submission()
            .begin(self.context.device())
            .unwrap();
        self.renderer
            .encoding()
            .encode_scene(
                self.context.device(),
                &mut encoder,
                (&mut self.gpu, &self.pipelines),
                crate::compositor::ColorDepth {
                    color: &color,
                    depth: &depth,
                },
                frame_from(pose()),
            )
            .unwrap();
        if submit {
            encoder.finish().submit().unwrap();
        }
    }

    fn slot(bytes: &[u8], slot: usize) -> &[u8] {
        &bytes[slot * STRIDE..(slot + 1) * STRIDE]
    }
}

#[test]
fn a_new_instance_starts_with_previous_equal_to_current() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let fixture = fixture();
    let (current, previous) = fixture.buffers();
    assert_eq!(current.len(), 2 * STRIDE);
    assert_eq!(current, previous);
}

#[test]
fn an_edit_keeps_the_last_published_transform_as_previous() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let mut fixture = fixture();
    let (before, _) = fixture.buffers();
    fixture
        .gpu
        .set_instance_transform(fixture.handle, moved(4321))
        .unwrap();
    fixture.flush();
    let (current, previous) = fixture.buffers();
    assert_ne!(Fixture::slot(&current, 0), Fixture::slot(&before, 0));
    assert_eq!(Fixture::slot(&previous, 0), Fixture::slot(&before, 0));
    assert_eq!(Fixture::slot(&previous, 1), Fixture::slot(&current, 1));
}

#[test]
fn a_frame_without_edits_converges_previous_to_current() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let mut fixture = fixture();
    fixture
        .gpu
        .set_instance_transform(fixture.handle, moved(4321))
        .unwrap();
    fixture.flush();
    fixture.flush();
    let (current, previous) = fixture.buffers();
    assert_eq!(current, previous);
}

#[test]
fn a_second_edit_moves_the_first_one_into_previous() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let mut fixture = fixture();
    fixture
        .gpu
        .set_instance_transform(fixture.handle, moved(4321))
        .unwrap();
    fixture.flush();
    let (first, _) = fixture.buffers();
    fixture
        .gpu
        .set_instance_transform(fixture.handle, moved(8000))
        .unwrap();
    fixture.flush();
    let (current, previous) = fixture.buffers();
    assert_ne!(Fixture::slot(&current, 0), Fixture::slot(&first, 0));
    assert_eq!(Fixture::slot(&previous, 0), Fixture::slot(&first, 0));
}

#[test]
fn a_cancelled_frame_leaves_previous_untouched_until_a_frame_publishes() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let mut fixture = fixture();
    let (before, previous_before) = fixture.buffers();
    fixture
        .gpu
        .set_instance_transform(fixture.handle, moved(4321))
        .unwrap();
    let Fixture { context, gpu, .. } = &mut fixture;
    let cancelled = context
        .submission()
        .prepare(context.device(), |encoder, _| {
            gpu.flush_instances_encoded(context.device(), encoder)
                .map(|_| ())
        })
        .unwrap();
    cancelled.cancel();
    let (current, previous) = fixture.buffers();
    assert_eq!(current, before);
    assert_eq!(previous, previous_before);
    fixture.flush();
    let (current, previous) = fixture.buffers();
    assert_ne!(Fixture::slot(&current, 0), Fixture::slot(&before, 0));
    assert_eq!(Fixture::slot(&previous, 0), Fixture::slot(&before, 0));
}

#[test]
fn a_reused_slot_starts_with_previous_equal_to_current() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let mut fixture = fixture();
    let (_, _) = fixture.buffers();
    let object = fixture.gpu.instance_handles().count();
    assert_eq!(object, 2);
    fixture.gpu.remove_instance(fixture.handle).unwrap();
    fixture.flush();
    let source = scene(SceneAlphaMode::Opaque, false, 2);
    let replacement = SceneObject::new(
        moved(9000),
        source.geometry().objects()[0].mesh(),
        source.geometry().objects()[0].material(),
    );
    fixture.gpu.add_instance(replacement, true).unwrap();
    fixture.flush();
    let (current, previous) = fixture.buffers();
    assert_eq!(current, previous);
}

#[test]
fn a_velocity_history_reset_survives_cancellation_and_initializes_current_transforms() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let mut fixture = fixture();
    fixture
        .gpu
        .set_instance_transform(fixture.handle, moved(4321))
        .unwrap();
    fixture.gpu.reset_velocity_history();
    let Fixture { context, gpu, .. } = &mut fixture;
    context
        .submission()
        .prepare(context.device(), |encoder, _| {
            gpu.flush_instances_encoded(context.device(), encoder)
                .map(|_| ())
        })
        .unwrap()
        .cancel();
    fixture.flush();
    let (current, previous) = fixture.buffers();
    assert_eq!(current, previous);
    fixture
        .gpu
        .set_instance_transform(fixture.handle, moved(8000))
        .unwrap();
    fixture.flush();
    let (next, previous) = fixture.buffers();
    assert_ne!(next, current);
    assert_eq!(previous, current);
}

#[test]
fn instance_buffers_count_in_the_authority_census_until_the_scene_drops() {
    use worldrunner_render_gpu::census::ResourceClass;
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    // A private authority, so other scenes on the shared device do not touch its tally.
    let authority =
        worldrunner_render_gpu::submission::SubmissionAuthority::new(context.queue().clone());
    let source = scene(SceneAlphaMode::Opaque, false, 2);
    let pipelines = ScenePipelines::with_taa_velocity(context.device(), HDR, 1).unwrap();
    let mut gpu = upload_scene(context.device(), &authority, &source, &pipelines).unwrap();
    gpu.flush_instances(context.device(), &authority).unwrap();
    let expected: u64 = gpu
        .instances
        .batches
        .values()
        .flat_map(|batch| [batch.buffer.as_deref(), batch.previous.as_deref()])
        .map(|buffer| buffer.unwrap().size())
        .sum();
    assert!(expected > 0);
    assert_eq!(
        authority.census().snapshot().get(ResourceClass::Instance),
        expected
    );
    drop(gpu);
    assert_eq!(
        authority.census().snapshot().get(ResourceClass::Instance),
        0
    );
}

#[test]
fn scene_textures_and_mesh_buffers_count_in_the_authority_census_until_the_scene_drops() {
    use worldrunner_render_gpu::census::{ResourceClass, texture_bytes};
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    // A private authority, so other scenes on the shared device do not touch its tally.
    let authority =
        worldrunner_render_gpu::submission::SubmissionAuthority::new(context.queue().clone());
    let source = super::super::decals::source(false, true);
    let pipelines = ScenePipelines::new(context.device(), HDR, 1).unwrap();
    let gpu = upload_scene(context.device(), &authority, &source, &pipelines).unwrap();
    let textures: u64 = gpu
        .retained
        .textures
        .iter()
        .map(|texture| texture_bytes(texture))
        .sum();
    // Levels share chunks, so each buffer counts once however many levels read it.
    let mut chunks: Vec<&wgpu::Buffer> = Vec::new();
    for lod in gpu.meshes.iter().flat_map(|mesh| &mesh.lods) {
        for span in lod.transition.iter().chain([&lod.vertices, &lod.indices]) {
            if !chunks.contains(&span.buffer()) {
                chunks.push(span.buffer());
            }
        }
    }
    let meshes: u64 = chunks.iter().map(|buffer| buffer.size()).sum();
    assert!(textures > 0 && meshes > 0);
    let snapshot = authority.census().snapshot();
    assert_eq!(snapshot.get(ResourceClass::Texture), textures);
    assert_eq!(snapshot.get(ResourceClass::Mesh), meshes);
    drop(gpu);
    let snapshot = authority.census().snapshot();
    assert_eq!(snapshot.get(ResourceClass::Texture), 0);
    assert_eq!(snapshot.get(ResourceClass::Mesh), 0);
}

#[test]
fn resource_only_flushes_and_cancelled_colour_keep_the_visible_baseline() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let mut fixture = fixture();
    let (before, _) = fixture.buffers();
    fixture
        .gpu
        .set_instance_transform(fixture.handle, moved(4321))
        .unwrap();
    for _ in 0..2 {
        fixture
            .gpu
            .flush_instances(fixture.context.device(), fixture.context.submission())
            .unwrap();
        let (current, previous) = fixture.buffers();
        assert_ne!(Fixture::slot(&current, 0), Fixture::slot(&before, 0));
        assert_eq!(Fixture::slot(&previous, 0), Fixture::slot(&before, 0));
        let batch = fixture.gpu.instances.batches.values().next().unwrap();
        assert!(batch.pending_ranges().is_empty());
        assert!(batch.has_pending());
    }
    fixture.color_frame(false);
    fixture.flush();
    let (current, previous) = fixture.buffers();
    assert_eq!(Fixture::slot(&previous, 0), Fixture::slot(&before, 0));
    fixture.flush();
    assert_eq!(fixture.buffers(), (current.clone(), current));
}

#[test]
fn visible_history_key_is_stable_and_distinct_from_resource_acknowledgements() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let mut fixture = fixture();
    let key = fixture.gpu.instance_history_key();
    let resources = fixture
        .gpu
        .capture_write_keys(fixture.context.device())
        .unwrap();
    assert!(!resources.contains(&key));
    fixture.gpu.reset_velocity_history();
    fixture
        .gpu
        .flush_instances(fixture.context.device(), fixture.context.submission())
        .unwrap();
    fixture.color_frame(false);
    fixture.flush();
    assert_eq!(fixture.gpu.instance_history_key(), key);
    assert!(
        !fixture
            .gpu
            .capture_write_keys(fixture.context.device())
            .unwrap()
            .contains(&key)
    );
}

#[test]
fn previous_only_settling_changes_physical_content_once_without_new_edits() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let mut fixture = fixture();
    fixture
        .gpu
        .set_instance_transform(fixture.handle, moved(4321))
        .unwrap();
    fixture.flush();
    let edits = fixture.gpu.instances.edits();
    let moving = fixture.gpu.instances.content_generation();
    fixture.flush();
    let settled = fixture.gpu.instances.content_generation();
    assert!(settled > moving);
    assert_eq!(fixture.gpu.instances.edits(), edits);
    fixture.flush();
    assert_eq!(fixture.gpu.instances.content_generation(), settled);
    assert_eq!(fixture.gpu.instances.edits(), edits);
}
