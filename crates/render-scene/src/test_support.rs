//! Shared-device test sessions. Declare the lease before fixtures so it drops last.
use std::{
    any::Any,
    cell::RefCell,
    panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
    sync::{Mutex, MutexGuard, PoisonError},
};
use worldrunner_render_gpu::test_support::{
    TestGpu, assert_shared_gpu_usable, install_fail_fast_handlers, record_shared_gpu_failure,
};

static SESSIONS: Mutex<()> = Mutex::new(());
thread_local! {
    static DEVICES: RefCell<Option<Vec<(wgpu::Device, wgpu::Queue)>>> = const { RefCell::new(None) };
    #[cfg(test)]
    static COMPLETED_UPLOAD_QUEUES: RefCell<Option<Vec<wgpu::Queue>>> = const { RefCell::new(None) };
}

/// Serializes scene queue ownership, including fixture teardown and dedicated devices.
///
/// Acquire once at the beginning of each GPU test, before creating any contexts.
/// Helpers reuse the active session without recursively acquiring this lock.
/// Assertion panics permit reuse after successful teardown. A teardown failure
/// permanently rejects subsequent sessions and shared GPU acquisitions.
#[must_use]
pub struct SceneTestSession {
    _guard: MutexGuard<'static, ()>,
}

impl SceneTestSession {
    pub fn acquire() -> Self {
        assert_shared_gpu_usable();
        DEVICES.with(|devices| assert!(devices.borrow().is_none(), "nested scene test session"));
        // Assertion unwinding poisons the lease even when GPU teardown succeeds.
        let guard = SESSIONS.lock().unwrap_or_else(PoisonError::into_inner);
        assert_shared_gpu_usable();
        DEVICES.with(|devices| *devices.borrow_mut() = Some(Vec::new()));
        Self { _guard: guard }
    }

    pub(crate) fn assert_active() {
        DEVICES.with(|devices| {
            assert!(
                devices.borrow().is_some(),
                "acquire a scene test session first"
            );
        });
    }

    pub(crate) fn retain(gpu: &TestGpu) {
        DEVICES.with(|devices| {
            let mut devices = devices.borrow_mut();
            let devices = devices
                .as_mut()
                .expect("acquire a scene test session first");
            if !devices.iter().any(|(device, _)| device == &gpu.device) {
                devices.push((gpu.device.clone(), gpu.queue.clone()));
            }
        });
    }
}

#[derive(Default)]
struct Teardown {
    failure: Option<Box<dyn Any + Send>>,
}

impl Teardown {
    fn attempt<T>(&mut self, step: &str, action: impl FnOnce() -> T) -> Option<T> {
        match catch_unwind(AssertUnwindSafe(action)) {
            Ok(value) => Some(value),
            Err(panic) => {
                let message = panic
                    .downcast_ref::<String>()
                    .map(String::as_str)
                    .or_else(|| panic.downcast_ref::<&str>().copied())
                    .unwrap_or("non-string panic payload");
                record_shared_gpu_failure(format!("scene test GPU {step} failed: {message}"));
                if self.failure.is_none() {
                    self.failure = Some(panic);
                }
                None
            }
        }
    }
}

fn finish_teardown(action: impl FnOnce(&mut Teardown)) {
    let mut teardown = Teardown::default();
    action(&mut teardown);
    // Complete every cleanup step before propagating a normal-drop failure.
    if !std::thread::panicking()
        && let Some(panic) = teardown.failure
    {
        resume_unwind(panic);
    }
}

fn finish_device<T>(
    teardown: &mut Teardown,
    submit: impl FnOnce() -> T,
    wait: impl FnOnce(Option<T>),
    restore_handlers: impl FnOnce(),
) {
    let submission = teardown.attempt("submit", submit);
    // A submit callback can panic after submission; wait for all work in that case.
    teardown.attempt("wait", || wait(submission));
    teardown.attempt("handler restoration", restore_handlers);
}

impl Drop for SceneTestSession {
    fn drop(&mut self) {
        finish_teardown(|teardown| {
            let devices = teardown.attempt("device collection", || {
                DEVICES.with(|devices| {
                    devices
                        .borrow_mut()
                        .take()
                        .expect("scene test devices missing")
                })
            });
            for (device, queue) in devices.into_iter().flatten() {
                finish_device(
                    teardown,
                    // Polling alone does not submit pending queue uploads.
                    || queue.submit([]),
                    |submission_index| {
                        device
                            .poll(wgpu::PollType::Wait {
                                submission_index,
                                timeout: None,
                            })
                            .expect("scene test GPU drain failed");
                        #[cfg(test)]
                        COMPLETED_UPLOAD_QUEUES.with(|completed| {
                            if let Some(queues) = completed.borrow_mut().as_mut() {
                                queues.push(queue.clone());
                            }
                        });
                    },
                    || install_fail_fast_handlers(&device),
                );
            }
        });
    }
}

