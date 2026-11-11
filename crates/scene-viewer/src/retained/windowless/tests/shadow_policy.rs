use super::*;

fn shadow_effects(base: &SceneEffects) -> SceneEffects {
    SceneEffects::new(SceneEffectsParts {
        water: None,
        sky: *base.sky(),
        sun: SceneSun {
            irradiance_micro: 1_000_000,
            ..*base.sun()
        },
        ambient: SceneAmbient {
            sky_irradiance_micro: 100_000,
            ground_irradiance_micro: 100_000,
        },
        shadows: base.shadows().clone(),
        ssao: None,
        camera: *base.camera(),
        grading: *base.grading(),
        post: vec![],
        anti_alias: SceneAntiAlias::Disabled,
    })
    .unwrap()
}

fn shadow_host() -> RetainedHost {
    let base = crate::renderer::retained_tests::source();
    let original = base.geometry().objects()[0];
    let caster = SceneObject::new(
        SceneTransform {
            position_millimeters: [0, 0, 800],
            scale_millionths: 200_000,
            ..*original.transform()
        },
        original.mesh(),
        original.material(),
    );
    let scene = base
        .with_objects_and_effects(
            vec![original, caster],
            Some(
                shadow_effects(base.effects().unwrap()).with_shadow_direction(Some(
                    SceneDirection::new([600_000, 0, 800_000]).unwrap(),
                )),
            ),
        )
        .unwrap();
    let mut options = crate::tests::options();
    options.size = [64, 64];
    options.render_clock = RenderClock::Fixed(Duration::ZERO);
    let mut shadows = crate::renderer::retained_tests::shadow_settings();
    shadows.refresh_policy.intervals = [16; 4];
    options.shadows = Some(shadows);
    RetainedHost::new(scene, options).unwrap()
}

fn set_policy(run: &mut WindowlessRetained<'_>, policy: crate::CameraShadowPolicy) {
    let options = run.host().options();
    let update = RetainedUpdate::camera_with_shadow_policy(
        options.pose,
        options.camera,
        options.camera_convention,
        crate::CameraContinuity::Continuous,
        Some(policy),
    );
    run.host_mut().apply(update).unwrap();
}

fn has_shadow_draws(run: &WindowlessRetained<'_>) -> bool {
    run.host()
        .statistics()
        .unwrap()
        .scene
        .passes
        .shadow_cascades
        .iter()
        .any(|cascade| cascade.calls > 0)
}

fn move_caster_while_disabled(run: &mut WindowlessRetained<'_>) {
    let id = crate::ObjectId(1);
    let object = run.host().object(id).unwrap();
    let moved = SceneObject::new(
        SceneTransform {
            position_millimeters: [600, 0, 800],
            ..*object.transform()
        },
        object.mesh(),
        object.material(),
    );
    run.host_mut()
        .apply(RetainedUpdate::Replace { id, object: moved })
        .unwrap();
}

#[test]
fn retained_camera_policy_changes_receiver_and_skips_shadow_casters() {
    let mut driver = Driver::default();
    let events = driver.events.clone();
    let mut run = shadow_host().windowless(&mut driver).unwrap();

    assert!(run.frame().unwrap());
    assert!(has_shadow_draws(&run));
    assert!(run.frame().unwrap());
    assert!(!has_shadow_draws(&run));
    let enabled_frame = run.host().statistics().unwrap().frame;
    let enabled = capture(&mut run, &events).1;

    set_policy(&mut run, crate::CameraShadowPolicy::Disabled);
    assert!(run.frame().unwrap());
    let disabled_stats = run.host().statistics().unwrap().scene;
    let disabled_frame = run.host().statistics().unwrap().frame;
    assert!(!has_shadow_draws(&run));
    assert!(
        disabled_stats
            .passes
            .shadow_cascades
            .iter()
            .all(|cascade| cascade.calls == 0)
    );
    let disabled = capture(&mut run, &events).1;
    assert!(disabled_frame > enabled_frame);
    assert_ne!(enabled, disabled);

    set_policy(&mut run, crate::CameraShadowPolicy::Enabled);
    assert!(run.frame().unwrap());
    assert!(has_shadow_draws(&run));
    let restored = capture(&mut run, &events).1;
    assert_eq!(enabled, restored);

    set_policy(&mut run, crate::CameraShadowPolicy::Disabled);
    assert!(run.frame().unwrap());
    move_caster_while_disabled(&mut run);
    assert!(run.frame().unwrap());
    assert!(!has_shadow_draws(&run));
    set_policy(&mut run, crate::CameraShadowPolicy::Enabled);
    assert!(run.frame().unwrap());
    assert!(has_shadow_draws(&run));
    let moved = capture(&mut run, &events).1;
    assert_ne!(restored, moved);
    run.shutdown().unwrap();
}

