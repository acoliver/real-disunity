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
    (0..factory.frame_slots())
        .map(|slot| {
            factory
                .build_for_slot(device, targets, &Resources::default(), slot)
                .unwrap()
        })
        .collect()
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
            names(set)[..3],
            ["taa resolve", "clear luminance histogram", "histogram"]
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

/// The output texture of each slot, then the history texture each slot reads.
fn outputs_and_histories(sets: &[StageSet]) -> (Vec<wgpu::Texture>, Vec<wgpu::Texture>) {
    let pick = |index: usize| {
        sets.iter()
            .map(|set| set.resources.textures[index].clone())
            .collect()
    };
    (pick(0), pick(1))
}

#[test]
fn a_slot_reads_the_output_of_the_slot_before_it_and_writes_its_own() {
    let (device, _) = crate::gpu_tests::gpu();
    let factory = factory(&device, &settings(true, false));
    let sets = slots(&factory, &device, &targets(&device, [8, 8], 1));
    let (outputs, histories) = outputs_and_histories(&sets);
    assert_eq!(outputs.len(), 3);
    for slot in 0..3 {
        assert_eq!(histories[slot], outputs[(slot + 2) % 3], "slot {slot}");
        assert_eq!((outputs[slot].width(), outputs[slot].height()), (8, 8));
        assert!(
            !outputs[..slot].contains(&outputs[slot]),
            "distinct outputs"
        );
        let usage = outputs[slot].usage();
        assert!(!usage.contains(wgpu::TextureUsages::COPY_DST));
    }
}

#[test]
fn outputs_are_shared_by_slot_builds_and_replaced_on_resize() {
    let (device, _) = crate::gpu_tests::gpu();
    let factory = factory(&device, &settings(true, false));
    let first = outputs_and_histories(&slots(&factory, &device, &targets(&device, [8, 8], 1)));
    let rebuilt = outputs_and_histories(&slots(&factory, &device, &targets(&device, [8, 8], 1)));
    assert_eq!(rebuilt, first, "same size reuses the outputs");
    let (resized, _) =
        outputs_and_histories(&slots(&factory, &device, &targets(&device, [16, 4], 1)));
    for (new, old) in resized.iter().zip(&first.0) {
        assert_ne!(new, old);
        assert_eq!((new.width(), new.height()), (16, 4));
    }
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
