//! Velocity pipelines keep each instance's previous-frame transform beside the current one.
use super::*;

const HDR: TextureFormat = TextureFormat::Rgba16Float;
const STRIDE: usize = crate::geometry::INSTANCE_BYTES;

struct Fixture {
    context: GpuContext,
    gpu: SceneGpu,
    handle: crate::draw::InstanceHandle,
}

fn fixture() -> Fixture {
    let context = context();
    let source = scene(SceneAlphaMode::Opaque, false, 2);
    let pipelines = ScenePipelines::with_taa_velocity(context.device(), HDR, 1).unwrap();
    let mut gpu =
        SceneGpu::upload(context.device(), context.submission(), &source, &pipelines).unwrap();
    gpu.flush_instances(context.device(), context.submission())
        .unwrap();
    let handle = gpu.instance_handles().next().unwrap();
    Fixture {
        context,
        gpu,
        handle,
    }
}

impl Fixture {
    /// The current and previous bytes of the first batch, one instance stride per slot.
    fn buffers(&self) -> (Vec<u8>, Vec<u8>) {
        let batch = self.gpu.instances.batches.values().next().unwrap();
        let length = batch.bytes.len() as u64;
        let read = |buffer: &Option<wgpu::Buffer>| {
            super::scheduler::buffer_bytes(&self.context, buffer.as_ref().unwrap(), length)
        };
        (read(&batch.buffer), read(&batch.previous))
    }

    fn flush(&mut self) {
        self.gpu
            .flush_instances(self.context.device(), self.context.submission())
            .unwrap();
    }

    fn slot(bytes: &[u8], slot: usize) -> &[u8] {
        &bytes[slot * STRIDE..(slot + 1) * STRIDE]
    }
}

#[test]
fn a_new_instance_starts_with_previous_equal_to_current() {
    let fixture = fixture();
    let (current, previous) = fixture.buffers();
    assert_eq!(current.len(), 2 * STRIDE);
    assert_eq!(current, previous);
}

#[test]
fn an_edit_keeps_the_last_published_transform_as_previous() {
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
