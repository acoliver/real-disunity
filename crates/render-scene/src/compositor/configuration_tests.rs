#[test]
fn reflection_mip_inventory_counts_physical_storage_once() {
    use worldrunner_render_plan::{SceneSsr, SceneSsrPyramid};
    let limits = wgpu::Limits::default();
    for (size, samples) in [([257, 5], 1), ([257, 5], 4), ([5, 257], 4), ([1, 1], 1)] {
        let config = FrameConfig::new(size, wgpu::TextureFormat::Rgba16Float, samples);
        let policy = SceneSsr {
            radiance_pyramid: SceneSsrPyramid::Full,
            ..Default::default()
        };
        let mut set = AllocationSet::default();
        radiance_allocations(config, &policy, &mut set);
        let logical = policy.radiance_extents(size);
        let [width, height] = crate::ssr::pyramid::storage_extent(config, &policy).unwrap();
        let mut bytes = 0u64;
        for level in 0..logical.len() {
            bytes += u64::from((width >> level).max(1)) * u64::from((height >> level).max(1)) * 8;
        }
        bytes += u64::try_from(logical.len() - 1).unwrap() * 16;
        assert_eq!(set.validate(&limits).unwrap(), bytes);
    }
}

use super::*;
use worldrunner_render_gpu::configuration::AllocationBudget;
use worldrunner_render_post::{AdaptiveExposure, BufferReadback, Readback};

#[test]
fn initial_allocation_reports_loss_before_success_or_validation_failure() {
    for fail in [false, true] {
        let context = pollster::block_on(GpuContext::request()).unwrap();
        let settings = RendererConfiguration::legacy([5, 3], wgpu::TextureFormat::Rgba8UnormSrgb);
        let result = ConfiguredFrameRenderer::new_with(
            &context,
            settings,
            AllocationBudget(4096),
            |settings| {
                let renderer = allocate(&context, settings, None, None);
                context
                    .submission()
                    .report_device_loss("injected allocation loss".into());
                if fail {
                    Err(
                        worldrunner_render_gpu::configuration::DriverAllocationError::Validation(
                            "invalid handle after loss".into(),
                        )
                        .into(),
                    )
                } else {
                    renderer
                }
            },
        );
        assert!(matches!(
            result,
            Err(ConfigurationError::Submission(
                worldrunner_render_gpu::submission::SubmitError::DeviceLost(_)
            ))
        ));
        assert!(context.submission().is_idle());
    }
}

#[test]
fn foreign_device_is_rejected_before_validation_or_allocation() {
    let owner = pollster::block_on(GpuContext::request()).unwrap();
    let foreign = pollster::block_on(GpuContext::request()).unwrap();
    let settings = RendererConfiguration::legacy([5, 3], wgpu::TextureFormat::Rgba8UnormSrgb);
    let mut renderer =
        ConfiguredFrameRenderer::new(&owner, settings.clone(), AllocationBudget(4096)).unwrap();
    let before = renderer.targets().texture().clone();
    let prepared = foreign
        .submission()
        .prepare(foreign.device(), |_, _| Ok::<_, ()>(()))
        .unwrap();
    let mut invalid = settings.clone();
    invalid.frame.size = [0, 0];
    assert!(matches!(
        renderer.reconfigure(&foreign, invalid),
        Err(ConfigurationError::ForeignDevice)
    ));
    assert!(matches!(
        renderer.reconfigure(&foreign, settings.clone()),
        Err(ConfigurationError::ForeignDevice)
    ));
    assert_eq!(renderer.targets().texture(), &before);
    assert_eq!(renderer.generation(), 0);
    assert_eq!(renderer.configuration().frame.size, [5, 3]);
    assert!(owner.submission().is_idle());
    prepared.cancel();
    renderer.reconfigure(&owner, settings).unwrap();
    assert_eq!(renderer.generation(), 1);
}

