#[test]
fn supplier_ssr_parameters_publish_without_rebuilding_after_startup() {
    let mut options = crate::tests::options();
    options.size = [64, 32];
    let mut host = RetainedHost::with_initial_content(options, |_| {
        let source = super::retained_tests::source();
        let effects = source
            .effects()
            .unwrap()
            .clone()
            .with_ssr(worldrunner_render_plan::SceneSsr::default())
            .unwrap();
        Ok(InitialContent {
            scene: source
                .with_objects_and_effects(source.geometry().objects().to_vec(), Some(effects))
                .unwrap(),
            settings: Some(super::adapter_frame_tests::selected_frame()),
        })
    })
    .unwrap();
    let mut renderer = pollster::block_on(Renderer::windowless(&mut host)).unwrap();
    assert!(renderer.render_retained(&mut host).unwrap());
    let effects = host.scene().effects().unwrap().clone();
    let next = effects
        .clone()
        .with_ssr(worldrunner_render_plan::SceneSsr {
            max_distance_millimeters: 12_000,
            ..*effects.ssr().unwrap()
        })
        .unwrap();
    host.apply(crate::RetainedUpdate::Settings(Box::new(next.clone())))
        .unwrap();
    assert!(host.readiness.queue.back().unwrap().scene.is_none());
    assert!(renderer.render_retained(&mut host).unwrap());
    assert_eq!(host.ready_generation(), host.accepted_generation());
    assert!(renderer.streaming.retired.is_empty());
    assert_eq!(
        renderer.presentation.scheduler.configuration().ssr.as_ref(),
        next.ssr()
    );
    renderer.drain().unwrap();
}

#[test]
fn concrete_startup_reuses_object_and_decal_storage() {
    let scene = storage_scene();
    let mut options = crate::tests::options();
    let adapter = startup_adapter();
    let size = options.size;
    let resolved = super::startup::resolve(
        super::SceneSource::Static {
            scene: &scene,
            options: &mut options,
        },
        &adapter,
        size,
    )
    .unwrap();
    assert_storage(&scene, &resolved.scene);
}

#[test]
fn retained_and_supplier_startup_borrow_stored_scene() {
    let adapter = startup_adapter();
    for supplier in [false, true] {
        let mut host = if supplier {
            RetainedHost::with_initial_content(crate::tests::options(), |_| {
                Ok(InitialContent {
                    scene: storage_scene(),
                    settings: None,
                })
            })
        } else {
            RetainedHost::new(storage_scene(), crate::tests::options())
        }
        .unwrap();
        let resolved =
            super::startup::resolve(super::SceneSource::Retained(&mut host), &adapter, [64, 32])
                .unwrap();
        let objects = resolved.scene.geometry().objects().as_ptr();
        let decals = resolved.scene.decals().as_ptr();
        drop(resolved);
        assert_eq!(host.initial_scene().geometry().objects().as_ptr(), objects);
        assert_eq!(host.initial_scene().decals().as_ptr(), decals);
        assert!(!host.initial_scene().geometry().objects().is_empty());
        assert!(!host.initial_scene().decals().is_empty());
    }
}

fn startup_adapter() -> wgpu::Adapter {
    worldrunner_render_gpu::test_support::TestGpu::shared_adapter().unwrap()
}

fn assert_storage(expected: &super::RenderScene, actual: &super::RenderScene) {
    assert!(!expected.geometry().objects().is_empty());
    assert!(!expected.decals().is_empty());
    assert_eq!(
        expected.geometry().objects().as_ptr(),
        actual.geometry().objects().as_ptr()
    );
    assert_eq!(expected.decals().as_ptr(), actual.decals().as_ptr());
}

fn storage_scene() -> super::RenderScene {
    use worldrunner_render_plan::*;
    let decal = SceneDecal {
        priority: 0,
        transform: SceneTransform {
            position_millimeters: [0; 3],
            rotation: SceneRotation::IDENTITY,
            scale_millionths: 1_000_000,
        },
        dimensions_millimeters: [1000; 3],
        material: MaterialHandle::from_raw(0),
        axis: SceneDecalAxis::Z,
        edge_fade_micro: 0,
        angle_cutoff_micro: 0,
        angle_fade_micro: 0,
        receivers: SceneDecalReceivers::ALL,
        mapping: SceneDecalMapping::default(),
        write_mask: SceneDecalWriteMask::default(),
        alpha_test_cutoff_micro: None,
        color_modifier: SceneDecalColorModifier::default(),
    };
    super::retained_tests::source()
        .with_decals(vec![decal])
        .unwrap()
}

