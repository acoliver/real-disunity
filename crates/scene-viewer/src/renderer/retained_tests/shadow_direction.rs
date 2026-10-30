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

#[test]
fn retained_shadow_direction_changes_pixels_without_rebuilding_geometry() {
    let context =
        pollster::block_on(GpuContext::request_with_features(wgpu::Features::empty())).unwrap();
    let base = source();
    let scene = shadow_scene(&base);
    let settings = Some(shadow_settings());
    let mut host = crate::RetainedHost::new(scene.clone(), crate::tests::options()).unwrap();
    let mut resident =
        Resident::new(&context, &scene, settings, crate::FrameProfile::Legacy).unwrap();
    let mut streaming =
        super::super::readiness::Streaming::new(settings, crate::FrameProfile::Legacy);
    let mut target = OffscreenTarget::new(
        context.device(),
        Viewport {
            width: 64,
            height: 64,
        },
    )
    .unwrap();
    let initial = frame(&mut resident, &context, &mut target);
    let uploads = resident.scene_mut().take_statistics();
    assert!(uploads.mesh_buffers.uploaded_bytes > 0);
    assert!(uploads.textures.uploaded_bytes > 0);
    let handles: Vec<_> = resident.scene_mut().instance_handles().collect();
    let effects = scene
        .effects()
        .unwrap()
        .clone()
        .with_shadow_direction(Some(SceneDirection::new([-600_000, 0, 800_000]).unwrap()));
    host.apply(crate::RetainedUpdate::Settings(Box::new(effects)))
        .unwrap();
    publish(&context, &mut streaming, &mut resident, &mut host);
    assert_eq!(
        resident.scene_mut().instance_handles().collect::<Vec<_>>(),
        handles
    );
    assert_eq!(host.ready_generation(), 1);
    assert_ne!(frame(&mut resident, &context, &mut target), initial);
    let uploads = resident.scene_mut().take_statistics();
    assert_eq!(uploads.mesh_buffers.uploaded_bytes, 0);
    assert_eq!(uploads.textures.uploaded_bytes, 0);
    assert!(
        resident
            .scene_mut()
            .flush_instances(context.device(), context.submission())
            .unwrap()
            .is_empty()
    );
}

fn shadow_scene(base: &RenderScene) -> RenderScene {
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
    base.with_objects_and_effects(
        vec![original, caster],
        Some(
            shadow_effects(base.effects().unwrap())
                .with_shadow_direction(Some(SceneDirection::new([600_000, 0, 800_000]).unwrap())),
        ),
    )
    .unwrap()
}
