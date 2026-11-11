use super::*;
use worldrunner_render_gpu::configuration::ConfigurationError;

#[test]
fn receiver_inventory_reserves_only_the_configured_shadow_bias_domain() {
    use worldrunner_render_scene::{
        compositor::ReceiverGradientReservation, shadows::DirectionalShadowSettings,
    };
    let shadow = DirectionalShadowSettings {
        resolution: 32,
        focus_meters: glam::Vec3::ZERO,
        half_extent_meters: 10.0,
        depth_half_extent_meters: 10.0,
        depth_bias: 0.0,
        normal_bias_meters: 0.0,
        refresh_policy: worldrunner_render_scene::shadows::CascadeRefreshPolicy::default(),
    };
    let mut settings = RendererConfiguration::legacy([65, 49], COLOR);
    settings.frame.format = wgpu::TextureFormat::Rgba16Float;
    settings.hdr = Some((1.0, crate::ToneMap::Reinhard));
    settings.ssr = Some(worldrunner_render_plan::SceneSsr {
        enabled: true,
        ..Default::default()
    });
    let mut base = None;
    for (shadows, expected, bytes) in [
        (None, ReceiverGradientReservation::None, 0),
        (Some(shadow), ReceiverGradientReservation::Front, 24),
        (
            Some(DirectionalShadowSettings {
                normal_bias_meters: 0.012,
                ..shadow
            }),
            ReceiverGradientReservation::Opposing,
            48,
        ),
    ] {
        let reservation = receiver_reservation(shadows);
        assert_eq!(reservation, expected);
        let policy = PresentationPolicy {
            receiver_gradients: reservation,
            budget: crate::PresentationBudget::default().into(),
            capture_pool: crate::CapturePoolPolicy::default(),
            recipes: false,
        };
        let (inventory, _) = policy
            .inventory(
                settings.clone(),
                Viewport {
                    width: 65,
                    height: 49,
                },
                &wgpu::Limits::default(),
            )
            .unwrap();
        assert_eq!(inventory.receiver_gradients, reservation);
        let total = inventory
            .allocations()
            .validate(&wgpu::Limits::default())
            .unwrap();
        let empty = *base.get_or_insert(total);
        assert_eq!(total - empty, bytes * 65 * 49);
    }
}

#[test]
fn default_budget_preserves_ordinary_scene_admission() {
    let options = crate::tests::options();
    assert_eq!(
        options.presentation_budget,
        crate::PresentationBudget::default().into()
    );
    assert_eq!(fixed_budget(&options), 1024 * 1024 * 1024);
    let mut settings = RendererConfiguration::legacy([800, 600], COLOR);
    settings.additional = capture_allocations(Viewport {
        width: 800,
        height: 600,
    });
    settings
        .additional
        .extend(recipe_allocations(256 * 1024 * 1024));
    settings
        .allocations()
        .validate_budget::<std::convert::Infallible>(
            &wgpu::Limits::default(),
            AllocationBudget(fixed_budget(&options)),
        )
        .unwrap();
}

#[test]
fn invalid_budget_fails_before_host_startup() {
    let mut options = crate::tests::options();
    options.presentation_budget = crate::PresentationBudget(0).into();
    assert!(matches!(
        crate::RetainedHost::new(crate::tests::scene(false), options),
        Err(ViewerError::InvalidOptions(_))
    ));
}

#[test]
fn raised_viewer_budget_admits_above_default_inventory() {
    let mut options = crate::tests::options();
    let size = Viewport {
        width: 2400,
        height: 1600,
    };
    let required = expected_water_inventory(size);
    assert!(required > fixed_budget(&options));
    assert!(matches!(synthetic_inventory(size, &options),
        Err(ViewerError::Configuration(ConfigurationError::Budget { required: bytes, budget }))
            if bytes == required && budget == 1024 * 1024 * 1024));
    options.presentation_budget = crate::PresentationBudget(2 * 1024 * 1024 * 1024).into();
    assert_eq!(synthetic_inventory(size, &options).unwrap(), required);
}

#[test]
fn retained_recipe_reservation_fits_issue_sized_water_msaa_inventory() {
    let size = Viewport {
        width: 1360,
        height: 768,
    };
    let options = crate::tests::options();
    let reserved = synthetic_inventory(size, &options).unwrap();
    assert_eq!(reserved, expected_water_inventory(size));
    // Texture, capture, snapshot and recipe storage, plus layout-dependent water frames.
    assert_eq!(reserved, 707_708_496 + water_frame_storage());
    let ordinary = reserved - 256 * 1024 * 1024;
    assert!(ordinary + reserved > fixed_budget(&options));
    assert!(reserved < fixed_budget(&options));
}

