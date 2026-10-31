use crate::{CaptureJitter, CaptureOutput, CapturePoolPolicy, RetainedCaptureRecipe, ViewerError};

fn recipe() -> RetainedCaptureRecipe {
    RetainedCaptureRecipe::new(
        2,
        1,
        CaptureJitter::None,
        false,
        vec![CaptureOutput::Srgb8],
        u64::MAX,
    )
    .unwrap()
}

fn derived(cap_bytes: Option<u64>) -> CapturePoolPolicy {
    CapturePoolPolicy::Derived {
        recipes: vec![recipe()],
        max_extent: [2560, 1440],
        headroom_bytes: 4096,
        granularity_bytes: 65536,
        cap_bytes,
    }
}

fn plan(
    options: &crate::ViewerOptions,
    capture: crate::PresentationCapture<'_>,
) -> Result<u64, ViewerError> {
    Ok(crate::plan_presentation(
        options.size,
        options,
        None,
        &wgpu::Limits::default(),
        wgpu::Features::empty(),
        crate::PresentationPlanning {
            capture,
            retained_effect_history: false,
        },
    )?
    .validate(&wgpu::Limits::default())
    .unwrap())
}

#[test]
fn default_pool_preserves_inventory_and_typed_rejection() {
    let options = crate::ViewerOptions {
        size: [2560, 1440],
        ..crate::tests::options()
    };
    assert_eq!(options.capture_pool, CapturePoolPolicy::Fixed(268_435_456));
    assert_eq!(
        crate::renderer::Startup::from(&options).capture_pool,
        268_435_456
    );
    let retained = plan(&options, crate::PresentationCapture::Retained).unwrap();
    let ordinary = plan(&options, crate::PresentationCapture::Ordinary).unwrap();
    assert_eq!(retained - ordinary, 268_435_456);
    assert_eq!(
        recipe().pool_bytes(options.size, 268_435_456, 1),
        Err(crate::CaptureRequestError::RecipeBudget {
            required: 412_876_800,
            limit: 268_435_456,
        })
    );
    assert!(matches!(
        plan(&options, crate::PresentationCapture::Recipe(&recipe())),
        Err(ViewerError::Capture(
            crate::CaptureRequestError::RecipeBudget {
                limit: 268_435_456,
                ..
            }
        ))
    ));
}

#[test]
fn derived_large_recipe_is_admitted_and_inventory_matches_pool() {
    let options = crate::ViewerOptions {
        size: [2560, 1440],
        capture_pool: derived(None),
        ..crate::tests::options()
    };
    let required = recipe().storage_bytes(options.size).unwrap();
    assert_eq!(required, 412_876_800);
    let expected = (required + 4096).next_multiple_of(65536);
    let retained = plan(&options, crate::PresentationCapture::Retained).unwrap();
    let ordinary = plan(&options, crate::PresentationCapture::Ordinary).unwrap();
    assert_eq!(retained - ordinary, expected);
    plan(&options, crate::PresentationCapture::Recipe(&recipe())).unwrap();
    let mut host = crate::RetainedHost::new(crate::tests::scene(false), options).unwrap();
    assert_eq!(host.request_capture_recipe(0, recipe()), Ok(0));
}

#[test]
fn capped_pool_rejects_identically_in_planning_and_admission() {
    let options = crate::ViewerOptions {
        size: [2560, 1440],
        capture_pool: derived(Some(1024)),
        ..crate::tests::options()
    };
    let expected = crate::CaptureRequestError::RecipeBudget {
        required: recipe().storage_bytes(options.size).unwrap(),
        limit: 1024,
    };
    assert!(
        matches!(plan(&options, crate::PresentationCapture::Recipe(&recipe())),
        Err(ViewerError::Capture(error)) if error == expected)
    );
    let mut host = crate::RetainedHost::new(crate::tests::scene(false), options).unwrap();
    assert_eq!(host.request_capture_recipe(0, recipe()), Err(expected));
}

#[test]
fn capture_pool_maximum_extent_does_not_enforce_request_byte_limit() {
    let request = RetainedCaptureRecipe::new(
        1,
        1,
        CaptureJitter::None,
        false,
        vec![CaptureOutput::Srgb8],
        20_480,
    )
    .unwrap();
    assert_eq!(request.storage_bytes([32, 32]), Ok(20_480));
    let derived = CapturePoolPolicy::Derived {
        recipes: vec![recipe()],
        max_extent: [64, 64],
        headroom_bytes: 0,
        granularity_bytes: 1,
        cap_bytes: None,
    };
    let capacity = derived
        .effective_bytes(1, wgpu::TextureFormat::R32Float)
        .unwrap();
    assert_eq!(capacity, 458_752);
    for capture_pool in [
        derived,
        CapturePoolPolicy::Fixed(capacity),
        CapturePoolPolicy::default(),
    ] {
        let options = crate::ViewerOptions {
            size: [32, 32],
            frame_profile: crate::FrameProfile::Legacy,
            capture_pool,
            ..crate::tests::options()
        };
        plan(&options, crate::PresentationCapture::Recipe(&request)).unwrap();
        let mut host = crate::RetainedHost::new(crate::tests::scene(false), options).unwrap();
        assert_eq!(host.request_capture_recipe(0, request.clone()), Ok(0));
    }
}

