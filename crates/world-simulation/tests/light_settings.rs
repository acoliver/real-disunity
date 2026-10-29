//! The native 3D light component: defaults, validation, normalization, cloning and destruction.
use worldrunner_world_simulation::presentation::{
    Light, LightBakeType, LightKind, LightSettings, LightShadowMode,
};
use worldrunner_world_simulation::{
    ComponentDesc, ComponentError, ComponentHandle, DestroyWhen, ObjectDesc, ObjectHandle, World,
    WorldError, WorldSettings,
};

type Setter = fn(&mut LightSettings, f32);

fn bits(values: &[f32]) -> Vec<u32> {
    values.iter().map(|value| value.to_bits()).collect()
}

fn invalid() -> WorldError {
    WorldError::InvalidComponent(ComponentError::InvalidPresentation)
}

fn world_with(light: Light) -> (World, ObjectHandle) {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let object = world.create_object(ObjectDesc::default()).unwrap();
    world
        .add_component(object, ComponentDesc::Light(light))
        .unwrap();
    (world, object)
}

fn light_handle(world: &World, object: ObjectHandle) -> Option<ComponentHandle> {
    world
        .snapshot()
        .components
        .into_iter()
        .find_map(|component| {
            (component.object == object && matches!(component.desc, ComponentDesc::Light(_)))
                .then_some(component.handle)
        })
}

fn light_of(world: &World, object: ObjectHandle) -> Light {
    let handle = light_handle(world, object).expect("a light on the object");
    match world.component(handle).unwrap().desc {
        ComponentDesc::Light(light) => light,
        _ => unreachable!("light_handle finds lights"),
    }
}

fn with(change: impl FnOnce(&mut LightSettings)) -> LightSettings {
    let mut settings = LightSettings::default();
    change(&mut settings);
    settings
}

#[test]
fn a_new_light_has_the_engines_defaults() {
    let light = Light::default();
    let s = light.settings;
    assert!(light.enabled);
    assert_eq!(s.kind, LightKind::Point);
    assert_eq!(bits(&s.color), bits(&[1.; 4]));
    assert_eq!(
        bits(&[s.intensity, s.range, s.spot_angle, s.inner_spot_angle]),
        bits(&[1., 10., 30., 21.8])
    );
    assert_eq!(s.shadows, LightShadowMode::None);
    assert_eq!(
        bits(&[
            s.shadow_strength,
            s.shadow_bias,
            s.shadow_normal_bias,
            s.shadow_near_plane
        ]),
        bits(&[1., 0.05, 0.4, 0.2])
    );
    assert_eq!((s.culling_mask, s.rendering_layer_mask), (u32::MAX, 1));
    assert_eq!(
        bits(&[s.bounce_intensity, s.color_temperature, s.cookie_size]),
        bits(&[1., 6570., 10.])
    );
    assert!(!s.use_color_temperature);
    assert_eq!(s.cookie_texture, 0);
    assert_eq!(s.bake_type, LightBakeType::Realtime);
    assert_eq!(light.validate(), Ok(()));
}

#[test]
fn negative_and_non_finite_values_are_rejected() {
    let setters: [(&str, Setter); 4] = [
        ("intensity", |s, v| s.intensity = v),
        ("range", |s, v| s.range = v),
        ("bounce intensity", |s, v| s.bounce_intensity = v),
        ("cookie size", |s, v| s.cookie_size = v),
    ];
    for (name, set) in setters {
        for bad in [-1., f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let settings = with(|s| set(s, bad));
            assert_eq!(settings.validate(), Err(invalid()), "{name} {bad}");
            assert_eq!(settings.normalized(), Err(invalid()), "{name} {bad}");
        }
        assert_eq!(with(|s| set(s, 0.)).validate(), Ok(()), "{name} zero");
    }
}

#[test]
fn a_color_temperature_must_be_finite_and_above_zero() {
    for bad in [0., -5., f32::NAN, f32::INFINITY] {
        assert_eq!(
            with(|s| s.color_temperature = bad).validate(),
            Err(invalid()),
            "{bad}"
        );
    }
    assert_eq!(with(|s| s.color_temperature = 3200.).validate(), Ok(()));
}

#[test]
fn colors_and_shadow_biases_must_be_finite() {
    let setters: [Setter; 5] = [
        |s, v| s.color[0] = v,
        |s, v| s.color[3] = v,
        |s, v| s.shadow_bias = v,
        |s, v| s.shadow_normal_bias = v,
        |s, v| s.shadow_near_plane = v,
    ];
    for set in setters {
        assert_eq!(with(|s| set(s, f32::NAN)).validate(), Err(invalid()));
        assert_eq!(with(|s| set(s, f32::INFINITY)).validate(), Err(invalid()));
        assert_eq!(with(|s| set(s, -2.)).validate(), Ok(()));
    }
}

#[test]
fn stored_angles_and_shadow_strength_must_be_in_range() {
    let out_of_range = [
        with(|s| s.spot_angle = 0.5),
        with(|s| s.spot_angle = 179.5),
        with(|s| s.inner_spot_angle = -1.),
        with(|s| s.inner_spot_angle = 31.),
        with(|s| s.shadow_strength = 1.5),
        with(|s| s.shadow_strength = -0.5),
    ];
    for settings in out_of_range {
        assert_eq!(settings.validate(), Err(invalid()), "{settings:?}");
    }
    let edges = with(|s| {
        s.spot_angle = 179.;
        s.inner_spot_angle = 179.;
        s.shadow_strength = 0.;
    });
    assert_eq!(edges.validate(), Ok(()));
}

