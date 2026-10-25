use super::submission::*;
use std::{cell::Cell, rc::Rc};

fn context() -> (wgpu::Device, wgpu::Queue) {
    let instance = crate::device::headless_instance();
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .unwrap();
    pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap()
}

#[test]
fn frame_identity_excludes_cancelled_and_unrelated_preparations() {
    let (device, queue) = context();
    let authority = SubmissionAuthority::new(queue);
    let mut first = None;
    let prepared = authority
        .prepare(&device, |_, edits| {
            assert!(edits.same_frame(edits));
            first = Some(edits.clone());
            Ok::<_, ()>(())
        })
        .unwrap();
    prepared.cancel();
    let first = first.unwrap();
    assert!(!first.same_frame(&first));
    let next = authority
        .prepare(&device, |_, edits| {
            assert!(!first.same_frame(edits));
            Ok::<_, ()>(())
        })
        .unwrap();
    drop(next);
}

#[test]
fn frame_identity_spans_only_the_owned_live_capture() {
    let (device, queue) = context();
    let authority = SubmissionAuthority::new(queue);
    let mut first = None;
    let prefix = authority
        .begin_capture(
            &device,
            capture::CaptureWrites::new([], [], []).unwrap(),
            (),
            |_, edits, ()| {
                first = Some(edits.clone());
                Ok::<_, ()>(())
            },
        )
        .unwrap();
    let continuation = prefix.submit().unwrap();
    let first = first.unwrap();
    let suffix = continuation
        .resume(
            &device,
            |_, _| Ok::<_, ()>(()),
            |_, edits, (), ()| {
                assert!(!first.same_transaction(edits));
                assert!(first.same_frame(edits));
                Ok::<_, ()>(())
            },
        )
        .unwrap();
    suffix
        .submit()
        .unwrap()
        .finish(&device, |_, _| Ok::<_, ()>(()))
        .unwrap();
    authority.drain_captures(&device).unwrap();
    assert!(!first.same_frame(&first));
}

#[test]
fn failure_cancel_busy_and_next_frame_publication() {
    let (device, queue) = context();
    let authority = SubmissionAuthority::new(queue.clone());
    let resident = Rc::new(Cell::new(0));
    let failed = authority.prepare(&device, |_, edits| {
        let resident = resident.clone();
        edits.stage(move || resident.set(1));
        Err::<(), _>("failed")
    });
    assert!(matches!(failed, Err(PrepareError::Preparation("failed"))));
    assert_eq!(resident.get(), 0);
    let prepared = authority
        .prepare(&device, |_, edits| {
            let resident = resident.clone();
            edits.stage(move || resident.set(2));
            Ok::<_, ()>(())
        })
        .unwrap();
    assert_eq!(resident.get(), 0);
    assert!(matches!(
        authority.prepare(&device, |_, _| Ok::<_, ()>(())),
        Err(PrepareError::Busy)
    ));
    assert!(matches!(
        authority.clone().prepare(&device, |_, _| Ok::<_, ()>(())),
        Err(PrepareError::Busy)
    ));
    prepared.cancel();
    assert_eq!(resident.get(), 0);
    let prepared = authority
        .prepare(&device, |_, edits| {
            let resident = resident.clone();
            edits.stage(move || resident.set(resident.get() + 3));
            Ok::<_, ()>(())
        })
        .unwrap();
    let submission = prepared.submit().unwrap();
    assert_eq!(resident.get(), 3);
    let next = authority
        .prepare(&device, |_, _| {
            assert_eq!(resident.get(), 3);
            Ok::<_, ()>(())
        })
        .unwrap();
    drop(next);
    device
        .poll(wgpu::PollType::Wait {
            submission_index: Some(submission),
            timeout: None,
        })
        .unwrap();
    assert_eq!(resident.get(), 3);
}

#[test]
fn dropped_transaction_discards_publications() {
    let (device, queue) = context();
    let authority = SubmissionAuthority::new(queue.clone());
    let resident = Rc::new(Cell::new(false));
    let prepared = authority
        .prepare(&device, |_, edits| {
            let resident = resident.clone();
            edits.stage(move || resident.set(true));
            Ok::<_, ()>(())
        })
        .unwrap();
    drop(prepared);
    assert!(!resident.get());
    assert!(authority.prepare(&device, |_, _| Ok::<_, ()>(())).is_ok());
}

#[test]
fn production_submission_sites_are_confined_to_authority() {
    fn visit(path: &std::path::Path) {
        for entry in std::fs::read_dir(path).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                if path.file_name().unwrap() != "tests" {
                    visit(&path);
                }
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                let name = path.file_name().unwrap().to_str().unwrap();
                if name.contains("test")
                    || name.contains("regression")
                    || name == "submission.rs"
                    || name == "concurrency.rs"
                    || matches!(
                        name,
                        "numerical.rs" | "regressions.rs" | "msaa.rs" | "contracts.rs"
                    )
                {
                    continue;
                }
                let text = std::fs::read_to_string(&path).unwrap();
                let production = text.split("#[cfg(test)]\nmod tests").next().unwrap();
                assert!(
                    !production.contains("queue.submit(")
                        && !production.contains("queue().submit("),
                    "direct submission in {}",
                    path.display()
                );
            }
        }
    }
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    visit(&root);
}

