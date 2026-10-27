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
            Err(CaptureRequestError::UnsupportedRecipe)
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
