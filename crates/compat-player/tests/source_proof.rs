#[path = "../src/source_proof.rs"]
mod source_proof;

use source_proof::{Inspection, SelectedGame};
use std::path::{Path, PathBuf};

static CONTROL_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .find(|p| p.join("crates/compat-player").is_dir())
        .expect("workspace ancestor")
        .to_path_buf()
}

#[test]
fn document_names_cannot_escape_selected_source_root() {
    for path in [
        "/etc/passwd",
        "../escape.cs",
        "a/../../escape.cs",
        "C:\\escape.cs",
        "a\\..\\escape.cs",
        "",
        "a//b.cs",
        "a/./b.cs",
    ] {
        assert!(
            source_proof::validate_document_path(path).is_err(),
            "{path}"
        );
    }
    assert!(source_proof::validate_document_path("Assets/Scripts/Player.cs").is_ok());
}

#[test]
fn actual_keyboard_signatures_stop_at_unqualified_metadata_boundary() {
    let _guard = CONTROL_LOCK.lock().expect("control lock");
    let inspection =
        Inspection::run(&workspace(), SelectedGame::KeyboardControl).expect("actual inspector");
    let report = inspection.report();
    assert_eq!(report["disposition"], "UNQUALIFIED");
    assert!(report["failures"].as_array().expect("failures").is_empty());
    assert_eq!(
        report["edges"]
            .as_array()
            .expect("edges")
            .iter()
            .filter(|e| e["status"] == "metadata-match")
            .count(),
        3
    );
    assert!(
        report["requirements"]
            .as_array()
            .expect("requirements")
            .iter()
            .any(|r| r["code"] == "host-qualified-signature-inventory")
    );
    assert_eq!(inspection.output_sha256().len(), 64);
    inspection.verify_current().expect("stable actual inputs");
    let mut serialized = Vec::new();
    inspection
        .write_report(&mut serialized)
        .expect("diagnostics serialize");
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&serialized).expect("JSON"),
        *report
    );
}

#[test]
fn immutable_originals_preserve_complete_source_and_method_census() {
    for (game, sources, methods, edges) in [
        (SelectedGame::Snake, 8, 36, 241),
        (SelectedGame::Pong, 20, 81, 207),
        (SelectedGame::Breakout, 7, 44, 289),
    ] {
        let inspection = Inspection::run(&workspace(), game).expect("actual frozen image");
        let r = inspection.report();
        assert!(
            r["failures"].as_array().expect("failures").is_empty(),
            "{}",
            r["failures"]
        );
        assert_eq!(
            r["documents"]
                .as_array()
                .expect("documents")
                .iter()
                .filter(|d| d["category"] == "source")
                .count(),
            sources
        );
        assert_eq!(r["methods"].as_array().expect("methods").len(), methods);
        assert_eq!(r["closures"].as_array().expect("closures").len(), methods);
        assert_eq!(r["edges"].as_array().expect("edges").len(), edges);
        assert_eq!(r["disposition"], "UNQUALIFIED");
        assert!(
            r["requirements"]
                .as_array()
                .expect("requirements")
                .iter()
                .any(|r| r["code"] == "external-call")
        );
    }
}

#[test]
fn an_inspection_detects_a_changed_owned_source_after_creation() {
    struct Restore(PathBuf, Vec<u8>);
    impl Drop for Restore {
        fn drop(&mut self) {
            std::fs::write(&self.0, &self.1).expect("restore owned probe source");
        }
    }
    let _guard = CONTROL_LOCK.lock().expect("control lock");
    let root = workspace();
    let inspection =
        Inspection::run(&root, SelectedGame::KeyboardControl).expect("control inspection");
    let path = root.join(".work/resumed-source-signature-proof/probes/Keyboard/Code.cs");
    let bytes = std::fs::read(&path).expect("owned probe source");
    let _restore = Restore(path.clone(), bytes.clone());
    let mut changed = bytes;
    changed.extend_from_slice(b"\n// temporary source integrity control\n");
    std::fs::write(path, changed).expect("mutate owned control only");
    assert!(inspection.verify_current().is_err());
}
