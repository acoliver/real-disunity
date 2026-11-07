//! A host outside the post chain encodes a compiled list with the frame's pass timer.
use crate::contracts::*;
use crate::{
    CopyOutput, FrameConfig, PassId, PostChain, PostError, Resources, StageFactory, StageSet,
    TimingMode, TimingSupport,
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
    (bytes, chain.take_timings().unwrap(), support)
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
