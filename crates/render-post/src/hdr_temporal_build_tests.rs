//! The temporal bank `hdr_taa::append` builds for `UpscaleMode::Temporal`. Some tests build
//! `HdrPost` directly to inspect slots; the chain tests use the public `ToneMapOutput`. They establish the schedule, the shared bank and
//! the block upload; the stage maths is covered by `taa_upscale_gpu_tests`.
use crate::hdr_post::HdrPost;
use crate::{
    BufferReadback, FrameConfig, FrameTargets, HdrPostSettings, PostResolve, StageSet, TaaSettings,
    TemporalBlock, ToneMap, ToneMapOutput, UpscaleMode, UpscaleSettings,
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

/// Targets without a temporal mask: the path before masks existed.
pub(crate) fn targets(device: &wgpu::Device, render: [u32; 2], output: [u32; 2]) -> FrameTargets {
    target_set(device, render, output, false)
}

/// Targets with the producer mask the real chain allocates for temporal reconstruction.
pub(crate) fn masked_targets(
    device: &wgpu::Device,
    render: [u32; 2],
    output: [u32; 2],
) -> FrameTargets {
    target_set(device, render, output, true)
}

fn target_set(
    device: &wgpu::Device,
    render: [u32; 2],
    output: [u32; 2],
    temporal_mask: bool,
) -> FrameTargets {
    let config =
        FrameConfig::new(output, wgpu::TextureFormat::Rgba16Float, 1).with_render_extent(render);
    let options = crate::TargetOptions {
        velocity: true,
        temporal_mask,
    };
    FrameTargets::with_options(device, config, options).unwrap()
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

const UNLOCKED_STAGES: [&str; 6] = [
    "temporal depth clear",
    "temporal reconstruct dilate",
    "temporal depth clip",
    "temporal upscale",
    "temporal upscale history",
    "tone",
];

const LOCKED_STAGES: [&str; 8] = [
    "temporal depth clear",
    "temporal reconstruct dilate",
    "temporal depth clip",
    "temporal create locks",
    "temporal upscale",
    "temporal upscale history",
    "temporal lock history",
    "tone",
];

fn scheduled_names() -> Vec<Vec<&'static str>> {
    let (device, _) = crate::gpu_tests::gpu();
    let post = HdrPost::new(&device, &settings()).unwrap();
    slots(&post, &device, &targets(&device, [10, 10], [16, 16]))
        .iter()
        .map(|set| set.stages.iter().map(|s| s.id.name).collect())
        .collect()
}

#[test]
fn temporal_mode_schedules_the_reconstruction_without_lock_passes_by_default() {
    for names in scheduled_names() {
        assert_eq!(names, UNLOCKED_STAGES);
    }
}

#[test]
fn temporal_mode_schedules_the_lock_passes_when_locks_are_on() {
    let _on = crate::taa_upscale_gpu::WithLocks::new();
    for names in scheduled_names() {
        assert_eq!(names, LOCKED_STAGES);
    }
}

/// Checks the bank of a three slot build at 10x10 render and 16x16 output. `locks` adds the
/// candidates, the lock bank per slot and the shared lock history.
fn assert_bank(locks: bool) {
    let (device, _) = crate::gpu_tests::gpu();
    let post = HdrPost::new(&device, &settings()).unwrap();
    let sets = slots(&post, &device, &targets(&device, [10, 10], [16, 16]));
    let history = shared(&sets);
    assert_eq!(
        history.len(),
        1 + usize::from(locks),
        "colour history, lock history if on"
    );
    for set in &sets {
        assert_eq!(set.resources.textures.len(), 5 + 3 * usize::from(locks));
        let at_output = set
            .resources
            .textures
            .iter()
            .filter(|t| (t.width(), t.height()) == (16, 16))
            .inspect(|t| assert_eq!(t.format(), crate::taa_upscale_gpu::FORMAT))
            .count();
        assert_eq!(
            at_output,
            2 + 2 * usize::from(locks),
            "output, history, lock bank and history"
        );
        let at_render: Vec<_> = set
            .resources
            .textures
            .iter()
            .filter(|t| (t.width(), t.height()) == (10, 10))
            .map(wgpu::Texture::format)
            .collect();
        let candidates = at_render
            .iter()
            .filter(|f| **f == crate::taa_thin_gpu::CANDIDATE_FORMAT)
            .count();
        assert_eq!(at_render.len(), 3 + usize::from(locks));
        assert_eq!(
            candidates,
            1 + usize::from(locks),
            "motion, and candidates if on"
        );
        assert_eq!(set.resources.buffers.len(), 1, "the reconstructed depth");
    }
    let own = sets
        .iter()
        .flat_map(|s| &s.resources.textures)
        .filter(|t| !history.contains(t))
        .count();
    assert_eq!(
        own,
        3 * (4 + 2 * usize::from(locks)),
        "per slot output, depth, motion, confidence, then candidates and lock bank"
    );
}

#[test]
fn the_temporal_bank_is_one_shared_history_and_one_output_per_slot_without_locks() {
    assert_bank(false);
}

#[test]
fn the_temporal_bank_adds_the_lock_resources_when_locks_are_on() {
    let _on = crate::taa_upscale_gpu::WithLocks::new();
    assert_bank(true);
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
    crate::hdr_taa::upload_temporal(&device, &mut encoder, &buffer, &block).unwrap();
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

/// The public factory a host builds: fixed exposure 1 and the Reinhard curve, validated by the
/// same rules a renderer applies.
pub(crate) fn public_factory(device: &wgpu::Device, settings: &HdrPostSettings) -> ToneMapOutput {
    ToneMapOutput::new(wgpu::TextureFormat::Rgba8UnormSrgb, 1.0, ToneMap::Reinhard)
        .unwrap()
        .with_effects(device, *settings)
        .unwrap()
}

fn full_chain_settings() -> HdrPostSettings {
    HdrPostSettings {
        adaptive: Some(crate::AdaptiveExposure {
            metering: crate::ExposureMetering::TrimmedMean,
            log_range: [-12.0, 12.0],
            percentiles: [0.1, 0.9],
            key: 0.18,
            exposure_range: [0.01, 64.0],
            speeds: [3.0, 1.0],
            fixed_delta: Some(0.25),
            fixed_exposure: None,
        }),
        bloom: Some(crate::Bloom {
            policies: crate::BloomPolicies::default(),
            threshold: 0.5,
            knee: 0.5,
            intensity: 0.25,
            levels: 3,
        }),
        sharpen: Some(crate::CasSettings { sharpness: 0.5 }),
        upscale: Some(UpscaleSettings {
            mode: UpscaleMode::Temporal,
            sharpness: Some(0.5),
        }),
        ..settings()
    }
}

/// Paints the render-extent scene a constant colour.
fn constant_scene(encoder: &mut wgpu::CommandEncoder, view: &wgpu::TextureView) {
    drop(encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("constant scene"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view,
            resolve_target: None,
            depth_slice: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color {
                    r: 0.5,
                    g: 0.25,
                    b: 0.125,
                    a: 1.0,
                }),
                store: wgpu::StoreOp::Store,
            },
        })],
        ..Default::default()
    }));
}

