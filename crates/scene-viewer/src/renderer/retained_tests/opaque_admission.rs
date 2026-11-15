use super::*;
use crate::{RetainedHost, RetainedUpdate};
use worldrunner_render_plan::SceneOpaquePath;
use worldrunner_render_scene::compositor::opaque::OpaqueAdmissionError;

fn render_effects() -> SceneEffects {
    source()
        .effects()
        .unwrap()
        .clone()
        .with_opaque_path(SceneOpaquePath::GBUFFER_RENDER)
        .unwrap()
}

fn profile() -> crate::FrameProfile {
    crate::FrameProfile::Hdr {
        samples: 1,
        tone_map: crate::ToneMap::Reinhard,
    }
}

#[test]
fn opposing_settings_refusal_preserves_cpu_scene_and_all_generations() {
    let mut host = RetainedHost::new(
        source(),
        crate::ViewerOptions {
            frame_profile: profile(),
            shadows: Some(shadow_settings()),
            ..crate::tests::options()
        },
    )
    .unwrap();
    let scene = host.scene().clone();
    let objects: Vec<_> = host.objects().collect();
    let error = host
        .apply(RetainedUpdate::Settings(Box::new(render_effects())))
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains(&OpaqueAdmissionError::UnsupportedOpposingReceiver.to_string())
    );
    assert_eq!(host.scene(), &scene);
    assert_eq!(host.objects().collect::<Vec<_>>(), objects);
    assert_eq!(host.accepted_generation(), 0);
    assert_eq!(host.ready_generation(), 0);
    assert_eq!(host.readiness.published, 0);
    assert_eq!(host.preparations_in_flight(), 0);
    host.apply(RetainedUpdate::Settings(Box::new(
        scene.effects().unwrap().clone(),
    )))
    .unwrap();
    assert_eq!(host.accepted_generation(), 1);
}

#[test]
fn zero_bias_front_accepts_render_settings() {
    let shadows = DirectionalShadowSettings {
        normal_bias_meters: 0.0,
        ..shadow_settings()
    };
    let mut host = RetainedHost::new(
        source(),
        crate::ViewerOptions {
            frame_profile: profile(),
            shadows: Some(shadows),
            ..crate::tests::options()
        },
    )
    .unwrap();
    host.apply(RetainedUpdate::Settings(Box::new(render_effects())))
        .unwrap();
    assert_eq!(host.accepted_generation(), 1);
    assert_eq!(
        host.scene().effects().unwrap().opaque_path(),
        SceneOpaquePath::GBUFFER_RENDER
    );
}

#[test]
fn refused_resident_route_change_precedes_effects_and_scheduler_mutation() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let context = GpuContext::shared_for_test_with_features(wgpu::Features::empty()).unwrap();
    let source = source();
    let mut resident = Resident::new(&context, &source, None, profile()).unwrap();
    let size = Viewport {
        width: 64,
        height: 64,
    };
    let mut presentation = super::super::Presentation::with_profile(
        &context,
        crate::presentation::COLOR,
        size,
        size,
        profile(),
        source.effects(),
    )
    .unwrap();
    let generation = presentation.scheduler.generation();
    let color = presentation.targets.color.texture().clone();
    let handles: Vec<_> = resident.scene_mut().instance_handles().collect();
    let before = resident.scene_mut().statistics();
    let mut changed = render_effects();
    changed = changed
        .with_ssr(worldrunner_render_plan::SceneSsr::default())
        .unwrap();
    let error = super::super::retained::configure_effects(
        &context,
        &mut resident,
        &mut presentation,
        &changed,
    )
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains(&OpaqueAdmissionError::UnsupportedOpposingReceiver.to_string())
    );
    assert_eq!(presentation.scheduler.generation(), generation);
    assert_eq!(
        presentation.scheduler.configuration().opaque_path,
        SceneOpaquePath::Forward
    );
    assert_eq!(presentation.targets.color.texture(), &color);
    assert_eq!(
        resident.scene_mut().instance_handles().collect::<Vec<_>>(),
        handles
    );
    assert_eq!(resident.scene_mut().statistics().textures, before.textures);
    if let Resident::Plain(plain) = &resident {
        assert_eq!(plain.effects.as_ref(), source.effects());
    } else {
        panic!("plain fixture changed resident kind");
    }
}