#[test]
fn derived_pool_is_constant_across_smaller_and_larger_surfaces() {
    let options = crate::ViewerOptions {
        capture_pool: derived(None),
        size: [2560, 1440],
        ..crate::tests::options()
    };
    let expected = (412_876_800_u64 + 4096).next_multiple_of(65536);
    let mut host = crate::RetainedHost::new(crate::tests::scene(false), options).unwrap();
    assert_eq!(host.capture_pool_bytes(), expected);
    let id = host.request_capture_recipe(0, recipe()).unwrap();
    host.captures.cancel();
    assert!(
        matches!(host.captures.take_event(), Some(crate::RetainedEvent::Capture { id: actual, .. }) if actual == id)
    );
    let mut driver = Driver;
    let mut run = host.windowless(&mut driver).unwrap();
    for extent in [[128, 96], [3000, 1600]] {
        run.resize(extent).unwrap();
        assert!(run.frame().unwrap());
        assert_eq!(
            run.host().statistics().unwrap().capture_pool_bytes,
            expected
        );
        assert_eq!(run.host().capture_pool_bytes(), expected);
    }
    run.shutdown().unwrap();
}

#[test]
fn derived_pool_rejects_growth_beyond_capacity_with_typed_error() {
    let options = crate::ViewerOptions {
        capture_pool: derived(None),
        size: [2560, 1440],
        ..crate::tests::options()
    };
    let mut host = crate::RetainedHost::new(crate::tests::scene(false), options.clone()).unwrap();
    host.input(crate::RetainedInput::Resized([3000, 1600]));
    let expected = crate::CaptureRequestError::RecipeBudget {
        required: recipe().storage_bytes([3000, 1600]).unwrap(),
        limit: (412_876_800_u64 + 4096).next_multiple_of(65536),
    };
    assert_eq!(host.request_capture_recipe(0, recipe()), Err(expected));
    host.input(crate::RetainedInput::Resized([2800, 1200]));
    assert_eq!(host.request_capture_recipe(0, recipe()), Ok(0));
    let mut options = options;
    options.size = [3000, 1600];
    assert!(
        matches!(plan(&options, crate::PresentationCapture::Recipe(&recipe())),
        Err(ViewerError::Capture(error)) if error == expected)
    );
}

struct Driver;
impl crate::RetainedDriver for Driver {
    fn update(
        &mut self,
        _host: &mut crate::RetainedHost,
        _tick: &crate::RetainedTick,
    ) -> Result<(), String> {
        Ok(())
    }
}

#[test]
fn invalid_capture_policies_fail_before_host_startup() {
    let invalid = [
        CapturePoolPolicy::Fixed(0),
        CapturePoolPolicy::Derived {
            recipes: vec![],
            max_extent: [1, 1],
            headroom_bytes: 0,
            granularity_bytes: 1,
            cap_bytes: None,
        },
        CapturePoolPolicy::Derived {
            recipes: vec![recipe()],
            max_extent: [2560, 1440],
            headroom_bytes: 0,
            granularity_bytes: 0,
            cap_bytes: None,
        },
        CapturePoolPolicy::Derived {
            recipes: vec![recipe()],
            max_extent: [2560, 1440],
            headroom_bytes: 0,
            granularity_bytes: 1,
            cap_bytes: Some(0),
        },
    ];
    for extent in [[0, 1], [1, 0], [0, 0]] {
        let capture_pool = CapturePoolPolicy::Derived {
            recipes: vec![recipe()],
            max_extent: extent,
            headroom_bytes: 0,
            granularity_bytes: 1,
            cap_bytes: None,
        };
        assert!(matches!(
            capture_pool.validate(),
            Err(ViewerError::InvalidOptions(_))
        ));
    }
    for capture_pool in invalid {
        let options = crate::ViewerOptions {
            capture_pool,
            ..crate::tests::options()
        };
        assert!(matches!(
            crate::RetainedHost::new(crate::tests::scene(false), options),
            Err(ViewerError::InvalidOptions(_))
        ));
    }
}

