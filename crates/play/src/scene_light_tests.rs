use super::*;
use glam::{Quat, Vec3};
use worldrunner_render_plan::SceneDirection;
use worldrunner_render_scene::CameraPose;
use worldrunner_render_scene::shadows::CascadeRefreshPolicy;
use worldrunner_scene_viewer::CameraShadowPolicy;
use worldrunner_world_simulation::presentation::{
    Light, LightKind, LightSettings, LightShadowMode, light_views::extract_lights,
};
use worldrunner_world_simulation::{
    ComponentDesc, LocalTransform, ObjectDesc, World, WorldSettings,
};

fn settings(kind: LightKind, shadows: LightShadowMode, intensity: f32) -> LightSettings {
    LightSettings {
        kind,
        shadows,
        intensity,
        ..LightSettings::default()
    }
}

/// The lights a world with one light per entry presents, each on an object turned by `rotation`.
fn presented(lights: &[(LightSettings, Quat)]) -> Vec<PresentationLight> {
    let mut world = World::new(WorldSettings::default()).unwrap();
    for (settings, rotation) in lights {
        let desc = ObjectDesc {
            local_transform: LocalTransform {
                rotation: *rotation,
                ..LocalTransform::default()
            },
            ..ObjectDesc::default()
        };
        let object = world.create_object(desc).unwrap();
        let light = Light {
            enabled: true,
            settings: *settings,
        };
        world
            .add_component(object, ComponentDesc::Light(light))
            .unwrap();
    }
    extract_lights(&world)
        .unwrap()
        .into_iter()
        .map(|view| PresentationLight {
            object: 0,
            local_to_world: view.local_to_world,
            settings: view.settings,
        })
        .collect()
}

fn sun(intensity: f32) -> LightSettings {
    settings(LightKind::Directional, LightShadowMode::Soft, intensity)
}

#[test]
fn the_main_light_is_the_first_directional_light() {
    let point = settings(LightKind::Point, LightShadowMode::None, 1.0);
    let lights = presented(&[
        (point, Quat::IDENTITY),
        (sun(2.0), Quat::IDENTITY),
        (sun(3.0), Quat::IDENTITY),
    ]);
    assert_eq!(main_light(&lights), Some(&lights[1]));
    assert!(main_light(&lights[..1]).is_none(), "no directional light");
    assert!(main_light(&[]).is_none());
}

#[test]
fn the_sun_shines_along_the_lights_forward_axis_in_the_scene_basis() {
    // Turned 50 degrees about the engine's x axis, the light's forward axis (0, 0, 1) becomes
    // (0, -sin 50, cos 50) in engine coordinates, (0, -cos 50, -sin 50) in the Z-up scene basis.
    // The sun direction points back toward the light.
    let lights = presented(&[(sun(1.5), Quat::from_rotation_x(50_f32.to_radians()))]);
    let sun = scene_sun(&lights[0]).unwrap();
    assert_eq!(
        sun.direction,
        SceneDirection::new([0, 642_788, 766_044]).unwrap()
    );
    assert_eq!(sun.irradiance_micro, 1_500_000);
}

#[test]
fn a_bright_light_is_held_to_the_scenes_largest_irradiance() {
    let lights = presented(&[(sun(25.0), Quat::IDENTITY)]);
    assert_eq!(
        scene_sun(&lights[0]).unwrap().irradiance_micro,
        MAXIMUM_SUN_IRRADIANCE_MICRO
    );
}

#[test]
fn a_light_without_a_forward_axis_gives_no_sun() {
    let mut lights = presented(&[(sun(1.0), Quat::IDENTITY)]);
    lights[0].local_to_world[8..11].copy_from_slice(&[0.0; 3]);
    assert!(scene_sun(&lights[0]).is_err());
}

#[test]
fn only_a_shadow_casting_light_gives_the_fixed_shadow_settings() {
    let hard = settings(LightKind::Directional, LightShadowMode::Hard, 1.0);
    let none = settings(LightKind::Directional, LightShadowMode::None, 1.0);
    let lights = presented(&[
        (sun(1.0), Quat::IDENTITY),
        (hard, Quat::IDENTITY),
        (none, Quat::IDENTITY),
    ]);
    let expected = DirectionalShadowSettings {
        focus_meters: Vec3::ZERO,
        half_extent_meters: 50.0,
        depth_half_extent_meters: 100.0,
        resolution: 2048,
        depth_bias: 0.0005,
        normal_bias_meters: 0.02,
        refresh_policy: CascadeRefreshPolicy::default(),
    };
    assert_eq!(shadow_settings(&lights[0]), Some(expected));
    assert_eq!(shadow_settings(&lights[1]), Some(expected));
    assert_eq!(shadow_settings(&lights[2]), None);
}

