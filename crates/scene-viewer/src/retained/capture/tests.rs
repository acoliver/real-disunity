#[test]
fn native_rcas_admission_requires_hdr_without_consuming_capacity() {
    let mut host = host();
    let recipe = RetainedCaptureRecipe::new(
        1,
        1,
        CaptureJitter::None,
        false,
        vec![CaptureOutput::Srgb8],
        u64::MAX,
    )
    .unwrap()
    .with_native_rcas(crate::NativeRcasSettings {
        stops: 0.75,
        domain: crate::RcasDomain::EncodedOutput,
    })
    .unwrap();
    assert_eq!(
        host.request_capture_recipe(0, recipe.clone()),
        Err(CaptureRequestError::HdrRequired)
    );
    assert!(host.captures.outstanding.is_empty());
    host.options.frame_profile = crate::FrameProfile::Hdr {
        samples: 1,
        tone_map: crate::ToneMap::Reinhard,
    };
    assert_eq!(host.request_capture_recipe(0, recipe), Ok(0));
}

#[test]
fn recipe_admission_is_bounded_and_cancelled_reservations_release_on_delivery() {
    let mut host = host();
    let hdr = || {
        RetainedCaptureRecipe::new(
            1,
            1,
            CaptureJitter::None,
            false,
            vec![CaptureOutput::LinearHdr],
            u64::MAX,
        )
        .unwrap()
    };
    assert_eq!(
        host.request_capture_recipe(0, hdr()),
        Err(CaptureRequestError::HdrRequired)
    );
    assert!(host.captures.outstanding.is_empty());
    host.options.frame_profile = crate::FrameProfile::Hdr {
        samples: 1,
        tone_map: crate::ToneMap::Reinhard,
    };
    host.size = [2048, 2048];
    let id = host.request_capture_recipe(0, hdr()).unwrap();
    assert!(matches!(
        host.request_capture_recipe(0, hdr()),
        Err(CaptureRequestError::RecipeBudget { .. })
    ));
    assert_eq!(host.captures.outstanding.len(), 1);
    host.captures.cancel();
    assert!(!host.captures.reservations.is_empty());
    assert!(
        matches!(host.captures.take_event(), Some(RetainedEvent::Capture { id: actual, outcome: RetainedCaptureOutcome::Cancelled, .. }) if actual == id)
    );
    assert!(host.captures.reservations.is_empty());
    host.request_capture_recipe(0, hdr()).unwrap();
}

#[test]
fn unavailable_execution_modes_are_rejected_without_consuming_request_ids() {
    let mut host = host();
    for output in [
        CaptureOutput::Depth,
        CaptureOutput::Normals,
        CaptureOutput::AmbientOcclusion,
        CaptureOutput::SsrConfidence,
    ] {
        let recipe =
            RetainedCaptureRecipe::new(1, 1, CaptureJitter::None, false, vec![output], u64::MAX)
                .unwrap();
        assert_eq!(
            host.request_capture_recipe(0, recipe),
            Err(CaptureRequestError::FeatureUnavailable { output })
        );
    }
    assert_eq!(host.request_capture(0).unwrap(), 0);
}

use super::*;
use crate::{RetainedHost, RetainedUpdate};

fn host() -> RetainedHost {
    RetainedHost::new(crate::tests::scene(false), crate::tests::options())
        .unwrap()
        .with_capture_limit(2)
        .unwrap()
}

#[test]
fn requests_are_bounded_until_notification_delivery() {
    let mut host = host();
    let first = host.request_capture(0).unwrap();
    let second = host.request_capture(0).unwrap();
    assert_ne!(first, second);
    assert_eq!(
        host.request_capture(0),
        Err(CaptureRequestError::BackPressure { limit: 2 })
    );
    host.captures.cancel();
    assert_eq!(
        host.request_capture(0),
        Err(CaptureRequestError::BackPressure { limit: 2 })
    );
    assert!(
        matches!(host.captures.take_event(), Some(RetainedEvent::Capture {
        id, generation: 0, outcome: RetainedCaptureOutcome::Cancelled,
    }) if id == first)
    );
    host.request_capture(0).unwrap();
}

#[test]
fn unknown_superseded_and_exiting_requests_are_typed() {
    let mut host = host();
    assert_eq!(
        host.request_capture(1),
        Err(CaptureRequestError::UnknownGeneration { generation: 1 })
    );
    host.apply(RetainedUpdate::Camera {
        convention: worldrunner_camera::ViewConvention::RIGHT_HANDED,
        pose: crate::tests::options().pose,
        lens: crate::tests::options().camera,
    })
    .unwrap();
    host.readiness.publish_front();
    assert_eq!(
        host.request_capture(0),
        Err(CaptureRequestError::Superseded {
            generation: 0,
            ready: 1
        })
    );
    host.request_exit();
    assert_eq!(host.request_capture(1), Err(CaptureRequestError::Exiting));
}

#[test]
fn zero_capture_limit_is_rejected() {
    assert!(host().with_capture_limit(0).is_err());
}