#[test]
fn recipe_set_uses_largest_effective_storage_and_checked_rounding() {
    let small = RetainedCaptureRecipe::new(
        1,
        1,
        CaptureJitter::None,
        false,
        vec![CaptureOutput::AmbientOcclusion],
        u64::MAX,
    )
    .unwrap();
    let mut large = RetainedCaptureRecipe::new(
        1,
        1,
        CaptureJitter::None,
        false,
        vec![CaptureOutput::SsrConfidence],
        u64::MAX,
    )
    .unwrap();
    let policy = CapturePoolPolicy::Derived {
        recipes: vec![small, large.clone()],
        max_extent: [33, 2],
        headroom_bytes: 7,
        granularity_bytes: 16,
        cap_bytes: None,
    };
    large.frame_samples = 4;
    let required = large.storage_bytes([33, 2]).unwrap();
    let bytes = policy
        .effective_bytes(4, wgpu::TextureFormat::Rg32Float)
        .unwrap();
    assert_eq!(bytes, (required + 7).next_multiple_of(16));
    let overflow = CapturePoolPolicy::Derived {
        recipes: vec![recipe()],
        max_extent: [2560, 1440],
        headroom_bytes: u64::MAX,
        granularity_bytes: 1,
        cap_bytes: None,
    };
    assert_eq!(
        overflow.effective_bytes(1, wgpu::TextureFormat::R32Float),
        Err(crate::CaptureRequestError::InvalidRecipe)
    );
}

#[test]
fn recipe_raster_override_preserves_declared_surface_pool() {
    use super::{COLOR, Presentation, PresentationPolicy};
    use worldrunner_render_scene::{Viewport, device::GpuContext};
    let context = pollster::block_on(GpuContext::request()).unwrap();
    let size = Viewport {
        width: 32,
        height: 24,
    };
    let raster = Viewport {
        width: 64,
        height: 48,
    };
    let mut presentation = Presentation::with_settings(
        &context,
        COLOR,
        [size, size],
        crate::FrameProfile::Legacy,
        None,
        &PresentationPolicy {
            budget: crate::PresentationBudgetPolicy::Derived {
                headroom_bytes: 0,
                granularity_bytes: 1,
                cap_bytes: None,
            },
            recipes: true,
            capture_pool: derived(None),
        },
    )
    .unwrap();
    let before = presentation.capture_pool_bytes();
    presentation.resize_recipe(&context, raster, None).unwrap();
    assert_eq!(presentation.capture_pool_bytes(), before);
    presentation.resize(&context, raster, raster).unwrap();
    assert_eq!(presentation.capture_pool_bytes(), before);
}

#[test]
fn derived_capture_pool_keeps_initial_effect_format_on_configuration() {
    use super::{COLOR, Presentation, PresentationPolicy};
    use worldrunner_render_scene::{Viewport, device::GpuContext};
    let recipe = RetainedCaptureRecipe::new(
        1,
        1,
        CaptureJitter::None,
        false,
        vec![CaptureOutput::AmbientOcclusion],
        u64::MAX,
    )
    .unwrap();
    let pool = CapturePoolPolicy::Derived {
        recipes: vec![recipe],
        max_extent: [33, 2],
        headroom_bytes: 0,
        granularity_bytes: 1,
        cap_bytes: None,
    };
    let context = pollster::block_on(GpuContext::request()).unwrap();
    let size = Viewport {
        width: 33,
        height: 2,
    };
    let mut presentation = Presentation::with_settings(
        &context,
        COLOR,
        [size, size],
        crate::FrameProfile::Hdr {
            samples: 1,
            tone_map: crate::ToneMap::Reinhard,
        },
        None,
        &PresentationPolicy {
            budget: crate::PresentationBudgetPolicy::Derived {
                headroom_bytes: 0,
                granularity_bytes: 1,
                cap_bytes: None,
            },
            recipes: true,
            capture_pool: pool,
        },
    )
    .unwrap();
    assert_eq!(presentation.capture_pool_bytes(), 1552);
    let mut settings = presentation.scheduler.configuration().clone();
    settings.ssao = Some(gather());
    presentation
        .scheduler
        .reconfigure(&context, settings)
        .unwrap();
    assert_eq!(presentation.capture_pool_bytes(), 1552);
}

fn gather() -> worldrunner_render_plan::SceneSsao {
    use worldrunner_render_plan::*;
    SceneSsao {
        receiver: SceneSsaoReceiver::DepthAwareGather,
        depth_footprint: SceneSsaoDepthFootprint::default(),
        kernel: SceneSsaoKernel::Hemisphere,
        filter: SceneSsaoFilter::DepthAwareBox,
        sample_count: 4,
        bias_millimeters: 1,
        radius_millimeters: 100,
        intensity_micro: 1_000_000,
        horizon_angle_millidegrees: 90_000,
    }
}
