//! A host outside the post chain encodes a compiled list with the frame's pass timer.
use crate::contracts::*;
use crate::{
    AttachmentDeclaration, AttachmentLoad, AttachmentView, CopyOutput, FrameConfig, FrameError,
    FrameOperations, FramePlan, OperationsGroup, PassDeclaration, PassEntry, PassId, PassSlot,
    PostChain, PostError, ResourceRef, Resources, StageFactory, StageSet, TimingMode,
    TimingSupport,
};

use super::operations_tests::{buffer, device, id};

struct Factory;
impl StageFactory for Factory {
    fn build(
        &self,
        device: &wgpu::Device,
        targets: &crate::FrameTargets,
        resources: &Resources,
    ) -> Result<StageSet, PostError> {
        CopyOutput::new(device, targets.config().format).build(device, targets, resources)
    }
}

const BYTES: [u8; 16] = [7; 16];

/// A compiled upload of [`BYTES`] into a buffer and a readback of it into a staging buffer.
fn operations(device: &wgpu::Device) -> (ResourceRegistry, [TypedOperation; 2], wgpu::Buffer) {
    let mut registry = ResourceRegistry::default();
    let source = buffer(
        device,
        16,
        wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST,
    );
    let staging = buffer(
        device,
        16,
        wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
    );
    let mut view = |buffer| {
        let key = registry
            .import_buffer(buffer, ResourceLifetime::FrameLocal, None)
            .unwrap();
        registry.buffer_view(key, 0..16, 1).unwrap()
    };
    let (written, read_back) = (view(&source), view(&staging));
    let operations = [
        TypedOperation::new(
            id("timed upload"),
            TypedWork::Upload {
                destination: written.clone(),
                bytes: BYTES.to_vec(),
            },
        ),
        TypedOperation::new(
            id("timed readback"),
            TypedWork::ReadbackBuffer {
                source: written,
                destination: read_back,
            },
        ),
    ];
    (registry, operations, staging)
}

/// Encodes an upload and a readback with the chain's timer, submits the frame and returns the
/// staged bytes and the frame's timings.
fn run(timed: bool) -> (Vec<u8>, Vec<crate::FrameTiming>, TimingSupport) {
    let (device, queue) = device();
    let chain = PostChain::new(
        &device,
        FrameConfig::new([8, 8], wgpu::TextureFormat::Rgba8Unorm, 1),
        Resources::default(),
        Factory,
        crate::SceneDepth::Independent,
    )
    .unwrap();
    let support = if timed {
        chain
            .configure_timing(&device, &queue, TimingMode::Optional)
            .unwrap()
    } else {
        TimingSupport::Disabled
    };
    let (registry, operations, staging) = operations(&device);
    let compiled = registry.compile_operations(&device, &operations).unwrap();
    let authority = worldrunner_render_gpu::submission::SubmissionAuthority::new(queue.clone());
    let mut encoder = authority.begin(&device).unwrap();
    chain.begin_timing(5).unwrap();
    chain
        .timing_state()
        .encode_checked(&compiled, &registry, &mut encoder)
        .unwrap();
    chain.finish_timing(&mut encoder).unwrap();
    let submitted = encoder.finish().submit().unwrap();
    device
        .poll(wgpu::PollType::Wait {
            submission_index: Some(submitted),
            timeout: Some(std::time::Duration::from_secs(10)),
        })
        .unwrap();
    staging.slice(..).map_async(wgpu::MapMode::Read, |result| {
        result.unwrap();
    });
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    let bytes = staging.slice(..).get_mapped_range().unwrap().to_vec();
    let expected = usize::from(support == TimingSupport::Supported);
    let frames =
        crate::timing_test_support::take_frames(&device, expected, || chain.take_timings());
    (bytes, frames, support)
}

#[test]
fn the_list_is_encoded_in_order_and_each_operation_is_a_timed_pass() {
    let (bytes, timings, support) = run(true);
    assert_eq!(bytes, BYTES);
    if support == TimingSupport::Supported {
        assert_eq!(timings.len(), 1);
        assert_eq!(timings[0].frame_id, 5);
        let passes: Vec<_> = timings[0].passes.iter().map(|pass| pass.pass_id).collect();
        assert_eq!(
            passes,
            [
                PassId::Operation(id("timed upload")),
                PassId::Operation(id("timed readback")),
            ]
        );
    } else {
        eprintln!("no timestamp queries on this adapter: {support:?}");
        assert!(timings.is_empty());
    }
}

