//! HDR post wiring of the TAA resolve at the stage-factory level. `FrameRenderer` refuses TAA
//! until it uploads the block, so these build `ToneMapOutput` directly. They do not establish
//! the resolve output on a real camera.
use crate::chain::post_effect_tests::{adaptive, output, paint, pixels};
use crate::{
    Bloom, FrameConfig, FrameTargets, HdrPostSettings, PostChain, PostResolve, Resources,
    StageFactory, StageSet, ToneMap, ToneMapOutput,
};

fn settings(taa: bool, bloom: bool) -> HdrPostSettings {
    HdrPostSettings {
        adaptive: Some(adaptive()),
        bloom: bloom.then_some(Bloom {
            policies: crate::BloomPolicies::default(),
            threshold: 1.0,
            knee: 0.5,
            intensity: 0.25,
            levels: 3,
        }),
        resolve: if taa {
            PostResolve::taa(0.9)
        } else {
            PostResolve::None
        },
        ..Default::default()
    }
}

fn factory(device: &wgpu::Device, settings: &HdrPostSettings) -> ToneMapOutput {
    ToneMapOutput::new(wgpu::TextureFormat::Rgba8UnormSrgb, 1.0, ToneMap::Reinhard)
        .unwrap()
        .with_effects(device, *settings)
        .unwrap()
}

fn targets(device: &wgpu::Device, size: [u32; 2], samples: u32) -> FrameTargets {
    FrameTargets::new(
        device,
        FrameConfig::new(size, wgpu::TextureFormat::Rgba16Float, samples),
    )
    .unwrap()
}

fn slots(factory: &ToneMapOutput, device: &wgpu::Device, targets: &FrameTargets) -> Vec<StageSet> {
    let sets = (0..factory.frame_slots())
        .map(|slot| {
            factory
                .build_for_slot(device, targets, &Resources::default(), slot)
                .unwrap()
        })
        .collect();
    factory.settle_build(true);
    sets
}

fn names(set: &StageSet) -> Vec<&'static str> {
    set.stages.iter().map(|stage| stage.id.name).collect()
}

#[test]
fn a_non_taa_stage_list_is_unchanged_and_allocates_no_history() {
    let (device, _) = crate::gpu_tests::gpu();
    let factory = factory(&device, &settings(false, true));
    let sets = slots(&factory, &device, &targets(&device, [8, 8], 1));
    for set in &sets {
        assert_eq!(
            names(set),
            [
                "clear luminance histogram",
                "histogram",
                "reduce",
                "publish frame exposure",
                "prefilter",
                "downsample",
                "downsample",
                "upsample",
                "upsample",
                "tone"
            ]
        );
    }
    assert_eq!(factory.taa_history_size(), None);
}

#[test]
fn taa_stages_run_first_in_every_slot_before_metering() {
    let (device, _) = crate::gpu_tests::gpu();
    let factory = factory(&device, &settings(true, true));
    let sets = slots(&factory, &device, &targets(&device, [8, 8], 1));
    assert_eq!(sets.len(), 3);
    for set in &sets {
        assert_eq!(
            names(set)[..4],
            [
                "taa resolve",
                "taa history",
                "taa depth history",
                "clear luminance histogram"
            ]
        );
    }
}

#[test]
fn taa_with_bloom_and_adaptive_exposure_builds_at_one_and_four_samples() {
    let (device, _) = crate::gpu_tests::gpu();
    for (samples, bloom) in [(1, true), (4, true), (1, false), (4, false)] {
        let factory = factory(&device, &settings(true, bloom));
        let sets = slots(&factory, &device, &targets(&device, [9, 7], samples));
        assert_eq!(sets.len(), 3, "{samples}x bloom {bloom}");
        assert_eq!(factory.taa_history_size(), Some([9, 7]));
    }
}