#[test]
fn disabled_policy_survives_resource_resident_rebuild() {
    let mut driver = Driver::default();
    let events = driver.events.clone();
    let mut run = shadow_host().windowless(&mut driver).unwrap();
    set_policy(&mut run, crate::CameraShadowPolicy::Disabled);
    assert!(run.frame().unwrap());
    let disabled = capture_recipe(&mut run, &events).0;

    let scene = run.host().scene().clone();
    run.host_mut()
        .apply(RetainedUpdate::Resources {
            resources: scene.resources().clone(),
            meshes: scene.geometry().meshes().to_vec(),
        })
        .unwrap();
    assert!(run.frame().unwrap());
    while !run.host().readiness.queue.is_empty() {
        assert!(run.frame().unwrap());
    }
    assert!(!has_shadow_draws(&run));
    let (image, _) = capture_recipe(&mut run, &events);
    assert_eq!(disabled, image);

    set_policy(&mut run, crate::CameraShadowPolicy::Enabled);
    assert!(run.frame().unwrap());
    assert!(has_shadow_draws(&run));
    run.shutdown().unwrap();
}

fn capture_recipe(
    run: &mut WindowlessRetained<'_>,
    events: &RefCell<Vec<RetainedEvent>>,
) -> (RgbaImage, Box<crate::RetainedCaptureObservation>) {
    let generation = run.host().accepted_generation();
    let recipe = crate::RetainedCaptureRecipe::new(
        1,
        2,
        crate::CaptureJitter::None,
        false,
        vec![crate::CaptureOutput::Srgb8],
        1024 * 1024,
    )
    .unwrap();
    assert!(recipe.accumulated());
    let id = run
        .host_mut()
        .request_capture_recipe(generation, recipe)
        .unwrap();
    let mut watchdog = CaptureWatchdog::new(run, format!("shadow policy recipe {id}"));
    loop {
        run.frame().unwrap();
        run.poll().unwrap();
        let result = events.borrow().iter().find_map(|event| match event {
            RetainedEvent::Capture {
                id: actual,
                generation: published,
                outcome:
                    RetainedCaptureOutcome::RecipeCompleted {
                        products,
                        observation,
                        ..
                    },
            } if *actual == id => {
                assert_eq!(*published, generation);
                products
                    .srgb8
                    .clone()
                    .map(|image| (image, observation.clone()))
            }
            _ => None,
        });
        watchdog.check(run, result.is_some());
        if let Some(result) = result {
            return result;
        }
        std::thread::yield_now();
    }
}

#[test]
fn accumulated_capture_applies_camera_policy_without_an_ordinary_frame() {
    let mut driver = Driver::default();
    let events = driver.events.clone();
    let mut run = shadow_host().windowless(&mut driver).unwrap();
    let enabled = capture_recipe(&mut run, &events).0;

    set_policy(&mut run, crate::CameraShadowPolicy::Disabled);
    let (disabled, observation) = capture_recipe(&mut run, &events);
    assert!(
        observation
            .scene
            .passes
            .shadow_cascades
            .iter()
            .all(|cascade| cascade.calls == 0),
        "disabled accumulated capture must not execute shadow casters"
    );
    assert!(
        enabled != disabled,
        "accumulated capture must apply the disabled receiver policy"
    );

    set_policy(&mut run, crate::CameraShadowPolicy::Enabled);
    let restored = capture_recipe(&mut run, &events).0;
    assert_eq!(enabled, restored);
    run.shutdown().unwrap();
}

#[test]
fn rejected_plain_shadow_enable_preserves_the_rendered_view() {
    let mut driver = Driver::default();
    let events = driver.events.clone();
    let mut run = host(crate::tests::scene(false))
        .windowless(&mut driver)
        .unwrap();
    let before = capture(&mut run, &events).1;
    let options = run.host().options();
    let mut pose = options.pose;
    pose.eye_millimeters[0] += 500;
    let update = RetainedUpdate::camera_with_shadow_policy(
        pose,
        options.camera,
        options.camera_convention,
        crate::CameraContinuity::Cut,
        Some(crate::CameraShadowPolicy::Enabled),
    );
    assert!(matches!(
        run.host_mut().apply(update),
        Err(crate::ViewerError::UnsupportedScene(_))
    ));
    assert_eq!(run.host().accepted_generation(), 0);
    assert_eq!(before, capture(&mut run, &events).1);
    run.shutdown().unwrap();
}
