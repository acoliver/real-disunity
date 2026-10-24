use compat_kit::{RunRequest, files};
use compat_player::assembly_qualification::prepare_game;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

struct Fixture {
    root: PathBuf,
    directory: tempfile::TempDir,
    request: RunRequest,
}
impl Fixture {
    fn new(count: usize) -> Self {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .unwrap();
        let directory = tempfile::tempdir_in(root.join(".work/tmp")).unwrap();
        let source = directory.path().join("source");
        fs::create_dir_all(source.join("Assets/Scripts")).unwrap();
        let request = RunRequest {
            schema_version: 1,
            run_id: "qualification".into(),
            fixture: "qualification".into(),
            profile: "compat-6000.3".into(),
            desktop: false,
            seed: 17,
            timeline: vec![],
            assertions: vec![],
            source_root: source,
            source_hashes: BTreeMap::new(),
            artifact_root: directory.path().join("artifacts"),
            metadata: json!({}),
        };
        let mut fixture = Self {
            root,
            directory,
            request,
        };
        for index in 0..count {
            fixture.add_source(index);
        }
        fixture
    }
    fn add_source(&mut self, index: usize) {
        self.write_source(
            index,
            &format!("public class Game{index} : UnityEngine.MonoBehaviour {{}}"),
        );
    }
    fn write_source(&mut self, index: usize, text: &str) {
        let relative = format!("Assets/Scripts/Game{index}.cs");
        let path = self.request.source_root.join(&relative);
        fs::write(&path, text).unwrap();
        self.request
            .source_hashes
            .insert(relative, files::hash(&path).unwrap());
    }
    fn proof(&self) -> PathBuf {
        self.request
            .artifact_root
            .join("build/original-source-count.json")
    }
    fn cached(&mut self, assembly: &Path) {
        self.request.metadata =
            json!({"assembly": assembly, "assembly_qualification": self.proof()});
    }
    fn reject(&self, expected: &str) {
        let error = prepare_game(&self.root, &self.request)
            .expect_err("unqualified assembly must be rejected");
        assert!(
            error.contains(expected),
            "expected {expected:?}, got {error:?}"
        );
    }
}

#[test]
fn existing_empty_dll_without_proof_is_rejected() {
    let mut fixture = Fixture::new(1);
    let dll = fixture.directory.path().join("unknown.dll");
    fs::write(&dll, []).unwrap();
    fixture.request.metadata = json!({"assembly": dll});
    fixture.reject("assembly_qualification");
}

#[test]
fn artifact_and_cached_paths_cannot_escape_worktree() {
    let mut fixture = Fixture::new(1);
    fixture.request.metadata = json!({"assembly": fixture.root.join("Cargo.toml")});
    fixture.reject(".work");
    fixture.request.metadata = json!({"assembly": ".work/not-absolute.dll"});
    fixture.reject("absolute");
    fixture.request.metadata = json!({});
    fixture.request.artifact_root = fixture.directory.path().join("../escape");
    fixture.reject("relative path");
}

#[test]
fn fresh_and_cached_assemblies_bind_current_sources_and_actual_pe() {
    let mut fixture = Fixture::new(7);
    let assembly = prepare_game(&fixture.root, &fixture.request).unwrap();
    let proof: Value = files::read_json(&fixture.proof()).unwrap();
    check_fresh_proof(&fixture, &assembly, &proof);
    fixture.cached(&assembly);
    check_cached_proof_and_tampering(&fixture, &assembly, &proof);

    fixture.write_source(0, "public class ChangedGame : UnityEngine.MonoBehaviour {}");
    fixture.reject("source hashes");
    fixture.request.metadata = json!({});
    let changed = prepare_game(&fixture.root, &fixture.request).unwrap();
    fixture.cached(&changed);
    assert!(prepare_game(&fixture.root, &fixture.request).is_ok());

    fixture.add_source(7);
    fixture.reject("source count");
    fixture.request.metadata = json!({});
    let updated = prepare_game(&fixture.root, &fixture.request).unwrap();
    fixture.cached(&updated);
    assert!(prepare_game(&fixture.root, &fixture.request).is_ok());
    fixture.request.metadata["script_count"] = json!(7);
    fixture.reject("expected 7");
    fixture.request.metadata["script_count"] = json!(8);
    assert!(prepare_game(&fixture.root, &fixture.request).is_ok());

    fixture
        .request
        .source_hashes
        .remove("Assets/Scripts/Game7.cs");
    fixture.reject("selection");
}

fn check_fresh_proof(fixture: &Fixture, assembly: &Path, proof: &Value) {
    assert_eq!(proof["count"], 7);
    assert_eq!(proof["source_count"], 7);
    assert_eq!(proof["source_hashes"], json!(fixture.request.source_hashes));
    assert_eq!(proof["assembly_sha256"], files::hash(assembly).unwrap());
    assert_eq!(proof["compiler_inputs_verified"], true);
    assert_eq!(proof["compiler_inputs"].as_array().unwrap().len(), 7);
    assert!(
        proof["metadata"]["types"]
            .as_array()
            .unwrap()
            .contains(&json!("Game6"))
    );
    assert!(
        proof["metadata"]["references"]
            .as_array()
            .unwrap()
            .contains(&json!("UnityCompat"))
    );
}