fn shared_textures(sets: &[StageSet]) -> Vec<wgpu::Texture> {
    sets[0]
        .resources
        .textures
        .iter()
        .filter(|texture| {
            sets[1..]
                .iter()
                .all(|s| s.resources.textures.contains(texture))
        })
        .cloned()
        .collect()
}

#[test]
fn one_history_is_shared_by_all_slots_and_replaced_on_resize() {
    let (device, _) = crate::gpu_tests::gpu();
    let factory = factory(&device, &settings(true, false));
    let first = slots(&factory, &device, &targets(&device, [8, 8], 1));
    let shared = shared_textures(&first);
    assert_eq!(shared.len(), 2, "colour and depth history");
    assert_eq!(shared[0].format(), crate::taa_gpu::FORMAT);
    assert_eq!(shared[1].format(), crate::taa_gpu::DEPTH_FORMAT);
    assert!(shared.iter().all(|t| (t.width(), t.height()) == (8, 8)));
    let rebuilt = slots(&factory, &device, &targets(&device, [8, 8], 1));
    assert_eq!(
        shared_textures(&rebuilt),
        shared,
        "same size reuses history"
    );
    let resized = slots(&factory, &device, &targets(&device, [16, 4], 1));
    let replaced = shared_textures(&resized);
    assert_eq!(replaced.len(), 2);
    assert!(replaced.iter().all(|t| (t.width(), t.height()) == (16, 4)));
    assert!(replaced.iter().all(|t| !shared.contains(t)));
    assert_eq!(factory.taa_history_size(), Some([16, 4]));
}

fn tone_row(device: &wgpu::Device, queue: &wgpu::Queue, settings: &HdrPostSettings) -> Vec<u8> {
    // Half a pixel of jitter and no history: pixels on the step average the two samples that
    // are equally near their centre, which the unjittered scene color never does.
    let block = crate::TaaUniform {
        reprojection: [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ],
        jitter_pixels: [0.5, 0.0],
        history_valid: false,
        depth_terms: [0.0; 2],
    }
    .to_bytes();
    let chain = PostChain::new(
        device,
        FrameConfig::new([8, 4], wgpu::TextureFormat::Rgba16Float, 1),
        Resources::default(),
        factory(device, settings),
    )
    .unwrap();
    let out = output(device, [8, 4]);
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    paint(
        device,
        &mut encoder,
        chain.targets().resolved(),
        "return vec4(select(0.0, 4.0, p.x >= 4.0), 0.0, 0.0, 1.0);",
    );
    chain
        .factory()
        .prepare_effects(&mut encoder, 0, 0.031, true)
        .unwrap();
    chain.factory().prepare_taa(&mut encoder, 0, Some(&block));
    chain.encode(
        &mut encoder,
        &out.create_view(&wgpu::TextureViewDescriptor::default()),
    );
    queue.submit([encoder.finish()]);
    pixels(device, queue, &out)
}

fn red(bytes: &[u8], x: usize) -> u8 {
    bytes[(8 * 2 + x) * 4]
}

#[test]
fn tone_mapping_reads_the_taa_output_not_the_raw_scene_color() {
    let (device, queue) = crate::gpu_tests::gpu();
    let plain = tone_row(&device, &queue, &settings(false, false));
    let taa = tone_row(&device, &queue, &settings(true, false));
    // Without TAA the step is exact; the jittered resolve reconstructs the pixel on the step
    // from both of its nearest samples.
    assert_eq!(red(&plain, 3), red(&plain, 0));
    assert!(
        red(&taa, 3) > red(&taa, 0),
        "{} {}",
        red(&taa, 3),
        red(&taa, 0)
    );
}

#[test]
fn rebinding_settings_without_taa_releases_both_histories() {
    let (device, _) = crate::gpu_tests::gpu();
    let with_taa = factory(&device, &settings(true, false));
    let _sets = slots(&with_taa, &device, &targets(&device, [8, 8], 1));
    assert_eq!(with_taa.taa_history_size(), Some([8, 8]));
    let without = ToneMapOutput::new(wgpu::TextureFormat::Rgba8UnormSrgb, 1.0, ToneMap::Reinhard)
        .unwrap()
        .with_retained_effects(&device, settings(false, true), Some(&with_taa))
        .unwrap();
    let _built = slots(&without, &device, &targets(&device, [8, 8], 1));
    assert_eq!(without.taa_history_size(), None);
    assert_eq!(
        with_taa.taa_history_size(),
        None,
        "the shared history is gone"
    );
}

