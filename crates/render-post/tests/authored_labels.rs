//! CPU-only construction of real authored schedules with wgpu's no-op backend.
use worldrunner_render_post::{
    Bloom, BloomPolicies, BloomPyramid, FrameConfig, FrameTargets, GaussianLevel, PassId,
    Resources, StageFactory, StageSet, ToneMap, ToneMapOutput,
};

fn reported_name(id: PassId) -> String {
    id.to_string()
}

fn targets(device: &wgpu::Device) -> FrameTargets {
    FrameTargets::new(
        device,
        FrameConfig {
            size: [32, 32],
            output_size: None,
            format: wgpu::TextureFormat::Rgba16Float,
            samples: 1,
        },
    )
    .unwrap()
}

#[test]
fn real_bloom_levels_match_the_reported_names() {
    let (device, _) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
    let targets = targets(&device);
    let mut set = StageSet::default();
    BloomPyramid {
        bloom: Bloom {
            policies: BloomPolicies::default(),
            threshold: 1.0,
            knee: 0.5,
            intensity: 1.0,
            levels: 3,
        },
        levels: [GaussianLevel {
            radius: 4,
            tint: [1.0; 3],
            weight: 1.0,
        }; 8],
    }
    .append(&device, &mut set, targets.resolved(), None)
    .unwrap();
    let stages: Vec<_> = set
        .stages
        .iter()
        .filter(|stage| stage.id.name == "gaussian_downsample")
        .collect();
    assert_eq!(stages.len(), 2);
    for (stage, expected) in stages
        .iter()
        .zip(["gaussian_downsample level 1", "gaussian_downsample level 2"])
    {
        let id = PassId::Stage(stage.id);
        assert_eq!(reported_name(id), expected);
        assert_eq!(stage.encoder_label(), expected);
        assert_eq!(id.encoder_label().as_ref(), reported_name(id));
    }
}

#[test]
fn fixed_tone_stage_matches_the_reported_name() {
    let (device, _) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
    let targets = targets(&device);
    let set = ToneMapOutput::new(wgpu::TextureFormat::Rgba8UnormSrgb, 1.0, ToneMap::Reinhard)
        .unwrap()
        .build(&device, &targets, &Resources::default())
        .unwrap();
    assert_eq!(set.stages.len(), 1);
    let stage = &set.stages[0];
    let id = PassId::Stage(stage.id);
    assert_eq!(reported_name(id), "tone");
    assert_eq!(stage.encoder_label(), reported_name(id));
    assert_eq!(id.encoder_label().as_ref(), reported_name(id));
}
