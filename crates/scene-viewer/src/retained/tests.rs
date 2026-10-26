fn dome_effects(initial: &SceneEffects) -> SceneEffects {
    SceneEffects::new(SceneEffectsParts {
        water: None,
        sky: SceneSky::DomeTexture {
            texture: TextureHandle::from_raw(0),
        },
        sun: *initial.sun(),
        ambient: *initial.ambient(),
        shadows: initial.shadows().clone(),
        ssao: None,
        camera: *initial.camera(),
        grading: *initial.grading(),
        post: vec![],
        anti_alias: *initial.anti_alias(),
    })
    .unwrap()
    .with_atmosphere(initial.atmosphere().unwrap().clone())
    .unwrap()
}

use worldrunner_render_plan::*;

#[test]
fn atmospheric_settings_are_prepared_in_generation_order() {
    let mut host = host();
    let base = host.scene.effects().unwrap().clone();
    let atmosphere = SceneAtmosphere {
        horizon_radiance_micro: [100_000; 3],
        zenith_radiance_micro: [200_000; 3],
        gradient_exponent_micro: 1_000_000,
        sun_angular_radius_millidegrees: 0,
        fog: None,
        discs: vec![],
        meshes: vec![],
        background_distance_millimeters: 100_000,
    };
    let initial = base.clone().with_atmosphere(atmosphere.clone()).unwrap();
    host.scene = RenderScene::new(SceneParts {
        resources: host.scene.resources().clone(),
        geometry: host.scene.geometry().clone(),
        terrain: None,
        effects: Some(initial.clone()),
    })
    .unwrap();
    host.apply(RetainedUpdate::Settings(Box::new(initial.clone())))
        .unwrap();
    let mut generation = host.accepted_generation();
    let mut background = atmosphere.clone();
    background.horizon_radiance_micro = [300_000; 3];
    let mut disc = atmosphere.clone();
    disc.discs.push(SceneCelestialDisc {
        direction: initial.sun().direction,
        angular_radius_millidegrees: 500,
        rotation_millidegrees: 0,
        texture: None,
        radiance_micro: [1_000_000; 3],
    });
    let mut mesh = atmosphere;
    mesh.meshes.push(SceneCelestialMesh {
        direction: initial.sun().direction,
        angular_radius_millidegrees: 500,
        distance_millimeters: 10_000,
        model_radius_millimeters: 1_000,
        rotation: SceneRotation::IDENTITY,
        mesh: MeshHandle::from_raw(0),
        material: MaterialHandle::from_raw(0),
    });
    let dome = dome_effects(&initial);
    for effects in [
        base.clone().with_atmosphere(background).unwrap(),
        base.clone().with_atmosphere(disc).unwrap(),
        base.with_atmosphere(mesh).unwrap(),
        dome,
    ] {
        host.apply(RetainedUpdate::Settings(Box::new(effects.clone())))
            .unwrap();
        generation += 1;
        assert_eq!(host.accepted_generation(), generation);
        assert_eq!(host.ready_generation(), 0);
        assert_eq!(host.scene.effects(), Some(&effects));
    }
}

use super::*;

fn host() -> RetainedHost {
    RetainedHost::new(
        crate::renderer::retained_tests::source(),
        crate::tests::options(),
    )
    .unwrap()
}

#[test]
fn cpu_consumer_receives_update_and_frame_without_gpu() {
    struct Consumer(usize);
    impl RetainedDriver for Consumer {
        fn update(&mut self, host: &mut RetainedHost, tick: &RetainedTick) -> Result<(), String> {
            self.0 += 1;
            assert_eq!(tick.frame, 0);
            host.apply(RetainedUpdate::Camera {
                pose: crate::tests::options().pose,
                lens: crate::tests::options().camera,
            })
            .map_err(|error| error.to_string())
        }
        fn event(&mut self, _event: &RetainedEvent) -> Result<(), String> {
            Ok(())
        }
    }
    let mut consumer = Consumer(0);
    let mut host = host();
    consumer
        .update(
            &mut host,
            &RetainedTick {
                frame: 0,
                elapsed: Duration::ZERO,
                dt: Duration::ZERO,
            },
        )
        .unwrap();
    consumer
        .event(&RetainedEvent::Presented { frame: 0 })
        .unwrap();
    assert_eq!(consumer.0, 1);
    assert!(host.view().unwrap().is_some());
}

#[test]
fn minimize_resize_and_recreation_preserve_scene() {
    let mut host = host();
    host.input(RetainedInput::Resized([0, 600]));
    assert!(host.view().unwrap().is_none());
    host.input(RetainedInput::Resized([320, 240]));
    assert_eq!(
        host.view().unwrap().unwrap().viewport(),
        Viewport {
            width: 320,
            height: 240
        }
    );
    host.input(RetainedInput::SurfaceRecreated);
    assert_eq!(host.size(), [320, 240]);
    host.input(RetainedInput::Occluded(true));
    assert!(host.view().unwrap().is_none());
    host.input(RetainedInput::Occluded(false));
    assert!(host.view().unwrap().is_some());
}

#[test]
fn focus_loss_emits_releases_in_stable_order_and_clears_held_keys() {
    let mut host = host();
    for key in ["w", "a"] {
        host.input(RetainedInput::Key {
            key: key.into(),
            logical: crate::LogicalKey::Character(key.into()),
            physical: if key == "a" {
                crate::KeyCode::KeyA
            } else {
                crate::KeyCode::KeyW
            }
            .into(),
            pressed: true,
            repeat: false,
        });
    }
    let events = host.input(RetainedInput::Focused(false));
    assert_eq!(
        events,
        vec![
            RetainedInput::Key {
                key: "a".into(),
                logical: crate::LogicalKey::Character("a".into()),
                physical: crate::KeyCode::KeyA.into(),
                pressed: false,
                repeat: false
            },
            RetainedInput::Key {
                key: "w".into(),
                logical: crate::LogicalKey::Character("w".into()),
                physical: crate::KeyCode::KeyW.into(),
                pressed: false,
                repeat: false
            },
            RetainedInput::Focused(false),
        ]
    );
    assert!(!host.key_held("a"));
    assert!(!host.key_held("w"));
    assert_eq!(
        host.input(RetainedInput::Focused(false)),
        vec![RetainedInput::Focused(false)]
    );
}
