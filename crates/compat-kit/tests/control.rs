use compat_kit::{RunControl, fixture, runner};
use serde_json::{Value, json};
use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    net::TcpStream,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

fn root(mode: &str) -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir_all(root.path().join("fixtures")).unwrap();
    fs::create_dir_all(root.path().join(".work/source")).unwrap();
    let fixture = json!({"schema_version":1,"name":"harness-only","source_root":".work/source","profiles":["test","unrun"],"default_profile":"test","evidence_label":"harness-only",
        "worker":{"program":env!("CARGO_BIN_EXE_compat-kit-protocol-test-worker"),"args":[mode]},"limits":{"deadline_ms":3000,"log_bytes":1024,"artifact_bytes":1_048_576,"capture_count":4},
        "assertions":[{"op":"equal","selector":"counter","value":{"count":7}}]});
    fs::write(
        root.path().join("fixtures/harness-only.json"),
        serde_json::to_vec(&fixture).unwrap(),
    )
    .unwrap();
    root
}
#[test]
fn cancelling_a_real_worker_terminates_its_process_group() {
    let root = root("descendant");
    let control = RunControl::default();
    let other = control.clone();
    let path = root.path().to_path_buf();
    let handle = thread::spawn(move || {
        let fixture = fixture::load(&path, "harness-only").unwrap();
        runner::run(&path, &fixture, None, false, &other).unwrap()
    });
    let pid_path = root.path().join(".work/descendant.pid");
    let start = Instant::now();
    while !pid_path.exists() {
        assert!(start.elapsed() < Duration::from_secs(5));
        thread::sleep(Duration::from_millis(10));
    }
    let pid = fs::read_to_string(&pid_path).unwrap();
    control.cancel();
    let record = handle.join().unwrap();
    assert_eq!(record.status, compat_kit::Status::Skipped);
    let process_state = fs::read_to_string(format!("/proc/{pid}/stat")).unwrap_or_default();
    assert!(
        process_state.is_empty() || process_state.split_whitespace().nth(2) == Some("Z"),
        "descendant still running: {process_state}"
    );
}
fn http(address: &str, method: &str, path: &str, body: &Value) -> (u16, String) {
    http_headers(address, method, path, body, &[])
}
fn http_headers(
    address: &str,
    method: &str,
    path: &str,
    body: &Value,
    headers: &[(&str, &str)],
) -> (u16, String) {
    let mut stream = TcpStream::connect(address).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let body = body.to_string();
    write!(stream,"{method} {path} HTTP/1.1\r\nHost: {address}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n",body.len()).unwrap();
    for (name, value) in headers {
        write!(stream, "{name}: {value}\r\n").unwrap();
    }
    write!(stream, "Connection: close\r\n\r\n{body}").unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    let (header, body) = response.split_once("\r\n\r\n").unwrap();
    (
        header.split_whitespace().nth(1).unwrap().parse().unwrap(),
        body.into(),
    )
}
#[test]
fn localhost_browser_endpoints_run_stop_refresh_and_reject_traversal() {
    let root = root("hang");
    let (mut server, address) = launch_server(root.path());
    let address = address.as_str();
    let (status, page) = http(address, "GET", "/", &Value::Null);
    assert_eq!(status, 200);
    assert!(page.contains("no record"));
    assert!(page.contains("/api/run"));
    let token = page
        .split("name=\"compat-token\" content=\"")
        .nth(1)
        .and_then(|s| s.split('\"').next())
        .expect("page token")
        .to_owned();
    let origin = format!("http://{address}");
    let auth = [
        ("Origin", origin.as_str()),
        ("X-Compat-Token", token.as_str()),
    ];
    let (status, body) = http_headers(
        address,
        "POST",
        "/api/run",
        &json!({"fixture":"harness-only"}),
        &auth,
    );
    assert_eq!(status, 202, "{body}");
    let job: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(
        http_headers(
            address,
            "POST",
            "/api/stop",
            &json!({"id":job["id"]}),
            &auth
        )
        .0,
        202
    );
    wait_for_completed_task(address);
    assert_eq!(
        http_headers(
            address,
            "POST",
            "/api/refresh",
            &json!({"fixture":"harness-only","accept":false}),
            &auth,
        )
        .0,
        202
    );
    assert_eq!(
        http(address, "GET", "/files/../../Cargo.toml", &Value::Null).0,
        400
    );
    server.kill().unwrap();
    server.wait().unwrap();
}

fn wait_for_completed_task(address: &str) {
    let start = Instant::now();
    loop {
        let (_, state) = http(address, "GET", "/api/state", &Value::Null);
        let state: Value = serde_json::from_str(&state).unwrap();
        if state["tasks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|t| t["done"] == true)
        {
            break;
        }
        assert!(start.elapsed() < Duration::from_secs(5));
        thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn deadline_and_successful_leader_exit_both_terminate_inherited_children() {
    for mode in ["descendant", "orphan"] {
        let root = root(mode);
        let fixture = fixture::load(root.path(), "harness-only").unwrap();
        let record =
            runner::run(root.path(), &fixture, None, false, &RunControl::default()).unwrap();
        assert_eq!(
            record.status,
            if mode == "orphan" {
                compat_kit::Status::Pass
            } else {
                compat_kit::Status::Error
            }
        );
        let pid = fs::read_to_string(root.path().join(".work/descendant.pid")).unwrap();
        let state = fs::read_to_string(format!("/proc/{pid}/stat")).unwrap_or_default();
        assert!(
            state.is_empty() || state.split_whitespace().nth(2) == Some("Z"),
            "child still running: {state}"
        );
    }
}

fn launch_server(root: &std::path::Path) -> (std::process::Child, String) {
    let mut server = Command::new(env!("CARGO_BIN_EXE_compat-kit"))
        .env("COMPAT_ROOT", root)
        .args(["serve", "--port", "0"])
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();
    let mut line = String::new();
    BufReader::new(server.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    let info: Value = serde_json::from_str(&line).unwrap();
    (server, info["address"].as_str().unwrap().to_owned())
}

#[test]
fn live_scan_unsafe_artifacts_retain_structured_stage_and_path() {
    let root = root("unsafe-live");
    let fixture = fixture::load(root.path(), "harness-only").unwrap();
    let record = runner::run(root.path(), &fixture, None, false, &RunControl::default()).unwrap();
    assert_eq!(record.status, compat_kit::Status::Error);
    assert_eq!(record.reporting.exit_code, Some(3));
    let cause = &record.reporting.failure_causes[0];
    assert_eq!(
        cause.kind,
        compat_kit::reporting::FailureKind::Infrastructure
    );
    assert_eq!(cause.stage, "supervision-artifacts");
    assert!(cause.message.contains("unsafe-link"));
    assert!(cause.message.contains("symlink rejected"));
    let process: Value =
        compat_kit::files::read_json(&root.path().join(&record.run_dir).join("process.json"))
            .unwrap();
    assert_eq!(process["failure_cause"]["stage"], "supervision-artifacts");
    assert!(
        !root
            .path()
            .join(&record.run_dir)
            .join("unsafe-link")
            .exists()
    );
}
