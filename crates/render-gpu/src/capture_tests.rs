// Capture snapshots deliberately use owned immutable values, not host references.
#[test]
fn unsubmitted_prefix_cancellation_has_no_publication_or_cleanup_work() {
    let (device, authority) = context();
    let value = Arc::new(AtomicUsize::new(0));
    let prefix = authority
        .begin_capture(&device, writes(), frozen(), |_, edits, _| {
            let value = value.clone();
            edits
                .stage_write(WriteKey::Resident(1), move || {
                    value.store(1, Ordering::SeqCst);
                })
                .unwrap();
            Ok::<_, ()>(())
        })
        .unwrap();
    let observer = prefix.observer();
    drop(prefix);
    assert_eq!(value.load(Ordering::SeqCst), 0);
    assert_eq!(authority.submitted_serial(), 0);
    assert_eq!(observer.outcome(), Some(CaptureOutcome::Cancelled));
    authority.drain_captures(&device).unwrap();
    assert!(authority.is_idle());
}

#[test]
fn capture_waits_for_earlier_work_and_poll_retires_abandoned_prefix() {
    let (device, authority) = context();
    let frame = authority.prepare(&device, |_, _| Ok::<_, ()>(())).unwrap();
    let submission = frame.submit().unwrap();
    assert!(matches!(
        authority.begin_capture(&device, writes(), frozen(), |_, _, _| Ok::<_, ()>(())),
        Err(CaptureError::Submission(
            crate::submission::SubmitError::Busy
        ))
    ));
    // Dispatch completion explicitly; admission never depends on GPU speed.
    device
        .poll(wgpu::PollType::Wait {
            submission_index: Some(submission),
            timeout: None,
        })
        .unwrap();
    let prefix = authority
        .begin_capture(&device, writes(), frozen(), |_, _, _| Ok::<_, ()>(()))
        .unwrap();
    let observer = prefix.observer();
    let continuation = prefix.submit().unwrap();
    assert!(matches!(
        authority.drain_captures(&device),
        Err(CaptureError::Submission(
            crate::submission::SubmitError::Busy
        ))
    ));
    drop(continuation);
    for _ in 0..1000 {
        authority.poll_captures(&device).unwrap();
        if authority.is_idle() {
            break;
        }
        std::thread::yield_now();
    }
    authority.drain_captures(&device).unwrap();
    assert!(authority.is_idle());
    assert_eq!(observer.outcome(), Some(CaptureOutcome::Dropped));
}

#[test]
fn final_readback_failure_retains_submitted_suffix_accounting() {
    let (device, authority) = context();
    let value = Arc::new(AtomicUsize::new(0));
    let prefix = authority
        .begin_capture(&device, writes(), frozen(), |_, _, _| {
            Ok::<_, &'static str>(())
        })
        .unwrap();
    let observer = prefix.observer();
    let suffix = prefix
        .submit()
        .unwrap()
        .resume(
            &device,
            |_, _| Ok(()),
            |_, edits, _, ()| {
                let value = value.clone();
                edits
                    .stage_write(WriteKey::Resident(2), move || {
                        value.store(2, Ordering::SeqCst);
                    })
                    .unwrap();
                Ok::<_, &'static str>(())
            },
        )
        .unwrap();
    let result = suffix
        .submit()
        .unwrap()
        .finish(&device, |_, _| Err::<(), _>("final map"));
    assert!(matches!(result, Err(CaptureError::Readback("final map"))));
    assert_eq!(value.load(Ordering::SeqCst), 2);
    assert_eq!(observer.outcome(), Some(CaptureOutcome::ReadbackFailed));
    assert!(authority.is_idle());
}

#[test]
fn device_loss_before_suffix_enqueue_discards_its_publications() {
    let (device, authority) = context();
    let value = Arc::new(AtomicUsize::new(0));
    let prefix = authority
        .begin_capture(&device, writes(), frozen(), |_, _, _| Ok::<_, ()>(()))
        .unwrap();
    let observer = prefix.observer();
    let suffix = prefix
        .submit()
        .unwrap()
        .resume(
            &device,
            |_, _| Ok(()),
            |_, edits, _, ()| {
                let value = value.clone();
                edits
                    .stage_write(WriteKey::Resident(2), move || {
                        value.store(2, Ordering::SeqCst);
                    })
                    .unwrap();
                Ok::<_, ()>(())
            },
        )
        .unwrap();
    authority.report_device_loss("suffix loss".into());
    assert!(matches!(
        suffix.submit(),
        Err(CaptureError::Submission(
            crate::submission::SubmitError::DeviceLost(_)
        ))
    ));
    assert_eq!(value.load(Ordering::SeqCst), 0);
    assert_eq!(observer.outcome(), Some(CaptureOutcome::DeviceLost));
    assert!(authority.drain_captures(&device).is_err());
}

