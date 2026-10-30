use super::*;

pub(super) fn retained_budget(event_loop: &EventLoop<()>, window: &Arc<Window>) {
    let scene = crate::tests::scene(false);
    let viewport = window.inner_size();
    let options = crate::ViewerOptions {
        presentation_budget: crate::PresentationBudget(300 * 1024 * 1024).into(),
        size: [viewport.width, viewport.height],
        ..crate::tests::options()
    };
    let mut startup = Startup::from(&options);
    startup.recipes = true;
    let mut renderer = pollster::block_on(Renderer::new(
        event_loop.owned_display_handle(),
        window.clone(),
        &scene,
        None,
        crate::FrameProfile::Legacy,
        startup,
    ))
    .unwrap();
    let mut host = crate::RetainedHost::new(scene.clone(), options).unwrap();
    present_retained(&mut renderer, &mut host);
    renderer.drain().unwrap();
    renderer.resize([16, 8]).unwrap();
    host.input(crate::RetainedInput::Resized([16, 8]));
    present_retained(&mut renderer, &mut host);
    renderer.drain().unwrap();
    let before = renderer.presentation.scheduler.generation();
    host.apply(crate::RetainedUpdate::Settings(Box::new(
        scene.effects().unwrap().clone(),
    )))
    .unwrap();
    present_retained(&mut renderer, &mut host);
    renderer.drain().unwrap();
    assert_eq!(host.ready_generation(), host.accepted_generation());
    assert!(renderer.presentation.scheduler.generation() > before);
    assert!(matches!(renderer.resize([2048, 2048]),
        Err(ViewerError::Configuration(worldrunner_render_gpu::configuration::ConfigurationError::Budget {
            required, budget,
        })) if required > budget && budget == 300 * 1024 * 1024));
}
