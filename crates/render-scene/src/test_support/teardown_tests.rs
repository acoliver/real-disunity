use super::*;
use std::process::Command;
use worldrunner_render_gpu::device::DeviceRequirements;

fn isolated(test: &str, scenario: &str) {
    let output = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            &format!("test_support::teardown_tests::{test}"),
            "--nocapture",
        ])
        .env("SCENE_TEARDOWN_SCENARIO", scenario)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{scenario}:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn panic_text(panic: &(dyn std::any::Any + Send)) -> &str {
    if let Some(message) = panic.downcast_ref::<String>() {
        message
    } else {
        panic.downcast_ref::<&str>().copied().unwrap()
    }
}

fn assert_reuse_rejected(reason: &str) {
    let acquire = catch_unwind(SceneTestSession::acquire)
        .err()
        .expect("session reused failed GPU state");
    let message = panic_text(acquire.as_ref());
    assert!(message.contains("shared GPU state unusable"), "{message}");
    assert!(message.contains(reason), "{message}");
    let gpu = catch_unwind(|| TestGpu::shared(&DeviceRequirements::default()))
        .expect_err("shared acquisition reached the GPU after failed teardown");
    let message = panic_text(gpu.as_ref());
    assert!(message.contains("shared GPU state unusable"), "{message}");
    assert!(message.contains(reason), "{message}");
}

struct FaultyDevices<'a> {
    fail_at: &'a str,
    events: &'a RefCell<Vec<String>>,
}

impl Drop for FaultyDevices<'_> {
    fn drop(&mut self) {
        finish_teardown(|teardown| {
            for device in ["first", "second"] {
                let record = |step| {
                    self.events.borrow_mut().push(format!("{device} {step}"));
                    assert!(
                        self.fail_at != "all" && (device != "first" || step != self.fail_at),
                        "injected {step} failure"
                    );
                };
                finish_device(
                    teardown,
                    || record("submit"),
                    |submission| {
                        assert_eq!(
                            submission,
                            (self.fail_at != "all"
                                && (device != "first" || self.fail_at != "submit"))
                                .then_some(())
                        );
                        record("wait");
                    },
                    || record("restore"),
                );
            }
        });
    }
}

fn expected_events() -> [&'static str; 6] {
    [
        "first submit",
        "first wait",
        "first restore",
        "second submit",
        "second wait",
        "second restore",
    ]
}

#[test]
fn failed_teardown_during_unwind_prevents_shared_reuse() {
    let Ok(fail_at) = std::env::var("SCENE_TEARDOWN_SCENARIO") else {
        for scenario in ["submit", "wait", "restore", "all"] {
            isolated(
                "failed_teardown_during_unwind_prevents_shared_reuse",
                scenario,
            );
        }
        return;
    };
    let events = RefCell::new(Vec::new());
    let panic = catch_unwind(AssertUnwindSafe(|| {
        let _session = SceneTestSession::acquire();
        let _fixture = FaultyDevices {
            fail_at: &fail_at,
            events: &events,
        };
        panic!("original assertion failure");
    }))
    .unwrap_err();
    assert_eq!(panic_text(panic.as_ref()), "original assertion failure");
    assert_eq!(*events.borrow(), expected_events());
    let first_failure = if fail_at == "all" { "submit" } else { &fail_at };
    let reason = format!("injected {first_failure} failure");
    assert_reuse_rejected(&reason);
    std::thread::spawn(move || assert_reuse_rejected(&reason))
        .join()
        .unwrap();
}

#[test]
fn normal_teardown_restores_all_devices_before_propagating_failure() {
    let Ok(fail_at) = std::env::var("SCENE_TEARDOWN_SCENARIO") else {
        for scenario in ["submit", "wait", "restore", "all"] {
            isolated(
                "normal_teardown_restores_all_devices_before_propagating_failure",
                scenario,
            );
        }
        return;
    };
    let events = RefCell::new(Vec::new());
    let panic = catch_unwind(AssertUnwindSafe(|| {
        let _session = SceneTestSession::acquire();
        let _fixture = FaultyDevices {
            fail_at: &fail_at,
            events: &events,
        };
    }))
    .unwrap_err();
    let first_failure = if fail_at == "all" { "submit" } else { &fail_at };
    let reason = format!("injected {first_failure} failure");
    assert_eq!(panic_text(panic.as_ref()), reason);
    assert_eq!(*events.borrow(), expected_events());
    assert_reuse_rejected(&reason);
}

#[test]
fn missing_devices_during_unwind_rejects_reuse_and_preserves_assertion() {
    if std::env::var("SCENE_TEARDOWN_SCENARIO").is_err() {
        isolated(
            "missing_devices_during_unwind_rejects_reuse_and_preserves_assertion",
            "missing",
        );
        return;
    }
    let panic = catch_unwind(|| {
        let _session = SceneTestSession::acquire();
        DEVICES.with(|devices| devices.borrow_mut().take());
        panic!("original assertion failure");
    })
    .unwrap_err();
    assert_eq!(panic_text(panic.as_ref()), "original assertion failure");
    assert_reuse_rejected("scene test devices missing");
}
