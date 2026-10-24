//! Real subprocess protocol peer for harness infrastructure tests only.
use serde_json::json;
use std::{env, fs, io::Write, path::PathBuf, thread, time::Duration};

fn main() {
    let mode = env::args().nth(1).unwrap();
    if gate(&mode) {
        return;
    }
    if mode == "sleep-child" {
        thread::sleep(Duration::from_secs(30));
        return;
    }
    if mode != "orphan" {
        descendant(&mode);
    }
    let response = PathBuf::from(env::var_os("COMPAT_RESPONSE").unwrap());
    let artifacts = PathBuf::from(env::var_os("COMPAT_ARTIFACTS").unwrap());
    unsafe_live(&artifacts, &mode);
    if mode == "hang" {
        thread::sleep(Duration::from_secs(30));
    }
    if mode == "malformed" {
        fs::write(response, "{bad").unwrap();
        return;
    }
    if mode == "log-flood" {
        for _ in 0..4096 {
            println!("harness-only-log-line");
        }
    }
    let request: serde_json::Value =
        serde_json::from_slice(&fs::read(env::var_os("COMPAT_REQUEST").unwrap()).unwrap()).unwrap();
    if mode == "source-drift" {
        fs::write(
            PathBuf::from(request["source_root"].as_str().unwrap()).join("Assets/Original.cs"),
            "changed",
        )
        .unwrap();
    }
    image(&artifacts, &mode);
    oversized(&artifacts, &mode);
    let capture = match mode.as_str() {
        "traversal" => "../escape.png",
        "missing-image" => "captures/missing.png",
        _ => "captures/frame.png",
    };
    let observations = observations(&mode, &request);
    let diagnostics = diagnostic(&mode);
    let report = json!({
        "schema_version": 1,
        "evidence_label": "harness-only",
        "observations": observations,
        "captures": [{"path":capture,"frame":0,"phase":"render","origin":"harness-only"}],
        "diagnostics": diagnostics,
        "teardown": {"completed":true,"native_worlds":u64::from(mode == "leak"),"managed_roots":0,"pending_readbacks":0}
    });

    let mut file = fs::File::create(response).unwrap();
    file.write_all(&serde_json::to_vec(&report).unwrap())
        .unwrap();
    if mode == "orphan" {
        descendant(&mode);
    }
}

fn diagnostic(mode: &str) -> Vec<compat_kit::model::Diagnostic> {
    mode.strip_prefix("diagnostic-")
        .and_then(|status| {
            serde_json::from_value(json!({"status":status,"message":"worker diagnostic"})).ok()
        })
        .into_iter()
        .collect()
}

fn observations(mode: &str, request: &serde_json::Value) -> serde_json::Value {
    if mode == "sequence" {
        json!([
            {"selector":"position","value":{"x":1.0},"frame":0,"phase":"update"},
            {"selector":"event","value":"enter","frame":0,"phase":"contact"},
            {"selector":"event","value":"stay","frame":1,"phase":"contact"},
            {"selector":"position","value":{"x":2.001},"frame":2,"phase":"update"},
            {"selector":"event","value":"exit","frame":2,"phase":"contact"},
            {"selector":"timeline","value":request["timeline"],"frame":2,"phase":"observe"}
        ])
    } else {
        json!([{"selector":"counter", "value":{"count":counter(mode)}, "frame":0, "phase":"update"}])
    }
}

fn image(artifacts: &std::path::Path, mode: &str) {
    fs::create_dir_all(artifacts.join("captures")).unwrap();
    let file = fs::File::create(artifacts.join("captures/frame.png")).unwrap();
    let mut encoder = png::Encoder::new(file, 2, 2);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .unwrap()
        .write_image_data(&[255; 16])
        .unwrap();
    if mode == "bad-image" {
        fs::write(artifacts.join("captures/frame.png"), "PNG but invalid").unwrap();
    }
}

fn descendant(mode: &str) {
    if mode == "descendant" || mode == "orphan" {
        let mut child = std::process::Command::new(env::current_exe().unwrap())
            .arg("sleep-child")
            .spawn()
            .unwrap();
        fs::write(
            PathBuf::from(env::var_os("COMPAT_ROOT").unwrap()).join(".work/descendant.pid"),
            child.id().to_string(),
        )
        .unwrap();
        let waiter = thread::spawn(move || child.wait().unwrap());
        if mode == "descendant" {
            waiter.join().unwrap();
        } else {
            std::process::exit(0);
        }
    }
}