#[test]
fn a_temporal_chain_dispatches_every_stage_at_the_output_extent_and_writes_every_pixel() {
    let (device, queue) = crate::gpu_tests::gpu();
    let (render, output) = ([14, 14], [24, 24]);
    let config =
        FrameConfig::new(output, wgpu::TextureFormat::Rgba16Float, 1).with_render_extent(render);
    let factory = public_factory(&device, &full_chain_settings());
    let chain =
        crate::PostChain::new(&device, config, crate::Resources::default(), factory).unwrap();
    // A constant field at the render extent; the output must be the same field at O.
    let out = crate::color_texture(&device, "out", output, wgpu::TextureFormat::Rgba8UnormSrgb);
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    constant_scene(&mut encoder, chain.targets().resolved());
    let block = TemporalBlock {
        render,
        output,
        jitter: [0.25, 0.25],
        phase_count: 8,
        settings: TaaSettings::new(0.9),
        history_valid: false,
    };
    let factory = chain.factory();
    factory.prepare_temporal(&mut encoder, 0, &block).unwrap();
    factory
        .prepare_effects(&mut encoder, 0, 0.031, true)
        .unwrap();
    chain.encode(
        &mut encoder,
        &out.create_view(&wgpu::TextureViewDescriptor::default()),
    );
    queue.submit([encoder.finish()]);
    let bytes = crate::Readback::new(
        &device,
        FrameConfig::new(output, out.format(), 1),
        1_000_000,
    )
    .unwrap()
    .read(&device, &crate::test_authority(&queue), &out)
    .unwrap();
    assert_eq!(bytes.len(), 24 * 24 * 4);
    for (i, p) in bytes.chunks_exact(4).enumerate() {
        assert!(
            p[0] > 0 && p[3] == 255,
            "pixel {i} untouched or unlit: {p:?}"
        );
    }
}

#[test]
fn every_stage_of_a_temporal_chain_is_sized_by_the_output_extent() {
    let (device, _) = crate::gpu_tests::gpu();
    let post = HdrPost::new(&device, &full_chain_settings()).unwrap();
    let sets = slots(&post, &device, &targets(&device, [14, 14], [24, 24]));
    for set in &sets {
        let histogram = set
            .stages
            .iter()
            .find_map(|stage| match &stage.operation {
                crate::Operation::Compute { dispatch, .. } if stage.id.name == "histogram" => {
                    Some(*dispatch)
                }
                _ => None,
            })
            .expect("a metering dispatch");
        assert_eq!(histogram, [3, 3, 1], "ceil(24 / 8), not ceil(14 / 8) = 2");
        // The reconstruction targets at the render extent 14, reconstruction output and history, the display-linear input of RCAS, CAS input and the
        // first bloom level are full size; the other bloom levels halve from 24, never from 14.
        for texture in &set.resources.textures {
            let side = texture.width();
            assert!([24, 14, 12, 6].contains(&side), "a texture of side {side}");
        }
        assert!(
            set.resources
                .textures
                .iter()
                .filter(|t| t.width() == 24)
                .count()
                >= 5,
            "output, history, tone output, CAS and RCAS inputs and the first bloom level"
        );
    }
}