#[test]
fn timing_changes_are_rejected_without_changing_inventory() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    let settings = RendererConfiguration::legacy([5, 3], wgpu::TextureFormat::Rgba8UnormSrgb);
    let mut renderer =
        ConfiguredFrameRenderer::new(&context, settings.clone(), AllocationBudget(4096)).unwrap();
    let before = renderer.targets().texture().clone();
    let mut timing = settings;
    timing.timing = TimingMode::Required;
    assert!(matches!(
        renderer.reconfigure(&context, timing),
        Err(ConfigurationError::Invalid(_))
    ));
    assert_eq!(renderer.configuration().timing, TimingMode::Disabled);
    assert_eq!(renderer.targets().texture(), &before);
    assert_eq!(renderer.generation(), 0);
}

#[test]
fn stock_inventory_matches_hand_computed_bytes() {
    let settings = RendererConfiguration::legacy([5, 3], wgpu::TextureFormat::Rgba8UnormSrgb);
    // One resolved RGBA8 target and one depth32 target.
    assert_eq!(
        settings
            .allocations()
            .validate(&wgpu::Limits::default())
            .unwrap(),
        5 * 3 * 8
    );
    let mut hdr = settings;
    hdr.hdr = Some((1.0, worldrunner_render_post::ToneMap::Reinhard));
    hdr.frame.format = wgpu::TextureFormat::Rgba16Float;
    hdr.frame.samples = 4;
    assert_eq!(
        hdr.allocations()
            .validate(&wgpu::Limits::default())
            .unwrap(),
        5 * 3 * (8 + 8 * 4 + 4 * 4)
    );
}
#[test]
fn replacement_preserves_history_and_rejection_preserves_targets() {
    let context = pollster::block_on(crate::device::GpuContext::request()).unwrap();
    let mut settings = RendererConfiguration::legacy([5, 3], wgpu::TextureFormat::Rgba8UnormSrgb);
    let mut renderer =
        ConfiguredFrameRenderer::new(&context, settings.clone(), AllocationBudget(4096)).unwrap();
    let before = renderer.targets().texture().clone();
    settings.frame.size = [100, 100];
    assert!(matches!(
        renderer.reconfigure(&context, settings),
        Err(ConfigurationError::Budget { .. })
    ));
    assert_eq!(renderer.targets().texture(), &before);
    assert_eq!(renderer.generation(), 0);
    let mut settings = renderer.configuration().clone();
    settings.frame.size = [7, 3];
    renderer.reconfigure(&context, settings).unwrap();
    assert_eq!(renderer.targets().config().size, [7, 3]);
    assert_eq!(renderer.generation(), 1);
}

