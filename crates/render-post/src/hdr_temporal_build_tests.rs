//! The temporal bank `hdr_taa::append` builds for `UpscaleMode::Temporal`. The public gate
//! "temporal reconstruction is not yet scheduled" rejects these configurations, so the tests
//! build `HdrPost` directly, below the gate. They establish the schedule, the shared bank and
//! the block upload; the stage maths is covered by `taa_upscale_gpu_tests`.
use crate::hdr_post::HdrPost;
use crate::{
    BufferReadback, FrameConfig, FrameTargets, HdrPostSettings, PostResolve, StageSet, TaaSettings,
    TemporalBlock, ToneMap, UpscaleMode, UpscaleSettings,
};

pub(crate) fn settings() -> HdrPostSettings {
    HdrPostSettings {
        resolve: PostResolve::taa(0.9),
        upscale: Some(UpscaleSettings {
            mode: UpscaleMode::Temporal,
            sharpness: None,
        }),
        ..Default::default()
    }
}

pub(crate) fn targets(device: &wgpu::Device, render: [u32; 2], output: [u32; 2]) -> FrameTargets {
    let config =
        FrameConfig::new(output, wgpu::TextureFormat::Rgba16Float, 1).with_render_extent(render);
    FrameTargets::with_velocity(device, config, true).unwrap()
}

pub(crate) fn slots(
    post: &HdrPost,
    device: &wgpu::Device,
    targets: &FrameTargets,
) -> Vec<StageSet> {
    let sets = (0..3)
        .map(|slot| {
            post.build(
                device,
                targets,
                slot,
                wgpu::TextureFormat::Rgba8UnormSrgb,
                (1.0, ToneMap::Reinhard),
            )
        })
        .collect();
    post.settle_build(true);
    sets
}

fn shared(sets: &[StageSet]) -> Vec<wgpu::Texture> {
    sets[0]
        .resources
        .textures
        .iter()
        .filter(|t| sets[1..].iter().all(|s| s.resources.textures.contains(t)))
        .cloned()
        .collect()
}

#[test]
fn temporal_mode_schedules_the_reconstruction_instead_of_the_native_resolve() {
    let (device, _) = crate::gpu_tests::gpu();
    let post = HdrPost::new(&device, &settings()).unwrap();
    for set in slots(&post, &device, &targets(&device, [10, 10], [16, 16])) {
        let names: Vec<_> = set.stages.iter().map(|s| s.id.name).collect();
        assert_eq!(
            names,
            ["temporal upscale", "temporal upscale history", "tone"]
        );
    }
}

#[test]
fn the_temporal_bank_is_one_shared_history_and_one_output_per_slot_at_the_output_extent() {
    let (device, _) = crate::gpu_tests::gpu();
    let post = HdrPost::new(&device, &settings()).unwrap();
    let sets = slots(&post, &device, &targets(&device, [10, 10], [16, 16]));
    let history = shared(&sets);
    assert_eq!(history.len(), 1, "one colour history, no depth history");
    for set in &sets {
        assert_eq!(set.resources.textures.len(), 2, "output and history only");
        for texture in &set.resources.textures {
            assert_eq!(texture.format(), crate::taa_upscale_gpu::FORMAT);
            assert_eq!((texture.width(), texture.height()), (16, 16));
        }
    }
    let outputs: Vec<_> = sets.iter().flat_map(|s| &s.resources.textures).collect();
    assert_eq!(
        outputs.iter().filter(|t| !history.contains(t)).count(),
        3,
        "each slot owns its output"
    );
}

#[test]
fn the_temporal_bank_follows_either_extent_and_survives_an_identical_rebuild() {
    let (device, _) = crate::gpu_tests::gpu();
    let post = HdrPost::new(&device, &settings()).unwrap();
    let first = shared(&slots(
        &post,
        &device,
        &targets(&device, [10, 10], [16, 16]),
    ));
    let same = shared(&slots(
        &post,
        &device,
        &targets(&device, [10, 10], [16, 16]),
    ));
    assert_eq!(same, first, "same extents reuse the bank");
    let output_only = shared(&slots(
        &post,
        &device,
        &targets(&device, [10, 10], [18, 18]),
    ));
    assert_ne!(output_only, same, "an output-only resize replaces the bank");
    let render_only = shared(&slots(
        &post,
        &device,
        &targets(&device, [12, 12], [18, 18]),
    ));
    assert_ne!(
        render_only, output_only,
        "a render-only resize replaces the bank"
    );
    assert_eq!(render_only[0].width(), 18);
}

fn readable_block_buffer(device: &wgpu::Device) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("readable slot block"),
        size: u64::try_from(crate::TAA_UNIFORM_BYTES).unwrap(),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    })
}

#[test]
fn a_temporal_block_reaches_the_slot_uniform_buffer() {
    let (device, queue) = crate::gpu_tests::gpu();
    let block = TemporalBlock {
        render: [10, 10],
        output: [16, 16],
        jitter: [0.25, -0.5],
        phase_count: 8,
        settings: TaaSettings::new(0.9),
        history_valid: true,
    };
    let buffer = readable_block_buffer(&device);
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    crate::hdr_taa::upload_temporal(&device, &mut encoder, &buffer, Some(&block)).unwrap();
    let mut capture = BufferReadback::new(&device, buffer.size(), 4096).unwrap();
    capture.encode(&mut encoder, &buffer).unwrap();
    let submission = queue.submit([encoder.finish()]);
    let bytes = capture.read_encoded(&device, Some(submission)).unwrap();
    assert_eq!(
        bytes[..crate::TEMPORAL_BLOCK_BYTES],
        block.to_bytes().unwrap()
    );
    assert!(
        bytes[crate::TEMPORAL_BLOCK_BYTES..].iter().all(|&b| b == 0),
        "the rest of the buffer stays zero"
    );
}

#[test]
fn no_block_uploads_zeros_which_reads_as_no_history() {
    let (device, queue) = crate::gpu_tests::gpu();
    let buffer = readable_block_buffer(&device);
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    crate::hdr_taa::upload_temporal(&device, &mut encoder, &buffer, None).unwrap();
    let mut capture = BufferReadback::new(&device, buffer.size(), 4096).unwrap();
    capture.encode(&mut encoder, &buffer).unwrap();
    let submission = queue.submit([encoder.finish()]);
    let bytes = capture.read_encoded(&device, Some(submission)).unwrap();
    assert!(bytes.iter().all(|&b| b == 0));
}
