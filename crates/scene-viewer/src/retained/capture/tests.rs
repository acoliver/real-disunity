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