use crate::submission::capture::{CaptureError, CaptureOutcome, CaptureWrites, WriteKey};
use crate::submission::{PrepareError, SubmissionAuthority};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

fn context() -> (wgpu::Device, SubmissionAuthority) {
    let instance = crate::device::headless_instance();
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .unwrap();
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
    (device, SubmissionAuthority::new(queue))
}
fn writes() -> CaptureWrites {
    CaptureWrites::new([WriteKey::Resident(1)], [WriteKey::Resident(2)], []).unwrap()
}
#[derive(Debug, PartialEq, Eq)]
struct Frozen {
    frame: u64,
    configuration: u64,
    camera: [u32; 4],
    animation: u64,
    inputs: Vec<u32>,
}
fn frozen() -> Frozen {
    Frozen {
        frame: 42,
        configuration: 7,
        camera: [1, 2, 3, 4],
        animation: 55,
        inputs: vec![8, 9],
    }
}

#[test]
fn frozen_identity_busy_and_segment_publication() {
    let (device, authority) = context();
    let prefix_value = Arc::new(AtomicUsize::new(0));
    let suffix_value = Arc::new(AtomicUsize::new(0));
    let prefix = authority
        .begin_capture(&device, writes(), frozen(), |_, edits, frame| {
            assert_eq!(frame, &frozen());
            let value = prefix_value.clone();
            edits
                .stage_write(WriteKey::Resident(1), move || {
                    value.store(1, Ordering::SeqCst);
                })
                .unwrap();
            Ok::<_, &'static str>(())
        })
        .unwrap();
    let observer = prefix.observer();
    assert_eq!(prefix_value.load(Ordering::SeqCst), 0);
    let continuation = prefix.submit().unwrap();
    assert_eq!(prefix_value.load(Ordering::SeqCst), 1);
    assert!(matches!(
        authority.prepare(&device, |_, _| Ok::<_, ()>(())),
        Err(PrepareError::Busy)
    ));
    assert!(authority.try_replacement().is_err());
    assert!(authority.submit_commands([]).is_err());
    let suffix = continuation
        .resume(
            &device,
            |_, _| Ok(123),
            |_, edits, frame, result| {
                assert_eq!(frame, &frozen());
                assert_eq!(*result, 123);
                let value = suffix_value.clone();
                edits
                    .stage_write(WriteKey::Resident(2), move || {
                        value.store(2, Ordering::SeqCst);
                    })
                    .unwrap();
                Ok::<_, &'static str>(())
            },
        )
        .unwrap();
    assert_eq!(suffix_value.load(Ordering::SeqCst), 0);
    let submitted = suffix.submit().unwrap();
    assert_eq!(suffix_value.load(Ordering::SeqCst), 2);
    assert!(matches!(
        authority.prepare(&device, |_, _| Ok::<_, ()>(())),
        Err(PrepareError::Busy)
    ));
    assert_eq!(
        submitted
            .finish(&device, |_, _| Ok::<_, &'static str>(456))
            .unwrap(),
        456
    );
    assert_eq!(observer.outcome(), Some(CaptureOutcome::Complete));
    assert!(authority.is_idle());
}

#[test]
fn write_sets_reject_overlap_undeclared_and_unkeyed_edits() {
    let key = WriteKey::Resident(1);
    assert!(CaptureWrites::new([key], [key], []).is_err());
    assert!(CaptureWrites::new([key], [key], [key]).is_ok());
    let (device, authority) = context();
    let result = authority.begin_capture(&device, writes(), frozen(), |_, edits, _| {
        let _ = edits.stage_write(WriteKey::Resident(2), || panic!("undeclared edit"));
        Ok::<_, ()>(())
    });
    assert!(matches!(result, Err(CaptureError::Write(_))));
    let result = authority.begin_capture(&device, writes(), frozen(), |_, edits, _| {
        edits.stage(|| panic!("unkeyed edit"));
        Ok::<_, ()>(())
    });
    assert!(matches!(result, Err(CaptureError::Write(_))));
    assert!(authority.is_idle());
}

#[test]
fn readback_and_suffix_failures_discard_suffix_and_drain_prefix() {
    for readback_fails in [true, false] {
        let (device, authority) = context();
        let value = Arc::new(AtomicUsize::new(0));
        let prefix = authority
            .begin_capture(&device, writes(), frozen(), |_, _, _| {
                Ok::<_, &'static str>(())
            })
            .unwrap();
        let observer = prefix.observer();
        let result = prefix.submit().unwrap().resume(
            &device,
            |_, _| {
                if readback_fails { Err("map") } else { Ok(()) }
            },
            |_, edits, _, ()| {
                let value = value.clone();
                edits
                    .stage_write(WriteKey::Resident(2), move || {
                        value.store(1, Ordering::SeqCst);
                    })
                    .unwrap();
                Err("suffix")
            },
        );
        if readback_fails {
            assert!(matches!(result, Err(CaptureError::Readback("map"))));
        } else {
            assert!(matches!(result, Err(CaptureError::Preparation("suffix"))));
        }
        authority.drain_captures(&device).unwrap();
        assert_eq!(value.load(Ordering::SeqCst), 0);
        assert_eq!(
            observer.outcome(),
            Some(if readback_fails {
                CaptureOutcome::ReadbackFailed
            } else {
                CaptureOutcome::PreparationFailed
            })
        );
        assert!(authority.prepare(&device, |_, _| Ok::<_, ()>(())).is_ok());
    }
}

#[test]
fn dropped_continuation_and_suffix_keep_inputs_until_drain() {
    for segment in [0, 1, 2] {
        let (device, authority) = context();
        let inputs = Arc::new(123);
        let weak = Arc::downgrade(&inputs);
        let prefix = authority
            .begin_capture(&device, writes(), inputs.clone(), |_, _, _| Ok::<_, ()>(()))
            .unwrap();
        drop(inputs);
        let observer = prefix.observer();
        let continuation = prefix.submit().unwrap();
        if segment > 0 {
            let suffix = continuation
                .resume(&device, |_, _| Ok::<_, ()>(()), |_, _, _, ()| Ok(()))
                .unwrap();
            if segment == 2 {
                drop(suffix.submit().unwrap());
            } else {
                drop(suffix);
            }
        } else {
            drop(continuation);
        }
        assert!(weak.upgrade().is_some());
        assert_eq!(observer.outcome(), Some(CaptureOutcome::Dropped));
        authority.drain_captures(&device).unwrap();
        assert!(weak.upgrade().is_none());
        assert!(authority.is_idle());
    }
}

#[test]
fn suffix_cannot_publish_prefix_keys_and_device_loss_stops_generation() {
    let (device, authority) = context();
    let prefix = authority
        .begin_capture(&device, writes(), frozen(), |_, _, _| Ok::<_, ()>(()))
        .unwrap();
    let result = prefix.submit().unwrap().resume(
        &device,
        |_, _| Ok(()),
        |_, edits, _, ()| {
            let _ = edits.stage_write(WriteKey::Resident(1), || panic!("prefix overwritten"));
            Ok::<_, ()>(())
        },
    );
    assert!(matches!(result, Err(CaptureError::Write(_))));
    authority.drain_captures(&device).unwrap();
    let prefix = authority
        .begin_capture(&device, writes(), frozen(), |_, _, _| Ok::<_, ()>(()))
        .unwrap();
    let observer = prefix.observer();
    let continuation = prefix.submit().unwrap();
    authority.report_device_loss("injected".into());
    assert!(matches!(
        continuation.resume(&device, |_, _| Ok(()), |_, _, _, ()| Ok::<_, ()>(())),
        Err(CaptureError::Submission(
            crate::submission::SubmitError::DeviceLost(_)
        ))
    ));
    assert_eq!(observer.outcome(), Some(CaptureOutcome::DeviceLost));
    assert!(authority.drain_captures(&device).is_err());
    assert!(matches!(
        authority.prepare(&device, |_, _| Ok::<_, ()>(())),
        Err(PrepareError::DeviceLost(_))
    ));
}
