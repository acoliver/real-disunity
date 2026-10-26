use worldrunner_world_simulation::presentation::lighting::global_light_color;
use worldrunner_world_simulation::presentation::{
    GlobalLight2d, Light2dType, SpriteLighting, SpriteRenderer, UniversalAdditionalCameraData,
};
use worldrunner_world_simulation::{
    ComponentDesc, ComponentError, ObjectDesc, World, WorldError, WorldSettings,
};

#[test]
fn global_lights_sum_by_layer_and_absence_preserves_tint() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let mut sprite = SpriteRenderer {
        lighting: SpriteLighting::Global,
        color: [0.5, 0.5, 0.5, 0.75],
        ..Default::default()
    };
    assert_color(global_light_color(&world, &sprite).unwrap(), sprite.color);
    let owner = world.create_object(ObjectDesc::default()).unwrap();
    for color in [[0.25, 0.5, 0.75, 1.], [0.25, 0.25, 0.25, 1.]] {
        let object = world.create_object(ObjectDesc::default()).unwrap();
        world
            .add_component(
                object,
                ComponentDesc::GlobalLight2d(GlobalLight2d {
                    color,
                    intensity: 1.,
                    ..Default::default()
                }),
            )
            .unwrap();
    }
    assert_color(
        global_light_color(&world, &sprite).unwrap(),
        [0.25, 0.375, 0.5, 0.75],
    );
    sprite.sorting_layer = 7;
    assert_color(
        global_light_color(&world, &sprite).unwrap(),
        [0., 0., 0., 0.75],
    );
    sprite.lighting = SpriteLighting::Unlit;
    assert_color(global_light_color(&world, &sprite).unwrap(), sprite.color);
    world
        .add_component(owner, ComponentDesc::AudioListener { enabled: true })
        .unwrap();
    assert_eq!(world.active_audio_listeners(), 1);
    world.set_active(owner, false).unwrap();
    assert_eq!(world.active_audio_listeners(), 0);
}

#[test]
fn unsupported_lighting_and_camera_fields_are_typed() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let object = world.create_object(ObjectDesc::default()).unwrap();
    for light_type in [
        Light2dType::Parametric,
        Light2dType::Freeform,
        Light2dType::Sprite,
        Light2dType::Point,
        Light2dType::Spot,
    ] {
        let desc = GlobalLight2d {
            light_type,
            ..Default::default()
        };
        assert_eq!(
            world.add_component(object, ComponentDesc::GlobalLight2d(desc)),
            Err(WorldError::InvalidComponent(
                ComponentError::UnsupportedField("lightType")
            ))
        );
    }
    for (property, field) in [
        (0, "renderPostProcessing"),
        (1, "antialiasing"),
        (2, "requiresDepthTexture"),
        (3, "requiresColorTexture"),
        (4, "renderType"),
        (5, "renderShadows"),
        (6, "stopNaN"),
        (7, "dithering"),
    ] {
        let mut data = UniversalAdditionalCameraData::default();
        assert_eq!(
            data.set(property, 1),
            Err(WorldError::InvalidComponent(
                ComponentError::UnsupportedField(field)
            ))
        );
    }
    assert_eq!(
        world.add_component(object, ComponentDesc::ShadowCaster2d),
        Err(WorldError::InvalidComponent(
            ComponentError::UnsupportedField("ShadowCaster2D")
        ))
    );
}

#[test]
fn participant_lifetime_clone_and_rejected_updates_keep_state() {
    use worldrunner_world_simulation::DestroyWhen;
    let mut world = World::new(WorldSettings::default()).unwrap();
    let owner = world.create_object(ObjectDesc::default()).unwrap();
    let light = world
        .add_component(
            owner,
            ComponentDesc::GlobalLight2d(GlobalLight2d::default()),
        )
        .unwrap();
    let listener = world
        .add_component(owner, ComponentDesc::AudioListener { enabled: true })
        .unwrap();
    world
        .add_component(owner, ComponentDesc::GUILayer { enabled: false })
        .unwrap();
    world
        .add_component(owner, ComponentDesc::FlareLayer { enabled: true })
        .unwrap();
    let mut bad = GlobalLight2d {
        blend_style_index: 1,
        ..Default::default()
    };
    assert_eq!(
        world.set_component(light, ComponentDesc::GlobalLight2d(bad.clone())),
        Err(WorldError::InvalidComponent(
            ComponentError::UnsupportedField("blendStyleIndex")
        ))
    );
    bad.blend_style_index = 0;
    bad.intensity = f32::NAN;
    assert!(
        world
            .set_component(light, ComponentDesc::GlobalLight2d(bad))
            .is_err()
    );
    assert_eq!(
        world.component(light).unwrap().desc,
        ComponentDesc::GlobalLight2d(GlobalLight2d::default())
    );
    assert!(
        world
            .add_component(owner, ComponentDesc::AudioListener { enabled: true })
            .is_err()
    );
    let clone = world.instantiate(owner, None).unwrap();
    assert_eq!(
        world.component(clone.components[&light]).unwrap().desc,
        world.component(light).unwrap().desc
    );
    assert_eq!(world.active_audio_listeners(), 2);
    world.destroy(clone.root, DestroyWhen::EndOfFrame).unwrap();
    assert_eq!(world.active_audio_listeners(), 1);
    world.destroy_component(listener).unwrap();
    assert_eq!(world.active_audio_listeners(), 0);
    world.destroy(owner, DestroyWhen::EndOfFrame).unwrap();
    let sprite = SpriteRenderer {
        lighting: SpriteLighting::Global,
        ..Default::default()
    };
    assert_color(global_light_color(&world, &sprite).unwrap(), sprite.color);
}
#[test]
fn camera_defaults_and_aliases_are_consistent() {
    let mut data = UniversalAdditionalCameraData::default();
    assert_eq!(data.get(8), Some(2));
    assert_eq!(data.get(9), Some(2));
    data.set(2, 0).unwrap();
    assert_eq!(data.get(8), Some(0));
    let before = data.clone();
    assert_eq!(
        data.set(8, 1),
        Err(WorldError::InvalidComponent(
            ComponentError::UnsupportedField("requiresDepthOption")
        ))
    );
    assert_eq!(data, before);
    assert_eq!(
        data.set(9, 1),
        Err(WorldError::InvalidComponent(
            ComponentError::UnsupportedField("requiresColorOption")
        ))
    );
    assert_eq!(
        data.set(77, 0),
        Err(WorldError::InvalidComponent(
            ComponentError::UnsupportedField("property")
        ))
    );
}

fn assert_color(actual: [f32; 4], expected: [f32; 4]) {
    assert_eq!(actual.map(f32::to_bits), expected.map(f32::to_bits));
}

#[test]
fn overflowing_light_accumulation_is_a_native_presentation_error() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    for _ in 0..2 {
        let object = world.create_object(ObjectDesc::default()).unwrap();
        world
            .add_component(
                object,
                ComponentDesc::GlobalLight2d(GlobalLight2d {
                    intensity: f32::MAX,
                    sorting_layers: vec![0, 0],
                    ..Default::default()
                }),
            )
            .unwrap();
    }
    let sprite = SpriteRenderer {
        lighting: SpriteLighting::Global,
        ..Default::default()
    };
    assert_eq!(
        global_light_color(&world, &sprite),
        Err(WorldError::InvalidComponent(
            ComponentError::InvalidPresentation
        ))
    );
}
