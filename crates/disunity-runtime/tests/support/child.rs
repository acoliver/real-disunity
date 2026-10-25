use std::{
    process::{Command, Stdio},
    time::{Duration, Instant},
};

// The parent owns the build lock and immutable artifacts until the child exits.
// This single internal protocol selects a test and optional case, never artifact paths.
const SELECTOR: &str = "DISUNITY_TEST_CHILD";

pub fn mode(test: &str) -> Option<String> {
    let selected = std::env::var(SELECTOR).ok()?;
    let (name, mode) = selected.split_once('|')?;
    (name == test).then(|| mode.to_owned())
}

pub fn active() -> bool {
    std::env::var_os(SELECTOR).is_some()
}

pub fn isolated(test: &str) -> bool {
    run(test, "")
}

pub fn run(test: &str, case: &str) -> bool {
    if mode(test).is_some() {
        return true;
    }
    assert!(!active(), "child protocol selected the wrong test");
    let artifacts = super::artifacts();
    assert!(artifacts.probe().is_file());
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", test, "--include-ignored", "--nocapture"])
        .env(SELECTOR, format!("{test}|{case}"))
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();
    let mut pipe = child.stdout.take().unwrap();
    let reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        std::io::Read::read_to_end(&mut pipe, &mut bytes).unwrap();
        bytes
    });
    let deadline = Instant::now() + Duration::from_mins(2);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            let bytes = reader.join().unwrap();
            let stdout = String::from_utf8_lossy(&bytes);
            println!("{stdout}");
            assert!(
                status.success() && stdout.contains("test result: ok. 1 passed"),
                "{test} ({case}): {status}\n{stdout}"
            );
            return false;
        }
        if Instant::now() >= deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            reader.join().unwrap();
            panic!("{test} ({case}): child exceeded 120 seconds");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}
