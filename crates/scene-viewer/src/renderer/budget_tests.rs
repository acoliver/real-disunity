use super::*;
use worldrunner_render_gpu::configuration::ConfigurationError;

fn host(budget: u64) -> crate::RetainedHost {
    crate::RetainedHost::new(
        crate::tests::scene(false),
        crate::ViewerOptions {
            size: [8, 8],
            presentation_budget: crate::PresentationBudget(budget),
            ..crate::tests::options()
        },
    )
    .unwrap()
}

#[test]
fn windowless_startup_uses_custom_budget_and_preserves_typed_rejection() {
    let host = host(256 * 1024 * 1024);
    assert!(matches!(pollster::block_on(Renderer::windowless(&host)),
        Err(ViewerError::Configuration(ConfigurationError::Budget { required, budget }))
        if required > budget && budget == 256 * 1024 * 1024));
}

#[test]
fn windowless_custom_budget_survives_resize_settings_and_single_sample_recipe() {
    let mut host = host(300 * 1024 * 1024);
    let mut renderer = pollster::block_on(Renderer::windowless(&host)).unwrap();
    renderer.resize([16, 8]).unwrap();
    host.input(crate::RetainedInput::Resized([16, 8]));
    assert!(renderer.render_retained(&mut host).unwrap());
    renderer.drain().unwrap();
    let generation = renderer.presentation.scheduler.generation();
    let effects = host
        .scene
        .effects()
        .unwrap()
        .clone()
        .with_specular_convention(worldrunner_render_plan::SceneSpecularConvention {
            roughness: worldrunner_render_plan::SceneRoughnessRemap::Perceptual04,
            ..Default::default()
        });
    host.apply(crate::RetainedUpdate::Settings(Box::new(effects)))
        .unwrap();
    assert!(renderer.render_retained(&mut host).unwrap());
    renderer.drain().unwrap();
    assert!(renderer.presentation.scheduler.generation() > generation);
    assert_eq!(host.ready_generation(), host.accepted_generation());
    assert_single_sample_generation(&mut renderer, &mut host);
    assert!(matches!(renderer.resize([2048, 2048]),
        Err(ViewerError::Configuration(ConfigurationError::Budget { required, budget }))
        if required > budget && budget == 300 * 1024 * 1024));
}

fn assert_single_sample_generation(renderer: &mut Renderer, host: &mut crate::RetainedHost) {
    let generation = renderer.presentation.scheduler.generation();
    let recipe = crate::RetainedCaptureRecipe::new(
        1,
        1,
        crate::CaptureJitter::None,
        false,
        vec![crate::CaptureOutput::Srgb8],
        1024 * 1024,
    )
    .unwrap();
    let id = host
        .request_capture_recipe(host.ready_generation(), recipe)
        .unwrap();
    assert!(renderer.render_retained(host).unwrap());
    assert_eq!(renderer.presentation.scheduler.generation(), generation);
    renderer.drain().unwrap();
    renderer.collect_retained(host).unwrap();
    assert!(matches!(host.captures.take_event(),
        Some(crate::RetainedEvent::Capture { id: actual,
            outcome: crate::RetainedCaptureOutcome::RecipeCompleted { .. }, .. }) if actual == id));
}

#[test]
fn native_rcas_settings_publish_once_with_an_earlier_frame_in_flight() {
    let mut host = host(300 * 1024 * 1024);
    let rcas = crate::NativeRcasSettings {
        stops: 0.75,
        domain: crate::RcasDomain::EncodedOutput,
    };
    host.options.frame_profile = crate::FrameProfile::HdrNativeRcas {
        samples: 1,
        tone_map: crate::ToneMap::Reinhard,
        rcas,
    };
    let mut renderer = pollster::block_on(Renderer::windowless(&host)).unwrap();
    let settings = renderer.presentation.scheduler.configuration().clone();
    let mut pair = settings.allocations();
    pair.extend(settings.allocations());
    let budget = pair.validate(&renderer.context.device().limits()).unwrap();
    renderer.presentation.scheduler =
        worldrunner_render_scene::compositor::ConfiguredFrameRenderer::new(
            &renderer.context,
            settings,
            worldrunner_render_gpu::configuration::AllocationBudget(budget),
        )
        .unwrap();
    renderer.presentation.refresh_targets();
    assert!(renderer.render_retained(&mut host).unwrap());
    let authority = renderer.context.submission();
    assert!(authority.submitted_serial() > authority.completed_serial());
    let generation = renderer.presentation.scheduler.generation();
    let effects = host.scene.effects().unwrap().clone();
    host.apply(crate::RetainedUpdate::Settings(Box::new(effects)))
        .unwrap();
    renderer.publish_retained(&mut host).unwrap();
    assert_eq!(host.ready_generation(), host.accepted_generation());
    assert_eq!(
        renderer
            .presentation
            .scheduler
            .configuration()
            .post
            .native_rcas,
        Some(rcas)
    );
    assert_eq!(renderer.presentation.scheduler.generation(), generation + 1);
    let size = host.view().unwrap().unwrap().viewport();
    renderer
        .presentation
        .resize(&renderer.context, size, size)
        .unwrap();
    assert_eq!(renderer.presentation.scheduler.generation(), generation + 1);
    assert!(renderer.render_retained(&mut host).unwrap());
    assert_eq!(renderer.presentation.scheduler.generation(), generation + 1);
    renderer.drain().unwrap();
}
