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

/// A factory around `HdrPost` that does not apply the public "not yet scheduled" gate, so the
/// built chain can run. Only test code can construct it.
struct BelowTheGate(HdrPost);
impl crate::StageFactory for BelowTheGate {
    fn validate_config(&self, _: &wgpu::Device, _: FrameConfig) -> Result<(), crate::PostError> {
        Ok(())
    }
    fn frame_slots(&self) -> usize {
        3
    }
    fn needs_velocity(&self) -> bool {
        true
    }
    fn settle_build(&self, accepted: bool) {
        self.0.settle_build(accepted);
    }
    fn build_for_slot(
        &self,
        device: &wgpu::Device,
        targets: &FrameTargets,
        _: &crate::Resources,
        slot: usize,
    ) -> Result<StageSet, crate::PostError> {
        let display = (1.0, ToneMap::Reinhard);
        let format = wgpu::TextureFormat::Rgba8UnormSrgb;
        Ok(self.0.build(device, targets, slot, format, display))
    }
    fn build(
        &self,
        device: &wgpu::Device,
        targets: &FrameTargets,
        persistent: &crate::Resources,
    ) -> Result<StageSet, crate::PostError> {
        self.build_for_slot(device, targets, persistent, 0)
    }
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

#[test]
fn a_temporal_chain_dispatches_every_stage_at_the_output_extent_and_writes_every_pixel() {
    let (device, queue) = crate::gpu_tests::gpu();
    let (render, output) = ([14, 14], [24, 24]);
    let config =
        FrameConfig::new(output, wgpu::TextureFormat::Rgba16Float, 1).with_render_extent(render);
    let post = HdrPost::new(&device, &full_chain_settings()).unwrap();
    let chain = crate::PostChain::new(
        &device,
        config,
        crate::Resources::default(),
        BelowTheGate(post),
    )
    .unwrap();
    // A constant field at the render extent; the output must be the same field at O.
    let out = crate::color_texture(&device, "out", output, wgpu::TextureFormat::Rgba8UnormSrgb);
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    drop(encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("constant scene"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: chain.targets().resolved(),
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
    let block = TemporalBlock {
        render,
        output,
        jitter: [0.25, 0.25],
        phase_count: 8,
        settings: TaaSettings::new(0.9),
        history_valid: false,
    };
    chain
        .factory()
        .0
        .prepare_temporal(&mut encoder, 0, &block)
        .unwrap();
    chain
        .factory()
        .0
        .prepare(&mut encoder, 0, 0.031, true, 1.0)
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
        // Reconstruction output and history, the display-linear input of RCAS, CAS input and the
        // first bloom level are full size; the other bloom levels halve from 24, never from 14.
        for texture in &set.resources.textures {
            let side = texture.width();
            assert!([24, 12, 6].contains(&side), "a texture of side {side}");
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