fn oversized(artifacts: &std::path::Path, mode: &str) {
    if mode == "storage" {
        fs::create_dir_all(artifacts.join(".git")).unwrap();
        fs::write(artifacts.join(".git/oversized.bin"), vec![0; 2_097_152]).unwrap();
    }
}

fn gate(mode: &str) -> bool {
    if gate_identity_drift(mode) {
        return true;
    }
    match mode {
        "gate-mutate" => {
            fs::write("Assets/Game.cs", "changed by gate").unwrap();
            true
        }
        "gate-original-commit" => original_commit(),
        "gate-run-candidate" => {
            let root = PathBuf::from(env::var_os("COMPAT_ROOT").unwrap());
            let mut fixture = compat_kit::fixture::load(&root, "local").unwrap();
            fixture.name = "nested-candidate".into();
            fixture.source_root = PathBuf::from(env::var_os("COMPAT_SOURCE_ROOT").unwrap());
            fixture.worker = Some(compat_kit::model::CommandSpec {
                program: env::current_exe().unwrap().to_string_lossy().into_owned(),
                args: vec!["pass".into()],
            });
            fixture.assertions = serde_json::from_value(
                json!([{"op":"equal","selector":"counter","pointer":"/count","value":7}]),
            )
            .unwrap();
            let record = compat_kit::runner::run(
                &root,
                &fixture,
                None,
                false,
                &compat_kit::RunControl::default(),
            )
            .unwrap();
            assert_eq!(record.status, compat_kit::Status::Pass, "{record:?}");
            true
        }
        "gate-pass" => true,
        "gate-fail" => std::process::exit(23),
        _ => false,
    }
}

fn counter(mode: &str) -> u64 {
    if mode == "alternating" {
        let path = PathBuf::from(env::var_os("COMPAT_ROOT").unwrap()).join(".work/attempt-count");
        let count = fs::read_to_string(&path)
            .unwrap_or_default()
            .parse::<u64>()
            .unwrap_or(0);
        fs::write(path, (count + 1).to_string()).unwrap();
        return if count % 2 == 0 { 7 } else { 8 };
    }
    if mode == "mismatch" { 8 } else { 7 }
}

fn gate_identity_drift(mode: &str) -> bool {
    if mode == "gate-upstream-ref-drift" {
        let upstream = PathBuf::from(env::var_os("COMPAT_ROOT").unwrap()).join(".work/upstream");
        assert!(
            std::process::Command::new("git")
                .arg("-C")
                .arg(upstream)
                .args(["update-ref", "refs/heads/main", "HEAD^"])
                .status()
                .unwrap()
                .success()
        );
        return true;
    }
    if mode == "gate-ref-drift" {
        assert!(
            std::process::Command::new("git")
                .args(["update-ref", "refs/heads/gate-added", "HEAD"])
                .status()
                .unwrap()
                .success()
        );
        return true;
    }
    if mode == "gate-fetch-drift" {
        let path = PathBuf::from(".git/FETCH_HEAD");
        let mut bytes = fs::read(&path).unwrap();
        bytes.extend_from_slice(b"\n");
        fs::write(path, bytes).unwrap();
        return true;
    }
    false
}

fn unsafe_live(artifacts: &std::path::Path, mode: &str) {
    if mode == "crash" {
        std::process::exit(17);
    }
    if mode == "unsafe-live" {
        std::os::unix::fs::symlink("/etc/passwd", artifacts.join("unsafe-link")).unwrap();
        thread::sleep(Duration::from_secs(30));
    }
}
fn original_commit() -> bool {
    let source = PathBuf::from(env::var_os("COMPAT_ROOT").unwrap()).join(".work/source");
    fs::write(source.join("Assets/Game.cs"), "class Concurrent {}\n").unwrap();
    for args in [
        vec!["add", "."],
        vec![
            "-c",
            "user.name=Harness",
            "-c",
            "user.email=harness@localhost",
            "commit",
            "-m",
            "concurrent edit",
        ],
    ] {
        assert!(
            std::process::Command::new("git")
                .arg("-C")
                .arg(&source)
                .args(args)
                .status()
                .unwrap()
                .success()
        );
    }
    true
}