#[test]
fn recipe_pages_and_accumulation_storage_fail_admission_without_consuming_ids() {
    let mut host = host();
    let recipe = RetainedCaptureRecipe::new(
        1,
        1,
        CaptureJitter::None,
        false,
        vec![CaptureOutput::Srgb8],
        u64::MAX,
    )
    .unwrap();
    assert_eq!(
        host.request_capture_recipe(0, recipe.clone().with_page_dependency()),
        Err(CaptureRequestError::PagingDisabled { generation: 0 })
    );
    let plain = recipe.storage_bytes([64, 64]).unwrap();
    let mut sampled = recipe;
    sampled.supersample = 2;
    let spatial = sampled.storage_bytes([64, 64]).unwrap();
    assert!(spatial > plain * 4);
    sampled.temporal_samples = 32;
    assert_eq!(sampled.storage_bytes([64, 64]).unwrap(), spatial);
    sampled.max_bytes = spatial - 1;
    assert!(matches!(
        host.request_capture_recipe(0, sampled),
        Err(CaptureRequestError::RecipeBudget { .. })
    ));
    assert_eq!(host.request_capture(0).unwrap(), 0);
}

#[test]
fn feature_availability_uses_requested_generation_and_counts_native_storage() {
    let mut host = host();
    host.options.frame_profile = crate::FrameProfile::Hdr {
        samples: 4,
        tone_map: crate::ToneMap::Reinhard,
    };
    let effects = host
        .scene
        .effects()
        .unwrap()
        .clone()
        .with_ssr(worldrunner_render_plan::SceneSsr {
            enabled: true,
            ..Default::default()
        })
        .unwrap();
    host.apply(RetainedUpdate::Settings(Box::new(effects)))
        .unwrap();
    let make = |output| {
        RetainedCaptureRecipe::new(1, 1, CaptureJitter::None, false, vec![output], u64::MAX)
            .unwrap()
    };
    assert_eq!(
        host.request_capture_recipe(0, make(CaptureOutput::Depth)),
        Err(CaptureRequestError::FeatureUnavailable {
            output: CaptureOutput::Depth
        })
    );
    let id = host
        .request_capture_recipe(1, make(CaptureOutput::SsrConfidence))
        .unwrap();
    let reserved = host.captures.reservations[&id];
    let mut packed = make(CaptureOutput::SsrConfidence);
    packed.frame_samples = 4;
    assert_eq!(reserved, packed.storage_bytes(host.size).unwrap());
    assert!(
        reserved
            > make(CaptureOutput::SsrConfidence)
                .storage_bytes(host.size)
                .unwrap()
    );
    host.captures.cancel();
    host.captures.take_event().unwrap();
    assert!(host.captures.reservations.is_empty());
    host.size = [2048, 2048];
    host.request_capture_recipe(1, make(CaptureOutput::Normals))
        .unwrap();
    assert!(matches!(
        host.request_capture_recipe(1, make(CaptureOutput::Normals)),
        Err(CaptureRequestError::RecipeBudget { .. })
    ));
}

#[test]
fn ao_capture_budget_uses_requested_generations_native_format() {
    use worldrunner_render_plan::{SceneEffects, SceneEffectsParts, SceneSsaoReceiver};
    let mut host = host();
    host.size = [33, 2];
    host.options.frame_profile = crate::FrameProfile::Hdr {
        samples: 1,
        tone_map: crate::ToneMap::Reinhard,
    };
    let initial = host.scene.effects().unwrap();
    let effects = SceneEffects::new(SceneEffectsParts {
        water: None,
        sky: *initial.sky(),
        sun: *initial.sun(),
        ambient: *initial.ambient(),
        shadows: initial.shadows().clone(),
        camera: *initial.camera(),
        grading: *initial.grading(),
        post: vec![],
        anti_alias: *initial.anti_alias(),
        ssao: Some(worldrunner_render_plan::SceneSsao {
            receiver: SceneSsaoReceiver::DepthAwareGather,
            depth_footprint: worldrunner_render_plan::SceneSsaoDepthFootprint::default(),
            kernel: worldrunner_render_plan::SceneSsaoKernel::Hemisphere,
            filter: worldrunner_render_plan::SceneSsaoFilter::DepthAwareBox,
            sample_count: 4,
            bias_millimeters: 1,
            radius_millimeters: 100,
            intensity_micro: 1_000_000,
            horizon_angle_millidegrees: 90_000,
        }),
    })
    .unwrap();
    host.apply(RetainedUpdate::Settings(Box::new(effects)))
        .unwrap();
    let make = |max| {
        RetainedCaptureRecipe::new(
            1,
            1,
            CaptureJitter::None,
            false,
            vec![CaptureOutput::AmbientOcclusion],
            max,
        )
        .unwrap()
    };
    assert_eq!(make(u64::MAX).storage_bytes(host.size), Ok(1552));
    assert_eq!(
        host.request_capture_recipe(1, make(2839)),
        Err(CaptureRequestError::RecipeBudget {
            required: 2840,
            limit: 2839
        })
    );
    let id = host.request_capture_recipe(1, make(2840)).unwrap();
    assert_eq!(id, 0);
    assert_eq!(host.captures.reservations[&id], 2840);
}
