#[test]
fn selected_effects_supply_initial_capture_readiness() {
    let mut options = crate::tests::options();
    options.size = [64, 32];
    let mut host = RetainedHost::with_initial_content(options, |_| {
        let scene = crate::tests::scene(false);
        let effects = scene
            .effects()
            .unwrap()
            .clone()
            .with_ssr(worldrunner_render_plan::SceneSsr::default())
            .unwrap();
        let scene = scene
            .with_objects_and_effects(vec![], Some(effects))
            .unwrap();
        Ok(InitialContent {
            scene,
            settings: Some(super::tests::settings()),
        })
    })
    .unwrap();
    host.request_capture_recipe(0, recipe(CaptureOutput::SsrConfidenceOpaque))
        .unwrap();
    host.resolve_initial_content(InitialContext {
        adapter: &identity(),
        surface_extent: [64, 32],
    })
    .unwrap();
    assert!(host.readiness.capture_features[3]);
    assert!(!host.readiness.capture_features[4]);
    assert_eq!(host.captures.recipes[&0].frame_samples, 4);
    assert_eq!(host.accepted_generation(), 0);
}

#[test]
fn supplied_object_and_decal_ids_start_at_authored_indices() {
    let mut host = RetainedHost::with_initial_content(crate::tests::options(), |_| {
        Ok(InitialContent {
            scene: content_scene(),
            settings: None,
        })
    })
    .unwrap();
    assert_eq!(host.object(crate::ObjectId(0)), None);
    host.resolve_initial_content(InitialContext {
        adapter: &identity(),
        surface_extent: [64, 32],
    })
    .unwrap();
    let expected = content_scene();
    for (index, object) in expected.geometry().objects().iter().enumerate() {
        assert_eq!(host.object(crate::ObjectId(index as u64)), Some(*object));
    }
    assert_eq!(host.accepted_generation(), 0);
    assert_eq!(host.ready_generation(), 0);
    host.apply(crate::RetainedUpdate::Decal(crate::DecalUpdate::Remove(0)))
        .unwrap();
    assert!(host.scene().decals().is_empty());
}

fn content_scene() -> worldrunner_render_plan::RenderScene {
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
    crate::renderer::retained_tests::source()
        .with_decals(vec![decal])
        .unwrap()
}

#[test]
fn concrete_startup_without_callback_preserves_scene_options_and_pool() {
    let scene = crate::renderer::retained_tests::source();
    let mut host = crate::RetainedHost::new(scene.clone(), crate::tests::options()).unwrap();
    let options = format!("{:?}", host.options());
    let pool = host.capture_pool_bytes();
    host.resolve_initial_content(InitialContext {
        adapter: &identity(),
        surface_extent: [64, 32],
    })
    .unwrap();
    assert_eq!(host.initial_scene(), &scene);
    assert_eq!(host.scene(), &scene);
    assert_eq!(format!("{:?}", host.options()), options);
    assert_eq!(host.capture_pool_bytes(), pool);
    assert_eq!(host.accepted_generation(), 0);
}

#[test]
fn deferred_capture_admission_preserves_generation_and_capacity_checks() {
    let mut host = RetainedHost::with_initial_content(crate::tests::options(), |_| {
        Ok(InitialContent {
            scene: crate::tests::scene(false),
            settings: None,
        })
    })
    .unwrap()
    .with_capture_limit(1)
    .unwrap();
    assert!(matches!(
        host.request_capture(1),
        Err(crate::CaptureRequestError::UnknownGeneration { generation: 1 })
    ));
    assert_eq!(host.request_capture(0).unwrap(), 0);
    assert!(matches!(
        host.request_capture(0),
        Err(crate::CaptureRequestError::BackPressure { limit: 1 })
    ));
    assert!(matches!(
        host.apply_with_pages(crate::RetainedUpdate::Objects(vec![])),
        Err(ViewerError::InitialSceneUnavailable)
    ));
    host.resolve_initial_content(InitialContext {
        adapter: &identity(),
        surface_extent: [64, 32],
    })
    .unwrap();
    assert_eq!(host.captures.waiting.len(), 1);
    assert!(host.view().unwrap().is_some());
}

#[test]
fn selected_invalid_options_and_page_dependencies_fail_startup() {
    let mut host = RetainedHost::with_initial_content(crate::tests::options(), |_| {
        let settings = FrameSettings {
            presentation_budget: crate::PresentationBudget(0).into(),
            ..super::tests::settings()
        };
        Ok(InitialContent {
            scene: crate::tests::scene(false),
            settings: Some(settings),
        })
    })
    .unwrap();
    assert!(matches!(
        host.resolve_initial_content(InitialContext {
            adapter: &identity(),
            surface_extent: [64, 32]
        }),
        Err(ViewerError::InvalidOptions(_))
    ));
    assert!(host.initial_scene.is_none());
    let mut host = RetainedHost::with_initial_content(crate::tests::options(), |_| {
        Ok(InitialContent {
            scene: crate::tests::scene(false),
            settings: None,
        })
    })
    .unwrap();
    host.request_capture_with_pages(0).unwrap();
    assert!(matches!(
        host.resolve_initial_content(InitialContext {
            adapter: &identity(),
            surface_extent: [64, 32]
        }),
        Err(ViewerError::Capture(
            crate::CaptureRequestError::PagingDisabled { generation: 0 }
        ))
    ));
}

