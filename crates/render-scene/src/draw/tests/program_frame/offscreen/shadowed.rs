//! The player's offscreen work in a shadowed frame. A window whose lights choose shadows draws
//! through the shadowed path, so that path takes the admitted operations and runs them in the
//! frame's one submission, as the plain path does.
use super::*;
use crate::draw::PreparedScene;
use crate::shadows::ShadowedTarget;
use crate::shadows::{DirectionalShadowSettings, ShadowedScene};
use worldrunner_render_post::AttachmentView;

/// The engine's card under the default lit effects, which a shadowed scene needs.
fn lit() -> RenderScene {
    let scene = engine_scene();
    let effects = SceneEffects::new(effect_parts()).unwrap();
    scene
        .with_objects_and_effects(scene.geometry().objects().to_vec(), Some(effects))
        .unwrap()
}

fn shadowed(fixture: &Fixture) -> ShadowedScene {
    ShadowedScene::new(
        &fixture.context,
        PreparedScene::new(&lit()).unwrap(),
        DirectionalShadowSettings {
            focus_meters: glam::Vec3::new(0.0, 0.0, 1.0),
            half_extent_meters: 4.0,
            depth_half_extent_meters: 4.0,
            resolution: 64,
            depth_bias: 0.001,
            normal_bias_meters: 0.01,
            refresh_policy: worldrunner_render_shadow::CascadeRefreshPolicy::default(),
        },
        ShadowedTarget::new(SCENE_FORMAT, 1),
    )
    .unwrap()
}

#[test]
fn the_shadowed_path_runs_the_players_offscreen_work_in_the_frames_one_submission() {
    let mut fixture = timed();
    let mut player = player(&mut fixture);
    let mut scene = shadowed(&fixture);
    let authority = fixture.context.submission().clone();
    let device = fixture.context.device().clone();
    let before = authority.submitted_serial();

    let (operations, readbacks) = build(&mut player);
    let mut recording = authority.begin(&device).unwrap();
    fixture.renderer.begin_timing(7).unwrap();
    scene
        .scene_mut()
        .admit_offscreen(operations.admit(&recording).unwrap());
    let color = AttachmentView::new(
        ResourceRef::new("scene color"),
        fixture.renderer.targets().scene().texture(),
    )
    .unwrap();
    let depth = AttachmentView::new(
        ResourceRef::new("scene depth"),
        fixture.renderer.targets().depth().texture(),
    )
    .unwrap();
    fixture
        .renderer
        .encoding()
        .encode_shadowed(
            &fixture.context,
            &super::super::rig::view(),
            (&color, &depth),
            &mut recording,
            &mut scene,
        )
        .unwrap();
    assert!(
        !scene.scene_mut().has_offscreen(),
        "the frame took the offscreen work"
    );
    assert_eq!(
        authority.submitted_serial(),
        before,
        "recording must not submit"
    );
    recording.finish().submit().unwrap();
    assert_eq!(
        authority.submitted_serial(),
        before + 1,
        "one frame, one submission"
    );
    check_offscreen_stages(readbacks, &device);
}
