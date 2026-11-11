use super::*;
use crate::presentation::{DisunityPresentation, disunity_presentation_prepare};
use worldrunner_world_simulation::{
    ComponentDesc, ComponentHandle, LocalTransform, ObjectDesc,
    presentation::{LightKind, LightSettings, LightShadowMode},
};

fn sun() -> Light {
    Light {
        enabled: true,
        settings: LightSettings {
            kind: LightKind::Directional,
            color: [1.0, 0.9, 0.8, 1.0],
            intensity: 1.5,
            shadows: LightShadowMode::Soft,
            shadow_bias: 0.1,
            ..LightSettings::default()
        },
    }
}

/// A world with one light, on an object the ABI knows as object 7 at (1, 2, 3).
fn world_with_light() -> (u64, ComponentHandle) {
    let mut store = crate::lock_store();
    let world = crate::store::create_world(&mut store).unwrap();
    let session = store.session_mut(world).unwrap();
    let object = session
        .native
        .create_object(ObjectDesc {
            local_transform: LocalTransform {
                translation: glam::Vec3::new(1.0, 2.0, 3.0),
                ..Default::default()
            },
            ..Default::default()
        })
        .unwrap();
    session.objects.insert(7, object);
    let light = session
        .native
        .add_component(object, ComponentDesc::Light(sun()))
        .unwrap();
    (world, light)
}

fn prepare(world: u64) {
    let mut header = DisunityPresentation::default();
    let result = unsafe { disunity_presentation_prepare(world, 80, 40, 18, &mut header) };
    assert_eq!(result.code, 0);
}

fn count(world: u64) -> (DisunityResult, u32) {
    let mut count = u32::MAX;
    let result = unsafe { disunity_presentation_light_count(world, &mut count) };
    (result, count)
}

fn light(world: u64, index: u32) -> (DisunityResult, DisunityLight, u64, DisunityMatrix) {
    let mut record = DisunityLight::default();
    let mut object = 0;
    let mut matrix = DisunityMatrix::default();
    let result =
        unsafe { disunity_presentation_light(world, index, &mut record, &mut object, &mut matrix) };
    (result, record, object, matrix)
}

#[test]
fn a_prepared_frame_lists_its_light_with_settings_identity_and_matrix() {
    let (world, _) = world_with_light();
    prepare(world);
    assert_eq!(count(world).1, 1);
    let (result, record, object, matrix) = light(world, 0);
    assert_eq!(result.code, 0);
    let expected = DisunityLight::from_native(sun());
    assert_eq!(
        (record.kind, record.shadows, record.enabled),
        (expected.kind, expected.shadows, 1)
    );
    assert_eq!(
        [
            record.r,
            record.g,
            record.b,
            record.intensity,
            record.shadow_bias
        ],
        [1.0, 0.9, 0.8, 1.5, 0.1]
    );
    assert_eq!(object, 7);
    assert_eq!([matrix.m12, matrix.m13, matrix.m14], [1.0, -3.0, 2.0]);
}

#[test]
fn the_prepared_lights_stay_until_the_next_prepare() {
    let (world, handle) = world_with_light();
    prepare(world);
    {
        let mut store = crate::lock_store();
        let session = store.session_mut(world).unwrap();
        let mut brighter = sun();
        brighter.settings.intensity = 3.0;
        session
            .native
            .set_component(handle, ComponentDesc::Light(brighter))
            .unwrap();
    }
    assert_eq!(light(world, 0).1.intensity, 1.5, "the prepared frame");
    prepare(world);
    assert_eq!(light(world, 0).1.intensity, 3.0, "the next frame");
}

#[test]
fn reads_before_a_frame_past_its_lights_or_into_null_are_rejected() {
    let (world, _) = world_with_light();
    assert_ne!(count(world).0.code, 0, "nothing prepared yet");
    assert_ne!(light(world, 0).0.code, 0, "nothing prepared yet");
    prepare(world);
    assert_ne!(light(world, 1).0.code, 0, "past the frame's lights");
    let mut record = DisunityLight::default();
    let mut matrix = DisunityMatrix::default();
    let null = std::ptr::null_mut();
    let result = unsafe { disunity_presentation_light(world, 0, &mut record, null, &mut matrix) };
    assert_ne!(result.code, 0);
    assert_ne!(
        unsafe { disunity_presentation_light_count(world, null.cast()) }.code,
        0
    );
}