#[test]
fn selected_invalid_shadows_fail_before_device_creation() {
    let mut host = crate::RetainedHost::with_initial_content(crate::tests::options(), |_| {
        let mut frame = super::adapter_frame_tests::selected_frame();
        let mut shadows = super::retained_tests::shadow_settings();
        shadows.resolution = 0;
        frame.shadows = Some(shadows);
        Ok(crate::InitialContent {
            scene: crate::tests::scene(false),
            settings: Some(frame),
        })
    })
    .unwrap();
    assert!(matches!(
        pollster::block_on(super::windowless::windowless_context(&mut host)),
        Err(crate::ViewerError::Shadow(_))
    ));
}

use super::{Renderer, Resident};
use crate::{InitialContent, RetainedHost};
use std::sync::{Arc, Mutex};

#[test]
fn supplier_first_residency_uses_selected_content_frame_and_shadows() {
    let observed = Arc::new(Mutex::new(Vec::new()));
    let received = observed.clone();
    let mut options = crate::tests::options();
    options.size = [64, 32];
    let mut host = RetainedHost::with_initial_content(options, move |context| {
        assert_eq!(context.surface_extent, [64, 32]);
        received.lock().unwrap().push(context.adapter.clone());
        let mut settings = super::adapter_frame_tests::selected_frame();
        settings.shadows = Some(super::retained_tests::shadow_settings());
        Ok(InitialContent {
            scene: super::retained_tests::source(),
            settings: Some(settings),
        })
    })
    .unwrap();
    let mut renderer = pollster::block_on(Renderer::windowless(&mut host)).unwrap();
    assert_eq!(
        *observed.lock().unwrap(),
        vec![renderer.context.adapter_identity()]
    );
    assert_eq!(
        host.objects().count(),
        super::retained_tests::source().geometry().objects().len()
    );
    assert_eq!(host.initial_scene(), &super::retained_tests::source());
    let frame = renderer.presentation.scheduler.configuration().frame;
    assert_eq!(frame.samples, 4);
    assert_eq!(frame.size, [32, 16]);
    assert!(matches!(renderer.resident, Resident::Shadowed(_)));
    assert_eq!(
        renderer.resident.scene_mut().instance_handles().count(),
        host.objects().count()
    );
    assert!(
        renderer
            .resident
            .scene_mut()
            .statistics()
            .mesh_buffers
            .uploaded_bytes
            > 0
    );
    assert!(renderer.render_retained(&mut host).unwrap());
    renderer.drain().unwrap();
}

#[test]
fn supplier_selected_scene_determines_required_features_before_preparation() {
    use worldrunner_render_plan::{SceneGeometry, SceneParts, SceneResources, SceneTexture};
    use worldrunner_render_plan::{TextureColorSpace, TextureEncoding};
    let mut options = crate::tests::options();
    options.size = [8, 8];
    let mut host = RetainedHost::with_initial_content(options, |_| {
        let scene = worldrunner_render_plan::RenderScene::new(SceneParts {
            resources: SceneResources::new(
                vec![SceneTexture::Encoded {
                    encoding: TextureEncoding::DdsContainer,
                    color_space: TextureColorSpace::Srgb,
                    bytes: vec![1, 2, 3].into_boxed_slice(),
                }],
                vec![],
            )
            .unwrap(),
            geometry: SceneGeometry::new(vec![], vec![]).unwrap(),
            terrain: None,
            effects: None,
        })
        .unwrap();
        Ok(InitialContent {
            scene,
            settings: None,
        })
    })
    .unwrap();
    let result = pollster::block_on(super::windowless::windowless_context(&mut host));
    let (_, context) = result.unwrap();
    assert!(
        context
            .device()
            .features()
            .contains(wgpu::Features::TEXTURE_COMPRESSION_BC)
    );
    assert_eq!(
        super::required_features(host.initial_scene()),
        wgpu::Features::TEXTURE_COMPRESSION_BC
    );
}
