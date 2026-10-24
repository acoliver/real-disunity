use compat_kit::{RunControl, fixture, refresh};
use serde_json::json;
use std::{fs, fs::OpenOptions};

#[test]
fn abandoned_refresh_lock_recovers_even_with_reused_live_pid_metadata() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir_all(root.path().join("fixtures")).unwrap();
    let locks = root.path().join(".work/compat/locks");
    fs::create_dir_all(&locks).unwrap();
    fs::write(
        locks.join("local.refresh.lock"),
        format!("{{\"pid\":{},\"generation\":\"old\"}}", std::process::id()),
    )
    .unwrap();
    fs::write(
        root.path().join("fixtures/local.json"),
        serde_json::to_vec(&json!({
            "schema_version":1,"name":"local","source_root":"missing","profiles":["test"],
            "default_profile":"test","evidence_label":"harness-only"
        }))
        .unwrap(),
    )
    .unwrap();
    let config = fixture::load(root.path(), "local").unwrap();
    let report = refresh::refresh(root.path(), &config, false, &RunControl::default()).unwrap();
    assert!(
        report["message"]
            .as_str()
            .unwrap()
            .contains("not configured"),
        "{report}"
    );
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(locks.join("local.refresh.lock"))
        .unwrap();
    file.lock().unwrap();
    let report = refresh::refresh(root.path(), &config, false, &RunControl::default()).unwrap();
    assert!(
        report["message"]
            .as_str()
            .unwrap()
            .contains("already locked"),
        "{report}"
    );
    assert!(locks.join("local.refresh.lock").exists());
}

#[test]
fn kernel_refresh_lease_recovers_after_owner_process_is_killed() {
    use std::io::{BufRead, BufReader};
    use std::process::{Command, Stdio};
    let root = tempfile::tempdir().unwrap();
    fs::create_dir_all(root.path().join("fixtures")).unwrap();
    let locks = root.path().join(".work/compat/locks");
    fs::create_dir_all(&locks).unwrap();
    let path = locks.join("local.refresh.lock");
    fs::write(
        root.path().join("fixtures/local.json"),
        json!({
            "schema_version":1,"name":"local","source_root":"missing","profiles":["test"],
            "default_profile":"test","evidence_label":"harness-only"
        })
        .to_string(),
    )
    .unwrap();
    let mut child = Command::new("python3").args(["-c", "import fcntl,sys,time; f=open(sys.argv[1],'w'); fcntl.flock(f,fcntl.LOCK_EX); print('ready',flush=True); time.sleep(30)"])
        .arg(&path).stdout(Stdio::piped()).spawn().unwrap();
    let mut ready = String::new();
    BufReader::new(child.stdout.take().unwrap())
        .read_line(&mut ready)
        .unwrap();
    assert_eq!(ready.trim(), "ready");
    let config = fixture::load(root.path(), "local").unwrap();
    let live = refresh::refresh(root.path(), &config, false, &RunControl::default()).unwrap();
    child.kill().unwrap();
    child.wait().unwrap();
    assert!(live["message"].as_str().unwrap().contains("already locked"));
    let recovered = refresh::refresh(root.path(), &config, false, &RunControl::default()).unwrap();
    assert!(
        recovered["message"]
            .as_str()
            .unwrap()
            .contains("not configured")
    );
    assert!(path.exists());
}