#[cfg(test)]
mod teardown_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::TryLockError;

    #[test]
    fn session_excludes_other_tests_without_a_gpu() {
        let session = SceneTestSession::acquire();
        assert!(matches!(SESSIONS.try_lock(), Err(TryLockError::WouldBlock)));
        drop(session);
        let _next = SceneTestSession::acquire();
    }

    #[test]
    fn panicking_session_does_not_prevent_the_next_session() {
        let panic = catch_unwind(|| {
            let _session = SceneTestSession::acquire();
            panic!("scene test assertion failed");
        })
        .unwrap_err();
        assert_eq!(
            panic.downcast_ref::<&str>(),
            Some(&"scene test assertion failed")
        );
        assert!(SESSIONS.is_poisoned());
        let _next = SceneTestSession::acquire();
    }

    #[test]
    fn teardown_submits_uploads_before_wait_and_handler_restore() {
        let session = SceneTestSession::acquire();
        let events = RefCell::new(Vec::new());
        let record = |event| {
            assert!(matches!(SESSIONS.try_lock(), Err(TryLockError::WouldBlock)));
            events.borrow_mut().push(event);
        };
        finish_teardown(|teardown| {
            finish_device(
                teardown,
                || {
                    record("submit");
                    42
                },
                |submission| {
                    assert_eq!(submission, Some(42));
                    record("wait");
                },
                || record("restore handlers"),
            );
        });
        assert_eq!(*events.borrow(), ["submit", "wait", "restore handlers"]);
        drop(session);
        let _next = SceneTestSession::acquire();
    }

    #[test]
    #[should_panic(expected = "acquire a scene test session first")]
    fn missing_session_fails_before_gpu_acquisition() {
        let _ = crate::device::GpuContext::shared_for_test();
    }

    #[test]
    fn lease_outlives_fixture_teardown() {
        struct Fixture;
        impl Drop for Fixture {
            fn drop(&mut self) {
                assert!(matches!(SESSIONS.try_lock(), Err(TryLockError::WouldBlock)));
                DEVICES.with(|devices| assert!(devices.borrow().is_some()));
            }
        }
        let _session = SceneTestSession::acquire();
        let _fixture = Fixture;
    }
}

#[cfg(test)]
mod gpu_tests {
    use super::*;
    use crate::device::GpuContext;

    #[test]
    fn upload_only_session_finishes_fixture_writes_before_next_session() {
        struct UploadOnDrop<'a> {
            queue: &'a wgpu::Queue,
            buffer: &'a wgpu::Buffer,
        }
        impl Drop for UploadOnDrop<'_> {
            fn drop(&mut self) {
                self.queue.write_buffer(self.buffer, 0, &[1, 2, 3, 4]);
            }
        }

        let device;
        let buffer;
        {
            let _session = SceneTestSession::acquire();
            COMPLETED_UPLOAD_QUEUES.with(|completed| *completed.borrow_mut() = Some(Vec::new()));
            let context =
                GpuContext::shared_for_test_with_features(wgpu::Features::empty()).unwrap();
            device = context.device().clone();
            buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("upload-only session readback"),
                size: 4,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            let _fixture = UploadOnDrop {
                queue: context.queue(),
                buffer: &buffer,
            };
        }

        let _next_session = SceneTestSession::acquire();
        let context = GpuContext::shared_for_test_with_features(wgpu::Features::empty()).unwrap();
        assert_eq!(context.device(), &device);
        // map_async also flushes uploads, so readback alone would mask a missing teardown submit.
        let completed = COMPLETED_UPLOAD_QUEUES.with(|completed| completed.borrow_mut().take());
        assert_eq!(completed.unwrap(), [context.queue().clone()]);
        let (sender, receiver) = std::sync::mpsc::channel();
        buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                sender.send(result).unwrap();
            });
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        receiver.recv().unwrap().unwrap();
        assert_eq!(
            &*buffer.slice(..).get_mapped_range().unwrap(),
            &[1, 2, 3, 4]
        );
        buffer.unmap();
    }
}