fn check_cached_proof_and_tampering(fixture: &Fixture, assembly: &Path, proof: &Value) {
    assert_eq!(
        prepare_game(&fixture.root, &fixture.request).unwrap(),
        assembly
    );
    // Legacy gate proofs have the same complete identity fields without player extensions.
    let mut legacy = proof.clone();
    for key in [
        "count",
        "files",
        "compiler_inputs",
        "compiler_inputs_verified",
    ] {
        legacy.as_object_mut().unwrap().remove(key);
    }
    files::write_json(&fixture.proof(), &legacy).unwrap();
    assert_eq!(
        prepare_game(&fixture.root, &fixture.request).unwrap(),
        assembly
    );

    let mut incorrect = proof.clone();
    incorrect["compiler_inputs"] = json!([]);
    files::write_json(&fixture.proof(), &incorrect).unwrap();
    fixture.reject("compiler inputs");
    incorrect = proof.clone();
    incorrect["metadata"]["types"] = json!(["Forged"]);
    files::write_json(&fixture.proof(), &incorrect).unwrap();
    fixture.reject("metadata");
    files::write_json(&fixture.proof(), &proof).unwrap();

    let original_bytes = fs::read(assembly).unwrap();
    fs::write(assembly, []).unwrap();
    fixture.reject("assembly SHA256");
    incorrect = proof.clone();
    incorrect["assembly_sha256"] = json!(files::hash(assembly).unwrap());
    files::write_json(&fixture.proof(), &incorrect).unwrap();
    fixture.reject("managed metadata");
    fs::write(assembly, original_bytes).unwrap();
    files::write_json(&fixture.proof(), proof).unwrap();
}

#[test]
fn fresh_compile_rejects_unrequested_msbuild_inputs_and_missing_facade_reference() {
    let fixture = Fixture::new(1);
    fs::create_dir_all(&fixture.request.artifact_root).unwrap();
    fs::write(
        fixture.request.artifact_root.join("Extra.cs"),
        "public class Extra {};",
    )
    .unwrap();
    fs::write(fixture.request.artifact_root.join("Directory.Build.targets"),
        r#"<Project><ItemGroup><Compile Include="$(MSBuildThisFileDirectory)Extra.cs" /></ItemGroup></Project>"#).unwrap();
    fixture.reject("actual MSBuild Compile inputs");
    assert!(
        !fixture.proof().exists(),
        "failed admission must never publish reusable proof"
    );
    fs::remove_file(
        fixture
            .request
            .artifact_root
            .join("Directory.Build.targets"),
    )
    .unwrap();
    let mut fixture = fixture;
    fixture.write_source(0, "public class NoFacade {}");
    fixture.reject("managed metadata");
    assert!(!fixture.proof().exists());
}

#[test]
fn unchanged_original_game_sources_produce_complete_reusable_proofs() {
    for game in ["snake", "breakout", "pong"] {
        let mut fixture = Fixture::new(0);
        let original = compat_kit::fixture::load(&fixture.root, game).unwrap();
        fixture.request.source_root =
            compat_kit::fixture::source_root(&fixture.root, &original).unwrap();
        fixture.request.source_hashes = compat_kit::fixture::inspect(&fixture.request.source_root)
            .unwrap()
            .scripts;
        if let Some(count) = original.metadata.get("script_count") {
            fixture.request.metadata["script_count"] = count.clone();
        }
        let assembly = prepare_game(&fixture.root, &fixture.request).unwrap();
        let proof: Value = files::read_json(&fixture.proof()).unwrap();
        assert!(proof["source_count"].as_u64().unwrap() > 0);
        assert_eq!(proof["compiler_inputs_verified"], true);
        fixture.cached(&assembly);
        assert_eq!(
            prepare_game(&fixture.root, &fixture.request).unwrap(),
            assembly
        );
    }
}

#[test]
fn current_source_bytes_and_source_selection_are_independently_checked() {
    let mut fixture = Fixture::new(1);
    fs::write(
        fixture.request.source_root.join("Assets/Scripts/Game0.cs"),
        "public class Changed : UnityEngine.MonoBehaviour {}",
    )
    .unwrap();
    fixture.reject("current source hashes");
    fixture
        .request
        .source_hashes
        .insert("Assets/Scripts/../../Outside.cs".into(), "ignored".into());
    fixture.reject("unsafe relative path");
}

#[cfg(unix)]
#[test]
fn symlinked_assembly_and_output_are_rejected() {
    use std::os::unix::fs::symlink;
    let mut fixture = Fixture::new(1);
    let link = fixture.directory.path().join("candidate.dll");
    symlink(fixture.root.join("Cargo.toml"), &link).unwrap();
    fixture.request.metadata = json!({"assembly": link});
    fixture.reject("symlink");
    let link = fixture.directory.path().join("output");
    symlink(fixture.directory.path(), &link).unwrap();
    fixture.request.metadata = json!({});
    fixture.request.artifact_root = link.join("artifacts");
    fixture.reject("symlink");
}