#[test]
fn exposure_history_and_stock_output_survive_replacement() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    let mut settings = RendererConfiguration::legacy([5, 3], wgpu::TextureFormat::Rgba8UnormSrgb);
    settings.frame.format = wgpu::TextureFormat::Rgba16Float;
    settings.hdr = Some((1.0, ToneMap::Reinhard));
    settings.post.adaptive = Some(adaptive_settings());
    let mut control =
        ConfiguredFrameRenderer::new(&context, settings.clone(), AllocationBudget(100_000))
            .unwrap();
    let mut candidate =
        ConfiguredFrameRenderer::new(&context, settings.clone(), AllocationBudget(100_000))
            .unwrap();
    candidate.reset_exposure();
    control.reset_exposure();
    assert_eq!(
        capture(&context, &mut control),
        capture(&context, &mut candidate)
    );
    let mut rejected = settings.clone();
    rejected.frame.size = [1000, 1000];
    assert!(matches!(
        candidate.reconfigure(&context, rejected),
        Err(ConfigurationError::Budget { .. })
    ));
    assert_eq!(
        capture(&context, &mut control),
        capture(&context, &mut candidate)
    );
    let old_exposure = candidate
        .history
        .as_ref()
        .unwrap()
        .exposure_buffer(0)
        .unwrap()
        .clone();
    candidate.reconfigure(&context, settings.clone()).unwrap();
    assert_eq!(
        candidate
            .history
            .as_ref()
            .unwrap()
            .exposure_buffer(0)
            .unwrap(),
        &old_exposure
    );
    let expected = capture(&context, &mut control);
    assert_eq!(expected, capture(&context, &mut candidate));
    settings.frame.size = [9, 7];
    candidate.reconfigure(&context, settings).unwrap();
    let (control_bytes, exposure) = capture(&context, &mut control);
    let (candidate_bytes, resized_exposure) = capture(&context, &mut candidate);
    assert_eq!(exposure, resized_exposure);
    assert_eq!(&control_bytes[..4], &candidate_bytes[..4]);
}
fn capture(context: &GpuContext, renderer: &mut ConfiguredFrameRenderer) -> (Vec<u8>, Vec<u8>) {
    let device = context.device();
    let frame = renderer.configuration().frame;
    let output_config = FrameConfig::new(frame.size, wgpu::TextureFormat::Rgba8UnormSrgb, 1);
    let output = worldrunner_render_post::color_texture(
        device,
        "configuration output",
        frame.size,
        output_config.format,
    );
    let mut image = Readback::new(device, output_config, 100_000).unwrap();
    let mut exposure = BufferReadback::new(device, 16, 16).unwrap();
    renderer.set_frame_delta(0.25).unwrap();
    let prepared = context
        .submission()
        .prepare(device, |encoder, _| {
            clear_scene(encoder, renderer);
            renderer
                .encode_output(
                    encoder,
                    &output.create_view(&wgpu::TextureViewDescriptor::default()),
                )
                .unwrap();
            image.encode(encoder, &output).unwrap();
            exposure
                .encode(
                    encoder,
                    renderer
                        .history
                        .as_ref()
                        .unwrap()
                        .exposure_buffer(0)
                        .unwrap(),
                )
                .unwrap();
            Ok::<_, ()>(())
        })
        .unwrap();
    let submission = prepared.submit().unwrap();
    device.poll(wgpu::PollType::Poll).unwrap();
    (
        image
            .read_encoded(device, Some(submission.clone()))
            .unwrap(),
        exposure.read_encoded(device, Some(submission)).unwrap(),
    )
}

fn clear_scene(encoder: &mut wgpu::CommandEncoder, renderer: &ConfiguredFrameRenderer) {
    let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: renderer.targets().scene(),
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color {
                    r: 0.1875,
                    g: 0.1875,
                    b: 0.1875,
                    a: 1.0,
                }),
                store: wgpu::StoreOp::Store,
            },
        })],
        ..Default::default()
    });
}