#[test]
fn a_lit_frame_carries_the_sun_ambient_light_and_camera_split_cascades() {
    let lights = presented(&[(sun(1.5), Quat::IDENTITY)]);
    let sun = scene_sun(&lights[0]).unwrap();
    let camera = worldrunner_render_plan::SceneCamera {
        vertical_fov_millidegrees: 60_000,
        near_plane_millimeters: 100,
        far_plane_millimeters: 1_000_000,
    };
    let effects = lit_effects(sun, camera).unwrap();
    assert_eq!(*effects.sun(), sun);
    assert_eq!(*effects.camera(), camera);
    assert_eq!(
        (
            effects.ambient().sky_irradiance_micro,
            effects.ambient().ground_irradiance_micro
        ),
        (300_000, 150_000)
    );
    assert_eq!(
        effects.shadows().split_distances_millimeters(),
        Some(&[10_000, 40_000, 150_000][..])
    );
    assert!(effects.ssao().is_none() && effects.post().is_empty());
    assert!(
        effects.atmosphere().is_none(),
        "the game's clear colour stays the background"
    );
}

fn lens() -> SceneCamera {
    SceneCamera {
        vertical_fov_millidegrees: 60_000,
        near_plane_millimeters: 100,
        far_plane_millimeters: 1_000_000,
    }
}

#[test]
fn a_frame_without_a_directional_light_stays_unlit() {
    let point = settings(LightKind::Point, LightShadowMode::Soft, 1.0);
    let lights = presented(&[(point, Quat::IDENTITY)]);
    let (mut lighting, shadows) = SceneLighting::start(&lights, lens()).unwrap();
    assert!(lighting.effects().is_none());
    assert!(shadows.is_none());
    assert!(lighting.update(&lights).unwrap().is_none());
}

#[test]
fn a_shadow_casting_sun_lights_the_first_frame_and_starts_shadows() {
    let lights = presented(&[(sun(1.5), Quat::from_rotation_x(0.5))]);
    let (mut lighting, shadows) = SceneLighting::start(&lights, lens()).unwrap();
    let expected = lit_effects(scene_sun(&lights[0]).unwrap(), lens()).unwrap();
    assert_eq!(lighting.effects(), Some(&expected));
    assert_eq!(shadows, shadow_settings(&lights[0]));
    assert!(shadows.is_some());
    assert!(
        lighting.update(&lights).unwrap().is_none(),
        "unchanged lighting sends nothing"
    );
}

#[test]
fn a_changed_light_sends_new_effects_once() {
    let lights = presented(&[(sun(1.0), Quat::IDENTITY)]);
    let (mut lighting, _) = SceneLighting::start(&lights, lens()).unwrap();
    let brighter = presented(&[(sun(2.0), Quat::IDENTITY)]);
    let sent = lighting.update(&brighter).unwrap().unwrap();
    assert_eq!(sent.sun().irradiance_micro, 2_000_000);
    assert_eq!(lighting.effects(), Some(&sent));
    assert!(lighting.update(&brighter).unwrap().is_none());
}

#[test]
fn a_sun_that_appears_later_lights_the_frame_without_shadows_at_start() {
    let (mut lighting, shadows) = SceneLighting::start(&[], lens()).unwrap();
    assert!(shadows.is_none());
    let lights = presented(&[(sun(1.0), Quat::IDENTITY)]);
    let sent = lighting.update(&lights).unwrap().unwrap();
    assert_eq!(sent.sun().irradiance_micro, 1_000_000);
}

#[test]
fn a_sun_that_goes_away_leaves_its_direction_with_no_irradiance() {
    let lights = presented(&[(sun(1.0), Quat::from_rotation_x(0.5))]);
    let (mut lighting, _) = SceneLighting::start(&lights, lens()).unwrap();
    let direction = lighting.effects().unwrap().sun().direction;
    let dark = lighting.update(&[]).unwrap().unwrap();
    assert_eq!(
        *dark.sun(),
        SceneSun {
            direction,
            irradiance_micro: 0
        }
    );
    assert!(lighting.update(&[]).unwrap().is_none());
}

#[test]
fn a_shadowed_viewer_follows_the_cameras_render_shadows_and_sends_each_change_once() {
    use worldrunner_scene_viewer::CameraShadowPolicy;
    let lights = presented(&[(sun(1.0), Quat::IDENTITY)]);
    let (mut lighting, shadows) = SceneLighting::start(&lights, lens()).unwrap();
    assert!(shadows.is_some());
    let off = lighting.camera_policy(Some(false));
    assert_eq!(off, Some(CameraShadowPolicy::Disabled));
    assert_eq!(
        lighting.camera_policy(Some(false)),
        off,
        "pending until published"
    );
    lighting.published(CameraShadowPolicy::Disabled);
    assert_eq!(lighting.camera_policy(Some(false)), None);
    assert_eq!(
        lighting.camera_policy(None),
        None,
        "no setting keeps the published policy"
    );
    assert_eq!(
        lighting.camera_policy(Some(true)),
        Some(CameraShadowPolicy::Enabled)
    );
}

