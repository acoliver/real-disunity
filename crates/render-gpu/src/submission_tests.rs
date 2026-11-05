use super::submission::*;
use std::{
    cell::Cell,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

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

struct DropProbe(Arc<AtomicUsize>);
impl Drop for DropProbe {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

fn retained_by_a_recording_frame(
    detach: bool,
) -> (
    SubmissionAuthority,
    RecordingFrame,
    wgpu::Device,
    Arc<AtomicUsize>,
) {
    let (device, queue) = context();
    let authority = SubmissionAuthority::new(queue);
    let dropped = Arc::new(AtomicUsize::new(0));
    let owner = FrameOwner::new(DropProbe(dropped.clone()), &authority);
    let recording = authority.begin(&device).unwrap();
    Publications::retain_lease(&recording, &owner.lease()).unwrap();
    if detach {
        owner.detach();
    } else {
        drop(owner);
    }
    (authority, recording, device, dropped)
}

#[test]
fn a_detached_owner_stays_held_by_its_frame_until_the_gpu_has_completed_it() {
    let (authority, recording, device, dropped) = retained_by_a_recording_frame(true);
    assert_eq!(
        dropped.load(Ordering::SeqCst),
        0,
        "the frame holds the owner"
    );
    recording.finish().submit().unwrap();
    assert_eq!(authority.submitted_serial(), 1);
    device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: None,
        })
        .unwrap();
    assert_eq!(authority.completed_serial(), 1);
    assert_eq!(
        dropped.load(Ordering::SeqCst),
        1,
        "completion released the owner"
    );
}

#[test]
fn a_detached_owner_is_released_at_once_when_its_frame_is_cancelled() {
    let (authority, recording, _device, dropped) = retained_by_a_recording_frame(true);
    assert_eq!(dropped.load(Ordering::SeqCst), 0);
    recording.finish().cancel();
    assert_eq!(dropped.load(Ordering::SeqCst), 1);
    assert_eq!(authority.submitted_serial(), 0, "nothing was submitted");
}

#[test]
fn a_detached_owner_with_no_frame_is_released_at_once_and_takes_no_new_frame() {
    let (device, queue) = context();
    let authority = SubmissionAuthority::new(queue);
    let dropped = Arc::new(AtomicUsize::new(0));
    let owner = FrameOwner::new(DropProbe(dropped.clone()), &authority);
    let lease = owner.lease();
    owner.detach();
    assert_eq!(
        dropped.load(Ordering::SeqCst),
        0,
        "the lease clone still holds it"
    );
    let recording = authority.begin(&device).unwrap();
    assert!(matches!(
        Publications::retain_lease(&recording, &lease),
        Err(SubmitError::Lease(LeaseError::Expired))
    ));
    drop(lease);
    assert_eq!(dropped.load(Ordering::SeqCst), 1);
}

#[test]
fn a_dropped_owner_still_expires_the_frame_that_retained_it() {
    let (authority, recording, _device, dropped) = retained_by_a_recording_frame(false);
    assert!(matches!(
        recording.finish().submit(),
        Err(SubmitError::Lease(LeaseError::Expired))
    ));
    assert_eq!(authority.submitted_serial(), 0);
    assert_eq!(dropped.load(Ordering::SeqCst), 1);
}
