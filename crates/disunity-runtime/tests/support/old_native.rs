use std::process::Command;

const CHILD: &str = "DISUNITY_OLD_NATIVE_CHILD";

/// Runs `body` for `test` in a child process that selects the explicitly configured older
/// native image. A process holds one CLR and one native host, so capability-rejection tests
/// cannot share the current image's process.
pub fn run_in_old_native_process(test: &str, body: impl FnOnce()) {
    if std::env::var_os(CHILD).is_some() {
        body();
        return;
    }
    let old = std::env::var_os("DISUNITY_OLD_NATIVE_LIBRARY")
        .expect("explicit older native image lacking the tested capability");
    let output = Command::new(std::env::current_exe().expect("test executable"))
        .args([test, "--exact", "--nocapture", "--test-threads=1"])
        .env("DISUNITY_TEST_NATIVE_LIBRARY", old)
        .env(CHILD, "1")
        .output()
        .expect("old-native child test process");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success() && stdout.contains("test result: ok. 1 passed"),
        "old-native child failed for {test}\n{stdout}\n{stderr}"
    );
}
