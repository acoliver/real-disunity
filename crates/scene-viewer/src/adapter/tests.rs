use super::*;
use crate::{FrameProfile, PresentationBudget, ToneMap, ViewerError};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

fn identity() -> AdapterIdentity {
    AdapterIdentity {
        name: "selected adapter".into(),
        class: AdapterClass::Integrated,
        backend: wgpu::Backend::Metal,
    }
}

pub(super) fn settings() -> FrameSettings {
    FrameSettings {
        shadows: Some(crate::renderer::retained_tests::shadow_settings()),
        frame_profile: FrameProfile::Hdr {
            samples: 4,
            tone_map: ToneMap::Reinhard,
        },
        presentation_budget: PresentationBudget(512 * 1024 * 1024).into(),
    }
}

#[test]
fn adapter_frame_resolution_applies_settings_once() {
    let calls = Arc::new(AtomicUsize::new(0));
    let count = calls.clone();
    let mut options = crate::tests::options();
    options.adapter_callback = Some(AdapterCallback::new(move |selected| {
        assert_eq!(selected, &identity());
        count.fetch_add(1, Ordering::SeqCst);
        Ok(Some(settings()))
    }));
    resolve(&identity(), &crate::tests::scene(false), &mut options).unwrap();
    assert_eq!(options.frame_profile, settings().frame_profile);
    assert_eq!(options.presentation_budget, settings().presentation_budget);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn adapter_frame_none_and_absent_callback_preserve_options() {
    for callback in [None, Some(AdapterCallback::new(|_| Ok(None)))] {
        let mut options = crate::tests::options();
        options.adapter_callback = callback;
        let original = format!("{options:?}");
        resolve(&identity(), &crate::tests::scene(false), &mut options).unwrap();
        assert_eq!(format!("{options:?}"), original);
    }
}

#[test]
fn adapter_frame_resolution_reuses_option_validation() {
    let mut options = crate::tests::options();
    options.adapter_callback = Some(AdapterCallback::new(|_| {
        Ok(Some(FrameSettings {
            presentation_budget: PresentationBudget(0).into(),
            ..settings()
        }))
    }));
    assert!(matches!(
        resolve(&identity(), &crate::tests::scene(false), &mut options),
        Err(ViewerError::InvalidOptions(
            "presentation allocation budget must be positive"
        ))
    ));
}

#[test]
fn adapter_frame_callback_error_is_typed() {
    let mut options = crate::tests::options();
    options.adapter_callback = Some(AdapterCallback::new(|_| Err("caller rejected".into())));
    assert!(
        matches!(resolve(&identity(), &crate::tests::scene(false), &mut options),
        Err(ViewerError::Driver(message)) if message == "caller rejected")
    );
}

fn recipe() -> crate::RetainedCaptureRecipe {
    crate::RetainedCaptureRecipe::new(
        1,
        1,
        crate::CaptureJitter::None,
        false,
        vec![crate::CaptureOutput::SsrConfidenceOpaque],
        1024 * 1024,
    )
    .unwrap()
}

#[test]
fn adapter_frame_resolution_rejects_scene_incompatible_profile() {
    let mut options = crate::tests::options();
    options.frame_profile = settings().frame_profile;
    options.adapter_callback = Some(AdapterCallback::new(|_| {
        Ok(Some(FrameSettings {
            frame_profile: FrameProfile::Legacy,
            ..settings()
        }))
    }));
    assert!(matches!(
        resolve(&identity(), &crate::tests::scene(true), &mut options),
        Err(ViewerError::UnsupportedScene("water requires HDR"))
    ));
    assert_eq!(options.frame_profile, settings().frame_profile);
}

#[test]
fn adapter_frame_derived_pool_uses_resolved_sample_count() {
    let mut options = crate::tests::options();
    options.size = [64, 32];
    options.capture_pool = crate::CapturePoolPolicy::Derived {
        recipes: vec![recipe()],
        max_extent: options.size,
        headroom_bytes: 0,
        granularity_bytes: 1,
        cap_bytes: None,
    };
    options.adapter_callback = Some(AdapterCallback::new(|_| Ok(Some(settings()))));
    let mut host = crate::RetainedHost::new(crate::tests::scene(false), options).unwrap();
    let initial = host.capture_pool_bytes();
    host.captures.recipes.insert(0, recipe());
    resolve_startup(
        &identity(),
        host.initial_scene.as_ref().unwrap(),
        &mut host.options,
        Some((&mut host.capture_pool, &mut host.captures, &host.readiness)),
    )
    .unwrap();
    let expected = host
        .options
        .capture_pool
        .effective_bytes(4, wgpu::TextureFormat::R32Float)
        .unwrap();
    assert!(expected > initial);
    assert_eq!(host.capture_pool_bytes(), expected);
    assert_eq!(host.captures.recipes[&0].frame_samples, 4);
}

#[test]
fn adapter_frame_none_preserves_existing_pool() {
    let mut host =
        crate::RetainedHost::new(crate::tests::scene(false), crate::tests::options()).unwrap();
    host.options.adapter_callback = Some(AdapterCallback::new(|_| Ok(None)));
    host.options.capture_pool = crate::CapturePoolPolicy::Fixed(17);
    let before = host.capture_pool_bytes();
    resolve_startup(
        &identity(),
        host.initial_scene.as_ref().unwrap(),
        &mut host.options,
        Some((&mut host.capture_pool, &mut host.captures, &host.readiness)),
    )
    .unwrap();
    assert_eq!(host.capture_pool_bytes(), before);
}

#[test]
fn adapter_frame_selected_budget_rejects_descriptor_plan_without_gpu() {
    let mut options = crate::tests::options();
    options.adapter_callback = Some(AdapterCallback::new(|_| {
        Ok(Some(FrameSettings {
            presentation_budget: PresentationBudget(1).into(),
            ..settings()
        }))
    }));
    resolve(&identity(), &crate::tests::scene(false), &mut options).unwrap();
    let limits = wgpu::Limits::default();
    let plan = crate::plan_presentation(
        options.size,
        &options,
        None,
        &limits,
        wgpu::Features::empty(),
        crate::PresentationPlanning {
            capture: crate::PresentationCapture::Retained,
            retained_effect_history: false,
        },
    )
    .unwrap();
    let required = plan.validate(&limits).unwrap();
    assert!(matches!(
        options
            .presentation_budget
            .allocation_policy()
            .resolve::<()>(required),
        Err(worldrunner_render_gpu::configuration::ConfigurationError::Budget { budget: 1, .. })
    ));
}

#[test]
fn adapter_frame_rejects_prequeued_hdr_capture_when_resolving_legacy() {
    let mut options = crate::tests::options();
    options.frame_profile = settings().frame_profile;
    options.adapter_callback = Some(AdapterCallback::new(|_| {
        Ok(Some(FrameSettings {
            frame_profile: FrameProfile::Legacy,
            ..settings()
        }))
    }));
    let mut host = crate::RetainedHost::new(crate::tests::scene(false), options).unwrap();
    let recipe = crate::RetainedCaptureRecipe::new(
        1,
        1,
        crate::CaptureJitter::None,
        false,
        vec![crate::CaptureOutput::LinearHdr],
        64 * 1024 * 1024,
    )
    .unwrap();
    host.request_capture_recipe(0, recipe).unwrap();
    assert!(matches!(
        resolve_startup(
            &identity(),
            host.initial_scene.as_ref().unwrap(),
            &mut host.options,
            Some((&mut host.capture_pool, &mut host.captures, &host.readiness,))
        ),
        Err(ViewerError::Capture(
            crate::CaptureRequestError::HdrRequired
        ))
    ));
}

fn retained_host(profile: FrameProfile) -> crate::RetainedHost {
    let mut options = crate::tests::options();
    options.frame_profile = settings().frame_profile;
    options.adapter_callback = Some(AdapterCallback::new(move |_| {
        Ok(Some(FrameSettings {
            frame_profile: profile,
            ..settings()
        }))
    }));
    crate::RetainedHost::new(crate::tests::scene(false), options).unwrap()
}

fn queue_ssr(host: &mut crate::RetainedHost) {
    let effects = host
        .scene()
        .effects()
        .unwrap()
        .clone()
        .with_ssr(worldrunner_render_plan::SceneSsr::default())
        .unwrap();
    host.apply(crate::RetainedUpdate::Settings(Box::new(effects)))
        .unwrap();
}

fn resolve_host(host: &mut crate::RetainedHost) -> Result<(), ViewerError> {
    resolve_startup(
        &identity(),
        host.initial_scene.as_ref().unwrap(),
        &mut host.options,
        Some((&mut host.capture_pool, &mut host.captures, &host.readiness)),
    )
}

#[test]
fn adapter_frame_rejects_prequeued_ssr_when_resolving_legacy() {
    let mut host = retained_host(FrameProfile::Legacy);
    queue_ssr(&mut host);
    assert_eq!(host.accepted_generation(), 1);
    assert!(matches!(
        resolve_host(&mut host),
        Err(ViewerError::UnsupportedScene(
            "water, SSAO and SSR streaming require HDR"
        ))
    ));
}

#[test]
fn adapter_frame_rejects_incompatible_intermediate_settings_generation() {
    let mut host = retained_host(FrameProfile::Legacy);
    let initial = host.scene().effects().unwrap().clone();
    queue_ssr(&mut host);
    host.apply(crate::RetainedUpdate::Settings(Box::new(initial)))
        .unwrap();
    assert_eq!(host.accepted_generation(), 2);
    assert!(host.scene().effects().unwrap().ssr().is_none());
    assert!(matches!(
        resolve_host(&mut host),
        Err(ViewerError::UnsupportedScene(
            "water, SSAO and SSR streaming require HDR"
        ))
    ));
}

#[test]
fn adapter_frame_compatible_prequeued_settings_resolve_without_gpu() {
    for profile in [FrameProfile::Legacy, settings().frame_profile] {
        let mut host = retained_host(profile);
        let initial = host.scene().effects().unwrap().clone();
        host.apply(crate::RetainedUpdate::Settings(Box::new(initial)))
            .unwrap();
        if profile != FrameProfile::Legacy {
            queue_ssr(&mut host);
        }
        let queued = host.readiness.queue.len();
        resolve_host(&mut host).unwrap();
        assert_eq!(host.options().frame_profile, profile);
        assert_eq!(host.readiness.queue.len(), queued);
    }
}