#[test]
fn a_viewer_started_without_shadows_publishes_no_camera_policy() {
    let point = settings(LightKind::Point, LightShadowMode::Soft, 1.0);
    let (lighting, shadows) =
        SceneLighting::start(&presented(&[(point, Quat::IDENTITY)]), lens()).unwrap();
    assert!(shadows.is_none());
    assert_eq!(lighting.camera_policy(Some(true)), None);
    assert_eq!(lighting.camera_policy(Some(false)), None);
}

fn pose(x: i64) -> CameraPose {
    CameraPose {
        eye_millimeters: [x, 0, 1000],
        target_millimeters: [0; 3],
    }
}

fn shadowed() -> SceneLighting {
    SceneLighting::start(&presented(&[(sun(1.0), Quat::IDENTITY)]), lens())
        .unwrap()
        .0
}

/// Publishes one frame and returns what reached the viewer, if anything.
fn frame(
    publication: &mut CameraPublication,
    lighting: &mut SceneLighting,
    moved: Option<CameraPose>,
    render_shadows: Option<bool>,
) -> Option<(CameraPose, Option<CameraShadowPolicy>)> {
    let mut sent = None;
    publication
        .publish(lighting, moved, render_shadows, |pose, policy| {
            sent = Some((pose, policy));
            Ok::<(), ()>(())
        })
        .unwrap();
    sent
}

#[test]
fn render_shadows_off_reaches_the_viewer_at_its_opening_pose_when_the_game_shows_no_camera() {
    use CameraShadowPolicy::Disabled;
    let (mut lighting, mut publication) = (shadowed(), CameraPublication::new(pose(1)));
    let mut next =
        |moved, render_shadows| frame(&mut publication, &mut lighting, moved, render_shadows);
    assert_eq!(next(None, Some(false)), Some((pose(1), Some(Disabled))));
    assert_eq!(next(None, Some(false)), None, "sent once");
    assert_eq!(
        next(None, None),
        None,
        "no setting keeps the published policy"
    );
}

#[test]
fn a_camera_sequence_sends_each_pose_and_each_policy_change_once() {
    use CameraShadowPolicy::{Disabled, Enabled};
    let (mut lighting, mut publication) = (shadowed(), CameraPublication::new(pose(1)));
    let mut next =
        |moved, render_shadows| frame(&mut publication, &mut lighting, moved, render_shadows);
    assert_eq!(
        next(None, Some(true)),
        Some((pose(1), Some(Enabled))),
        "initial on"
    );
    assert_eq!(next(None, Some(true)), None, "not repeated");
    assert_eq!(
        next(Some(pose(2)), Some(true)),
        Some((pose(2), None)),
        "the game camera appears"
    );
    assert_eq!(
        next(None, Some(false)),
        Some((pose(2), Some(Disabled))),
        "a policy change keeps the moved pose"
    );
    assert_eq!(
        next(Some(pose(3)), Some(true)),
        Some((pose(3), Some(Enabled)))
    );
}

#[test]
fn a_refused_camera_records_nothing_and_the_next_frame_sends_it_again() {
    use CameraShadowPolicy::Disabled;
    let (mut lighting, mut publication) = (shadowed(), CameraPublication::new(pose(1)));
    let refused = publication.publish(&mut lighting, Some(pose(2)), Some(false), |_, _| {
        Err("refused")
    });
    assert_eq!(refused, Err("refused"));
    assert_eq!(
        frame(&mut publication, &mut lighting, None, Some(false)),
        Some((pose(1), Some(Disabled))),
        "neither the pose nor the policy was recorded"
    );
}

#[test]
fn a_viewer_started_without_shadows_sends_only_moves() {
    let point = settings(LightKind::Point, LightShadowMode::Soft, 1.0);
    let mut lighting = SceneLighting::start(&presented(&[(point, Quat::IDENTITY)]), lens())
        .unwrap()
        .0;
    let mut publication = CameraPublication::new(pose(1));
    assert_eq!(
        frame(&mut publication, &mut lighting, None, Some(false)),
        None
    );
    assert_eq!(
        frame(&mut publication, &mut lighting, Some(pose(2)), Some(true)),
        Some((pose(2), None))
    );
}
