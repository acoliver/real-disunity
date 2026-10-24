#![cfg(target_os = "linux")]

#[path = "native_lifetime_helpers/mod.rs"]
mod helpers;

use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[test]
#[ignore = "requires selected genuine native image and isolated CLR process"]
fn native_image_survives_host_and_worker_teardown() {
    assert!(std::env::var_os("COMPAT_DISPLAY_CONTAINMENT").is_some());
    for mode in [
        "drop",
        "unwind",
        "early",
        "sealed-drop",
        "sealed-unwind",
        "sealed-early",
    ] {
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "native_lifetime_child",
                "--ignored",
                "--nocapture",
            ])
            .env("DISUNITY_LIFETIME_CASE", mode)
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(45);
        loop {
            if let Some(status) = child.try_wait().unwrap() {
                assert!(status.success(), "{mode}: {status}");
                break;
            }
            if Instant::now() >= deadline {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!("{mode}: exceeded 45 seconds");
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

#[test]
#[ignore = "subprocess entry selected by the bounded parent"]
fn native_lifetime_child() {
    let mode = std::env::var("DISUNITY_LIFETIME_CASE").unwrap();
    let worker_mode = mode.clone();
    let result = std::thread::spawn(move || helpers::exercise(&worker_mode)).join();
    if mode.ends_with("unwind") {
        let panic = result.expect_err("worker must unwind with a live world");
        assert_eq!(
            panic.downcast_ref::<&str>(),
            Some(&"intentional unwind with a live native world")
        );
    } else {
        result.unwrap();
    }
    helpers::verify_drained(&mode);
    eprintln!("LIFETIME {mode}: worker exited; live worlds=0; scripts=0; bindings=0");
}
