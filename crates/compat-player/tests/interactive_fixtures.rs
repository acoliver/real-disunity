use compat_kit::{RunRequest, fixture};
use serde_json::Value;
use std::path::Path;

#[test]
fn portable_interactive_fixtures_compile_every_current_original_script() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    for (name, count) in [
        ("snake-interactive", 8),
        ("pong-interactive", 20),
        ("breakout-interactive", 7),
    ] {
        let selected = fixture::load(&root, name).unwrap();
        let source_root = fixture::source_root(&root, &selected).unwrap();
        let artifacts = tempfile::tempdir_in(root.join(".work/tmp")).unwrap();
        assert_eq!(selected.metadata["script_count"], count);
        assert!(selected.metadata.get("assembly").is_none());
        assert!(selected.metadata.get("assembly_qualification").is_none());
        let request = RunRequest {
            schema_version: 1,
            run_id: format!("compile-{name}"),
            fixture: name.into(),
            profile: selected.default_profile,
            desktop: true,
            seed: selected.seed,
            source_hashes: fixture::inspect(&source_root).unwrap().scripts,
            source_root,
            artifact_root: artifacts.path().into(),
            metadata: selected.metadata,
            timeline: selected.timeline,
            assertions: selected.assertions,
        };
        let assembly =
            compat_player::assembly_qualification::prepare_game(&root, &request).unwrap();
        assert!(assembly.is_file());
        let proof: Value = serde_json::from_slice(
            &std::fs::read(artifacts.path().join("build/original-source-count.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(proof["source_count"], count);
        assert_eq!(proof["compiler_inputs_verified"], true);
        assert_eq!(
            proof["compiler_inputs"].as_array().unwrap().len(),
            usize::try_from(count).unwrap()
        );
        assert_eq!(proof["metadata"]["assembly"], "OriginalGame");
    }
}
