#![cfg(target_os = "linux")]
#[path = "support/handshake.rs"]
pub mod support;

#[path = "native_lifetime_helpers/mod.rs"]
mod helpers;

#[test]
fn native_image_survives_host_and_worker_teardown() {
    for mode in ["drop", "unwind", "early"] {
        support::child::run("native_lifetime_child", mode);
    }
}

#[test]
fn native_lifetime_child() {
    let Some(mode) = support::child::mode("native_lifetime_child") else {
        return;
    };
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