#[test]
fn device_loss_stops_admission_without_retry_or_publication() {
    let (device, queue) = context();
    let authority = SubmissionAuthority::new(queue);
    let published = Rc::new(Cell::new(false));
    let frame = authority
        .prepare(&device, |_, edits| {
            let published = published.clone();
            edits.stage(move || published.set(true));
            Ok::<_, ()>(())
        })
        .unwrap();
    authority.report_device_loss("injected".into());
    assert!(matches!(frame.submit(), Err(SubmitError::DeviceLost(_))));
    assert!(!published.get());
    assert!(matches!(
        authority.prepare(&device, |_, _| Ok::<_, ()>(())),
        Err(PrepareError::DeviceLost(_))
    ));
}

#[test]
fn all_convenience_paths_enter_preparation() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    for file in [
        "render-scene/src/offscreen.rs",
        "render-scene/src/shadows.rs",
        "render-scene/src/draw/retained.rs",
        "render-post/src/readback.rs",
    ] {
        let source = std::fs::read_to_string(root.join(file)).unwrap();
        assert!(
            source.contains(".prepare("),
            "convenience path bypasses preparation: {file}"
        );
        assert!(
            source.contains(".submit()?"),
            "convenience path bypasses submission: {file}"
        );
    }
}

#[test]
fn preparation_and_publication_unwinds_release_exclusion() {
    use std::panic::{AssertUnwindSafe, catch_unwind};
    let (device, queue) = context();
    let authority = SubmissionAuthority::new(queue);
    assert!(
        catch_unwind(AssertUnwindSafe(|| {
            let _ = authority.prepare(&device, |_, edits| -> Result<(), ()> {
                edits.stage(|| panic!("must be discarded"));
                panic!("recording failure");
            });
        }))
        .is_err()
    );
    assert!(Publications::current().is_none());
    let frame = authority
        .prepare(&device, |_, edits| {
            edits.stage(|| panic!("caller publication failure"));
            Ok::<_, ()>(())
        })
        .unwrap();
    assert!(catch_unwind(AssertUnwindSafe(|| frame.submit())).is_err());
    authority
        .prepare(&device, |_, _| Ok::<_, ()>(()))
        .unwrap()
        .cancel();
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
}

#[test]
fn configuration_reservation_excludes_every_clone_until_release() {
    let (device, queue) = context();
    let authority = SubmissionAuthority::new(queue);
    let guard = authority.try_configuration().unwrap();
    assert!(!authority.is_idle());
    let other = authority.clone();
    let worker_device = device.clone();
    std::thread::spawn(move || {
        assert!(matches!(
            other.prepare(&worker_device, |_, _| Ok::<_, ()>(())),
            Err(PrepareError::Busy)
        ));
        assert!(matches!(other.submit_commands([]), Err(SubmitError::Busy)));
        assert!(matches!(other.try_configuration(), Err(SubmitError::Busy)));
    })
    .join()
    .unwrap();
    drop(guard);
    authority
        .prepare(&device, |_, _| Ok::<_, ()>(()))
        .unwrap()
        .submit()
        .unwrap();
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    assert!(authority.try_configuration().is_ok());
}

#[test]
fn prepared_flights_register_completion_on_their_own_queue() {
    let (device, queue) = context();
    let authority = SubmissionAuthority::new(queue);
    let (_unpolled_device, _other_queue) = context();
    let flights = crate::flight::FrameAdmission::new(2).unwrap();
    let first = flights.try_acquire().unwrap();
    let frame = authority.prepare(&device, |_, _| Ok::<_, ()>(())).unwrap();
    let a = flights.submit_prepared(first, frame).unwrap();
    let second = flights.try_acquire().unwrap();
    let frame = authority.prepare(&device, |_, _| Ok::<_, ()>(())).unwrap();
    let b = flights.submit_prepared(second, frame).unwrap();
    a.wait(&device).unwrap();
    b.wait(&device).unwrap();
    device.poll(wgpu::PollType::Poll).unwrap();
    assert!(a.is_complete());
    assert!(b.is_complete());
    assert!(flights.is_idle());
}

#[test]
fn replaced_encoder_cannot_publish_an_abandoned_upload() {
    let (device, queue) = context();
    let authority = SubmissionAuthority::new(queue);
    let published = Rc::new(Cell::new(false));
    let frame = authority
        .prepare(&device, |encoder, edits| {
            let published = published.clone();
            edits.stage(move || published.set(true));
            *encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
            Ok::<_, ()>(())
        })
        .unwrap();
    assert!(matches!(
        frame.submit(),
        Err(SubmitError::Transaction(TransactionError::ForeignEncoder))
    ));
    assert!(!published.get());
    assert!(authority.is_idle());
}