#[test]
fn a_chain_without_a_timer_still_encodes_the_list() {
    let (bytes, timings, _) = run(false);
    assert_eq!(bytes, BYTES);
    assert!(timings.is_empty());
}

/// A compiled list a frame plan positions, encoded through the timer the executor hands it.
struct Positioned {
    compiled: CompiledOperations,
    registry: ResourceRegistry,
}
impl FrameOperations for Positioned {
    fn encode_operations(
        &self,
        timing: Option<&crate::TimingState>,
        encoder: &mut wgpu::CommandEncoder,
    ) -> Result<(), FrameError> {
        timing
            .expect("the executor hands a group the frame's timer")
            .encode_checked(&self.compiled, &self.registry, encoder)
            .map_err(FrameError::from)
    }
}

fn clearing(
    device: &wgpu::Device,
    entry: PassEntry,
    name: &'static str,
) -> (PassDeclaration, AttachmentView) {
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let resource = ResourceRef::new(name);
    let mut pass = PassDeclaration::nth(entry.slot(), entry.occurrence());
    pass.color = Some(AttachmentDeclaration {
        resource,
        format,
        size: [8, 8],
        samples: 1,
        load: AttachmentLoad::ClearColor(wgpu::Color::BLACK),
        store: wgpu::StoreOp::Store,
        read_only: false,
    });
    let texture = crate::color_texture(device, name, [8, 8], format);
    (pass, AttachmentView::new(resource, &texture).unwrap())
}

#[test]
fn a_positioned_group_is_timed_between_the_passes_around_it() {
    let (device, queue) = device();
    let mut chain = PostChain::new(
        &device,
        FrameConfig::new([8, 8], wgpu::TextureFormat::Rgba8Unorm, 1),
        Resources::default(),
        Factory,
        crate::SceneDepth::Independent,
    )
    .unwrap();
    let support = chain
        .configure_timing(&device, &queue, TimingMode::Optional)
        .unwrap();
    let (registry, operations, staging) = operations(&device);
    let compiled = registry.compile_operations(&device, &operations).unwrap();
    let work = Positioned { compiled, registry };
    let first = PassEntry::new(PassSlot::Opaque);
    let second = PassEntry::nth(PassSlot::Opaque, 1);
    let ((first_pass, first_view), (second_pass, second_view)) = (
        clearing(&device, first, "first target"),
        clearing(&device, second, "second target"),
    );
    let authority = worldrunner_render_gpu::submission::SubmissionAuthority::new(queue.clone());
    let mut encoder = authority.begin(&device).unwrap();
    chain.begin_timing(9).unwrap();
    chain
        .encode_geometry(
            &device,
            &mut encoder,
            &FramePlan {
                passes: &[first_pass, second_pass],
                views: &[first_view, second_view],
                requested: &[],
                operations: &[OperationsGroup {
                    before: Some(second),
                    work: &work,
                }],
            },
            &mut [],
        )
        .unwrap();
    chain.finish_timing(&mut encoder).unwrap();
    let submitted = encoder.finish().submit().unwrap();
    device
        .poll(wgpu::PollType::Wait {
            submission_index: Some(submitted),
            timeout: Some(std::time::Duration::from_secs(10)),
        })
        .unwrap();
    staging.slice(..).map_async(wgpu::MapMode::Read, |result| {
        result.unwrap();
    });
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    assert_eq!(
        staging.slice(..).get_mapped_range().unwrap().to_vec(),
        BYTES
    );
    let expected = usize::from(support == TimingSupport::Supported);
    let frames =
        crate::timing_test_support::take_frames(&device, expected, || chain.take_timings());
    if support == TimingSupport::Supported {
        let passes: Vec<_> = frames[0].passes.iter().map(|pass| pass.pass_id).collect();
        assert_eq!(
            passes,
            [
                PassId::Geometry(first),
                PassId::Operation(id("timed upload")),
                PassId::Operation(id("timed readback")),
                PassId::Geometry(second),
            ]
        );
    } else {
        eprintln!("no timestamp queries on this adapter: {support:?}");
        assert!(frames.is_empty());
    }
}