fn synthetic_inventory(size: Viewport, options: &crate::ViewerOptions) -> Result<u64, ViewerError> {
    let scene = crate::tests::scene(true);
    let (settings, _) = setup::configuration(
        wgpu::Features::empty(),
        COLOR,
        size,
        crate::FrameProfile::Hdr {
            samples: 4,
            tone_map: crate::ToneMap::Reinhard,
        },
        scene.effects(),
    )?;
    let startup = crate::renderer::Startup::from(options);
    let policy = PresentationPolicy {
        receiver_gradients:
            worldrunner_render_scene::compositor::ReceiverGradientReservation::Opposing,
        budget: startup.presentation_budget,
        capture_pool: crate::CapturePoolPolicy::default(),
        recipes: true,
    };
    let limits = wgpu::Limits::default();
    let (settings, _) = policy.inventory(settings, size, &limits)?;
    Ok(settings.allocations().validate(&limits).unwrap())
}

fn expected_water_inventory(size: Viewport) -> u64 {
    let pixels = u64::from(size.width) * u64::from(size.height);
    // HDR resolve + 4x color/depth; three color/two-depth snapshots;
    // nine 4x HDR water receivers; three staging slots and one padded CPU read.
    let targets = pixels * (8 + 4 * 8 + 4 * 4);
    let snapshots = pixels * (3 * 8 + 6 * 4) + 3 * (304 + 192);
    let water = pixels * 9 * 4 * 8 + 6 * 8 + 8 + water_frame_storage() + 6 * 12;
    let padded = (u64::from(size.width) * 4).next_multiple_of(256) * u64::from(size.height);
    targets + snapshots + water + 4 * padded + 3 * pixels * 4 + 256 * 1024 * 1024
}

fn water_frame_storage() -> u64 {
    let frame = u64::try_from(worldrunner_render_scene::frame::FRAME_UNIFORM_BYTES).unwrap();
    // Each of three slots owns staging and body blocks, both ending in the scene frame.
    3 * ((560 + frame) + (768 + frame))
}

#[test]
fn retained_startup_and_resize_keep_recipe_reservation() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let context = worldrunner_render_scene::device::GpuContext::shared_for_test().unwrap();
    let size = Viewport {
        width: 8,
        height: 8,
    };
    let mut presentation = Presentation::with_settings(
        &context,
        COLOR,
        [size, size],
        crate::FrameProfile::Legacy,
        None,
        &PresentationPolicy {
            receiver_gradients:
                worldrunner_render_scene::compositor::ReceiverGradientReservation::Opposing,
            budget: crate::PresentationBudget(300 * 1024 * 1024).into(),
            capture_pool: crate::CapturePoolPolicy::default(),
            recipes: true,
        },
    )
    .unwrap();
    assert_eq!(presentation.scheduler.generation(), 0);
    let next = Viewport {
        width: 16,
        height: 8,
    };
    presentation.resize(&context, next, next).unwrap();
    assert!(presentation.capture_pool.is_some());
    let mut expected = capture_allocations(next);
    expected.extend(recipe_allocations(256 * 1024 * 1024));
    assert_eq!(
        presentation
            .scheduler
            .configuration()
            .additional
            .validate(&context.device().limits())
            .unwrap(),
        expected.validate(&context.device().limits()).unwrap()
    );
}

#[test]
fn presentation_creation_and_resize_preserve_typed_budget_errors() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let context = worldrunner_render_scene::device::GpuContext::shared_for_test().unwrap();
    let size = Viewport {
        width: 8,
        height: 8,
    };
    let policy = PresentationPolicy {
        receiver_gradients:
            worldrunner_render_scene::compositor::ReceiverGradientReservation::Opposing,
        budget: crate::PresentationBudget(1).into(),
        capture_pool: crate::CapturePoolPolicy::default(),
        recipes: false,
    };
    assert!(
        matches!(Presentation::with_settings(&context, COLOR, [size, size],
        crate::FrameProfile::Legacy, None, &policy),
        Err(ViewerError::Configuration(ConfigurationError::Budget { required, budget: 1 }))
            if required > 1)
    );
    let mut presentation = Presentation::with_settings(
        &context,
        COLOR,
        [size, size],
        crate::FrameProfile::Legacy,
        None,
        &PresentationPolicy {
            receiver_gradients:
                worldrunner_render_scene::compositor::ReceiverGradientReservation::Opposing,
            budget: crate::PresentationBudget(50_000).into(),
            capture_pool: crate::CapturePoolPolicy::default(),
            recipes: false,
        },
    )
    .unwrap();
    let large = Viewport {
        width: 128,
        height: 128,
    };
    assert!(matches!(presentation.resize(&context, large, large),
        Err(ViewerError::Configuration(ConfigurationError::Budget { required, budget: 50_000 }))
            if required > 50_000));
    assert_eq!(presentation.capture_size, size);
}

fn fixed_budget(options: &crate::ViewerOptions) -> u64 {
    let crate::PresentationBudgetPolicy::Fixed(budget) = options.presentation_budget else {
        panic!("expected fixed presentation budget");
    };
    budget.0
}
