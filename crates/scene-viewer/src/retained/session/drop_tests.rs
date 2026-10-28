use std::{
    process::{Command, Stdio},
    time::{Duration, Instant},
};

pub(crate) const CHILD: &str = "REAL_DISUNITY_RETAINED_UNWIND_CHILD";

pub(crate) fn assert_panic_exit(test: &str, scenario: &str) {
    let test = test.split_once("::").unwrap().1;
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", test, "--nocapture", "--test-threads=1"])
        .env(CHILD, scenario)
        .env("RUST_BACKTRACE", "0")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    while child.try_wait().unwrap().is_none() {
        if Instant::now() >= deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("unwind subprocess timed out: {scenario}");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let output = child.wait_with_output().unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(101), "{scenario}: {stderr}");
    assert!(stderr.contains("original application panic"), "{stderr}");
    assert!(!stderr.contains("panic in a destructor"), "{stderr}");
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("retained unwind cleanup verified"),
        "cleanup assertions did not complete: {stderr}"
    );
}