#[test]
fn reflection_maps_and_composition_survive_both_replacement_paths() {
    use crate::sky::preparation::EnvironmentPreparation;
    use worldrunner_render_lighting::reflections::Composition;
    let context = pollster::block_on(GpuContext::request()).unwrap();
    let preparation = EnvironmentPreparation {
        convention: worldrunner_render_plan::SceneSpecularConvention::default(),
        cube_size: 2,
        source_lod_tail: 1,
        mip_count: 2,
        specular_intensity_micro: 1_000_000,
        lut_size: 1,
        samples: 8,
    };
    let maps = reflection_maps(&context, preparation);
    let views = maps.views().unwrap();
    let mut settings = RendererConfiguration::legacy([5, 3], wgpu::TextureFormat::Rgba8UnormSrgb);
    settings.frame.format = wgpu::TextureFormat::Rgba16Float;
    settings.hdr = Some((1.0, ToneMap::Reinhard));
    settings.ssr = Some(worldrunner_render_plan::SceneSsr {
        enabled: true,
        max_steps: 64,
        thickness_millimeters: 100,
        roughness_cutoff_micro: 600_000,
        resolution_scale_micro: 1_000_000,
        ..Default::default()
    });
    let base = settings
        .allocations()
        .validate(&context.device().limits())
        .unwrap();
    let external = preparation
        .allocations()
        .unwrap()
        .validate(&context.device().limits())
        .unwrap();
    settings.additional = preparation.allocations().unwrap();
    assert_eq!(
        settings
            .allocations()
            .validate(&context.device().limits())
            .unwrap(),
        base + external
    );
    let mut renderer =
        ConfiguredFrameRenderer::new(&context, settings.clone(), AllocationBudget(100_000))
            .unwrap();
    renderer
        .set_reflection_environment(context.device(), &maps)
        .unwrap();
    let current = renderer.state.current_mut();
    current.resize(context.device(), [7, 5]).unwrap();
    let ssr = current.ssr.as_ref().unwrap();
    assert_eq!(ssr.environment[0].texture(), views.0.texture());
    assert_eq!(ssr.environment[1].texture(), views.1.texture());
    assert_eq!(ssr.composition, Composition::ReplaceEnvironment);
    settings.frame.size = [9, 7];
    renderer.reconfigure(&context, settings).unwrap();
    let ssr = renderer.state.current().ssr.as_ref().unwrap();
    assert_eq!(ssr.environment[0].texture(), views.0.texture());
    assert_eq!(ssr.environment[1].texture(), views.1.texture());
    assert_eq!(ssr.composition, Composition::ReplaceEnvironment);
}

fn reflection_maps(
    context: &GpuContext,
    preparation: crate::sky::preparation::EnvironmentPreparation,
) -> crate::sky::preparation::PreparedEnvironmentMaps {
    use crate::sky::preparation::encode;
    let source = crate::water::receivers::empty_environment(context.device());
    let mut encoder = context.submission().begin(context.device()).unwrap();
    let maps = encode(
        context.device(),
        &mut encoder,
        &source[0],
        preparation,
        AllocationBudget(4096),
    )
    .unwrap();
    encoder.finish().submit().unwrap();
    maps
}

fn adaptive_settings() -> AdaptiveExposure {
    AdaptiveExposure {
        metering: worldrunner_render_post::ExposureMetering::TrimmedMean,
        log_range: [-8.0, 8.0],
        percentiles: [0.0, 1.0],
        key: 0.5,
        exposure_range: [0.1, 8.0],
        speeds: [2.0, 1.0],
        fixed_delta: Some(0.25),
        fixed_exposure: None,
    }
}

#[test]
fn authored_reflection_composition_changes_override_retained_service_selection() {
    use worldrunner_render_lighting::reflections::Composition;
    use worldrunner_render_plan::{SceneSsr, SceneSsrComposition};
    let context = pollster::block_on(GpuContext::request()).unwrap();
    let mut settings = RendererConfiguration::legacy([5, 3], wgpu::TextureFormat::Rgba8UnormSrgb);
    settings.frame.format = wgpu::TextureFormat::Rgba16Float;
    settings.hdr = Some((1.0, ToneMap::Reinhard));
    settings.ssr = Some(SceneSsr::default());
    let mut renderer =
        ConfiguredFrameRenderer::new(&context, settings.clone(), AllocationBudget(100_000))
            .unwrap();
    assert_eq!(
        renderer.state.current().ssr.as_ref().unwrap().composition,
        Composition::Additive
    );
    for (selected, expected) in [
        (
            SceneSsrComposition::ReplaceEnvironment,
            Composition::ReplaceEnvironment,
        ),
        (SceneSsrComposition::Additive, Composition::Additive),
    ] {
        settings.ssr.as_mut().unwrap().composition = selected;
        renderer.reconfigure(&context, settings.clone()).unwrap();
        assert_eq!(
            renderer.state.current().ssr.as_ref().unwrap().composition,
            expected
        );
        renderer
            .state
            .current_mut()
            .resize(context.device(), [7, 5])
            .unwrap();
        assert_eq!(
            renderer.state.current().ssr.as_ref().unwrap().composition,
            expected
        );
    }
}