use super::*;
use crate::{CaptureJitter, CaptureOutput, RetainedCaptureRecipe, RetainedHost, ViewerError};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

fn identity() -> AdapterIdentity {
    AdapterIdentity {
        name: "selected".into(),
        class: AdapterClass::Integrated,
        backend: wgpu::Backend::Metal,
    }
}

#[test]
fn supplier_is_deferred_once_and_uses_selected_settings() {
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = calls.clone();
    let mut options = crate::tests::options();
    options.size = [64, 32];
    options.capture_pool = crate::CapturePoolPolicy::Derived {
        recipes: vec![recipe(CaptureOutput::LinearHdr)],
        max_extent: options.size,
        headroom_bytes: 0,
        granularity_bytes: 1,
        cap_bytes: None,
    };
    let mut host = RetainedHost::with_initial_content(options, move |selected| {
        assert_eq!(selected.adapter, &identity());
        observed.fetch_add(1, Ordering::SeqCst);
        Ok(InitialContent {
            scene: crate::renderer::retained_tests::source(),
            settings: Some(super::tests::settings()),
        })
    })
    .unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(host.objects().count(), 0);
    assert!(host.view().unwrap().is_none());
    assert!(matches!(
        host.apply(crate::RetainedUpdate::Objects(vec![])),
        Err(ViewerError::InitialSceneUnavailable)
    ));
    host.request_capture_recipe(0, recipe(CaptureOutput::LinearHdr))
        .unwrap();
    host.resolve_initial_content(InitialContext {
        adapter: &identity(),
        surface_extent: [64, 32],
    })
    .unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        host.objects().count(),
        crate::renderer::retained_tests::source()
            .geometry()
            .objects()
            .len()
    );
    assert_eq!(host.options().shadows, super::tests::settings().shadows);
    assert_eq!(
        host.options().frame_profile,
        super::tests::settings().frame_profile
    );
    let expected = host
        .options
        .capture_pool
        .effective_bytes(4, wgpu::TextureFormat::R32Float)
        .unwrap();
    assert_eq!(host.capture_pool_bytes(), expected);
    assert_eq!(host.captures.recipes[&0].frame_samples, 4);
}

fn recipe(output: CaptureOutput) -> RetainedCaptureRecipe {
    RetainedCaptureRecipe::new(
        1,
        1,
        CaptureJitter::None,
        false,
        vec![output],
        64 * 1024 * 1024,
    )
    .unwrap()
}

#[test]
fn supplier_failure_is_typed_and_not_repeated() {
    let mut host =
        RetainedHost::with_initial_content(crate::tests::options(), |_| Err("rejected".into()))
            .unwrap();
    assert!(
        matches!(host.resolve_initial_content(InitialContext { adapter: &identity(), surface_extent: [64, 32] }), Err(ViewerError::InitialContent(message)) if message == "rejected")
    );
    assert!(matches!(
        host.resolve_initial_content(InitialContext {
            adapter: &identity(),
            surface_extent: [64, 32]
        }),
        Err(ViewerError::InitialSceneUnavailable)
    ));
}

#[test]
fn supplier_validates_selected_scene_and_pending_captures() {
    let mut host = RetainedHost::with_initial_content(crate::tests::options(), |_| {
        Ok(InitialContent {
            scene: crate::tests::scene(true),
            settings: None,
        })
    })
    .unwrap();
    assert!(matches!(
        host.resolve_initial_content(InitialContext {
            adapter: &identity(),
            surface_extent: [64, 32]
        }),
        Err(ViewerError::UnsupportedScene("water requires HDR"))
    ));
    let mut host = RetainedHost::with_initial_content(crate::tests::options(), |_| {
        Ok(InitialContent {
            scene: crate::tests::scene(false),
            settings: None,
        })
    })
    .unwrap();
    host.request_capture_recipe(0, recipe(CaptureOutput::SsrConfidenceOpaque))
        .unwrap();
    assert!(matches!(
        host.resolve_initial_content(InitialContext {
            adapter: &identity(),
            surface_extent: [64, 32]
        }),
        Err(ViewerError::Capture(
            crate::CaptureRequestError::FeatureUnavailable { .. }
        ))
    ));
}