/// A factory whose last slot can be made to fail after the earlier slots have built.
struct FailsLast {
    inner: ToneMapOutput,
    fail: std::cell::Cell<bool>,
}
impl StageFactory for FailsLast {
    fn validate_config(
        &self,
        device: &wgpu::Device,
        config: FrameConfig,
    ) -> Result<(), crate::PostError> {
        self.inner.validate_config(device, config)
    }
    fn frame_slots(&self) -> usize {
        self.inner.frame_slots()
    }
    fn needs_velocity(&self) -> bool {
        self.inner.needs_velocity()
    }
    fn needs_temporal_masks(&self) -> bool {
        self.inner.needs_temporal_masks()
    }
    fn settle_build(&self, accepted: bool) {
        self.inner.settle_build(accepted);
    }
    fn build_for_slot(
        &self,
        device: &wgpu::Device,
        targets: &FrameTargets,
        persistent: &Resources,
        slot: usize,
    ) -> Result<StageSet, crate::PostError> {
        if self.fail.get() && slot == 2 {
            return Err(crate::PostError::UnsupportedProfile("slot rejected"));
        }
        self.inner.build_for_slot(device, targets, persistent, slot)
    }
    fn build(
        &self,
        device: &wgpu::Device,
        targets: &FrameTargets,
        persistent: &Resources,
    ) -> Result<StageSet, crate::PostError> {
        self.inner.build(device, targets, persistent)
    }
}

#[test]
fn a_failed_replacement_keeps_the_committed_history_bank() {
    let (device, _) = crate::gpu_tests::gpu();
    let factory = FailsLast {
        inner: factory(&device, &settings(true, false)),
        fail: std::cell::Cell::new(false),
    };
    let config = FrameConfig::new([8, 8], wgpu::TextureFormat::Rgba16Float, 1);
    let mut chain = PostChain::new(&device, config, Resources::default(), factory).unwrap();
    assert_eq!(chain.factory().inner.taa_history_size(), Some([8, 8]));

    chain.factory().fail.set(true);
    assert!(chain.resize(&device, [16, 4]).is_err());
    assert_eq!(
        chain.factory().inner.taa_history_size(),
        Some([8, 8]),
        "the slots still run on the old history, so the bank must still be the old one"
    );

    chain.factory().fail.set(false);
    assert!(chain.resize(&device, [16, 4]).unwrap());
    assert_eq!(chain.factory().inner.taa_history_size(), Some([16, 4]));
}

#[test]
fn a_rejected_settings_change_that_turns_taa_off_keeps_the_committed_bank() {
    let (device, _) = crate::gpu_tests::gpu();
    let config = FrameConfig::new([8, 8], wgpu::TextureFormat::Rgba16Float, 1);
    let live = FailsLast {
        inner: factory(&device, &settings(true, false)),
        fail: std::cell::Cell::new(false),
    };
    let mut chain = PostChain::new(&device, config, Resources::default(), live).unwrap();
    let off = ToneMapOutput::new(wgpu::TextureFormat::Rgba8UnormSrgb, 1.0, ToneMap::Reinhard)
        .unwrap()
        .with_retained_effects(&device, settings(false, true), Some(&chain.factory().inner))
        .unwrap();
    let rejected = FailsLast {
        inner: off,
        fail: std::cell::Cell::new(true),
    };
    assert!(chain.reconfigure(&device, rejected).is_err());
    assert_eq!(
        chain.factory().inner.taa_history_size(),
        Some([8, 8]),
        "the live slots still run on the committed bank"
    );
}