#[test]
fn normalizing_clamps_the_spot_angle_to_1_through_179() {
    let low = with(|s| s.spot_angle = 0.).normalized().unwrap();
    let high = with(|s| s.spot_angle = 500.).normalized().unwrap();
    let infinite = with(|s| s.spot_angle = f32::INFINITY).normalized().unwrap();
    assert_eq!(bits(&[low.spot_angle]), bits(&[1.]));
    assert_eq!(bits(&[high.spot_angle]), bits(&[179.]));
    assert_eq!(bits(&[infinite.spot_angle]), bits(&[179.]));
}

#[test]
fn normalizing_clamps_the_inner_angle_to_zero_through_the_spot_angle() {
    let below = with(|s| s.inner_spot_angle = -3.).normalized().unwrap();
    let above = with(|s| {
        s.spot_angle = 60.;
        s.inner_spot_angle = 90.;
    })
    .normalized()
    .unwrap();
    assert_eq!(bits(&[below.inner_spot_angle]), bits(&[0.]));
    assert_eq!(bits(&[above.inner_spot_angle]), bits(&[60.]));
}

#[test]
fn lowering_the_spot_angle_lowers_the_inner_angle_with_it() {
    let narrowed = with(|s| s.spot_angle = 10.).normalized().unwrap();
    assert_eq!(
        bits(&[narrowed.spot_angle, narrowed.inner_spot_angle]),
        bits(&[10., 10.])
    );
    let unchanged = with(|s| s.spot_angle = 40.).normalized().unwrap();
    assert_eq!(bits(&[unchanged.inner_spot_angle]), bits(&[21.8]));
}

#[test]
fn normalizing_clamps_shadow_strength_to_zero_through_one() {
    let high = with(|s| s.shadow_strength = 2.).normalized().unwrap();
    let low = with(|s| s.shadow_strength = -1.).normalized().unwrap();
    assert_eq!(bits(&[high.shadow_strength]), bits(&[1.]));
    assert_eq!(bits(&[low.shadow_strength]), bits(&[0.]));
}

#[test]
fn normalizing_rejects_a_nan_angle_or_shadow_strength() {
    let rejected = [
        with(|s| s.spot_angle = f32::NAN),
        with(|s| s.inner_spot_angle = f32::NAN),
        with(|s| s.shadow_strength = f32::NAN),
    ];
    for settings in rejected {
        assert_eq!(settings.normalized(), Err(invalid()), "{settings:?}");
    }
}

#[test]
fn the_world_rejects_a_light_that_fails_validation() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let object = world.create_object(ObjectDesc::default()).unwrap();
    let bad = Light {
        enabled: true,
        settings: with(|s| s.intensity = -1.),
    };
    assert_eq!(
        world.add_component(object, ComponentDesc::Light(bad)),
        Err(invalid())
    );
    let handle = world
        .add_component(object, ComponentDesc::Light(Light::default()))
        .unwrap();
    assert_eq!(
        world.set_component(handle, ComponentDesc::Light(bad)),
        Err(invalid())
    );
    assert_eq!(light_of(&world, object), Light::default());
}

#[test]
fn an_object_has_at_most_one_light() {
    let (mut world, object) = world_with(Light::default());
    assert_eq!(
        world.add_component(object, ComponentDesc::Light(Light::default())),
        Err(WorldError::InvalidComponent(ComponentError::Duplicate))
    );
}

#[test]
fn a_disabled_light_keeps_its_settings() {
    let settings = with(|s| {
        s.kind = LightKind::Spot;
        s.intensity = 3.;
        s.shadows = LightShadowMode::Soft;
    });
    let (world, object) = world_with(Light {
        enabled: false,
        settings,
    });
    let light = light_of(&world, object);
    assert!(!light.enabled);
    assert_eq!(light.settings, settings);
}

#[test]
fn instantiating_clones_the_light_with_its_settings() {
    let settings = with(|s| {
        s.kind = LightKind::Directional;
        s.color = [1., 0.5, 0.25, 1.];
        s.intensity = 2.;
        s.cookie_texture = 77;
        s.bake_type = LightBakeType::Baked;
        s.use_color_temperature = true;
    });
    let (mut world, object) = world_with(Light {
        enabled: false,
        settings,
    });
    let clone = world.instantiate(object, None).unwrap();
    let cloned = light_of(&world, clone.root);
    assert!(!cloned.enabled);
    assert_eq!(cloned.settings, settings);
    let source = light_handle(&world, object).unwrap();
    let target = light_handle(&world, clone.root).unwrap();
    assert_ne!(source, target);
    assert_eq!(clone.components[&source], target);
    let changed = Light {
        enabled: true,
        settings: with(|s| s.intensity = 9.),
    };
    world
        .set_component(target, ComponentDesc::Light(changed))
        .unwrap();
    assert_eq!(light_of(&world, object).settings, settings);
}

#[test]
fn destroying_the_object_or_the_component_removes_the_light() {
    let (mut world, object) = world_with(Light::default());
    let baseline = world.storage_counts();
    world.destroy(object, DestroyWhen::EndOfFrame).unwrap();
    world.finish_phase();
    assert_eq!(world.storage_counts(), (baseline.0 - 1, baseline.1 - 2));

    let (mut world, object) = world_with(Light::default());
    let handle = light_handle(&world, object).unwrap();
    world.remove_component(handle).unwrap();
    assert!(world.component(handle).is_err());
    assert_eq!(world.storage_counts().1, 1);
}
