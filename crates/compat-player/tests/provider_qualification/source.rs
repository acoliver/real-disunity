use std::path::Path;
use worldrunner::compat_content::{ImportProfile, ImportSidecar, import_receipt};

pub fn actual_pe_source_proof(root: &Path, qualified: &super::providers::QualifiedProviders) {
    for (game, name, scene, count) in [
        (super::providers::GameSource::Snake, "snake", "GameScene", 8),
        (super::providers::GameSource::Pong, "pong", "Main", 20),
    ] {
        let receipt = import_receipt(
            &root.join(format!(".work/games/{name}")),
            Path::new(&format!("Assets/Scenes/{scene}.unity")),
            &ImportProfile::default(),
            &ImportSidecar::default(),
        )
        .unwrap();
        let proof = qualified
            .game_source(game, &receipt, "q2-source-evidence")
            .unwrap();
        assert_eq!(
            serde_json::to_value(proof).unwrap()["sources"]
                .as_object()
                .unwrap()
                .len(),
            count
        );
    }
    let game = root.join(".work/games/breakout");
    let receipt = import_receipt(
        &game,
        Path::new("Assets/Scenes/SampleScene.unity"),
        &ImportProfile::default(),
        &ImportSidecar::default(),
    )
    .expect("original scene can be inspected while admission remains blocked");
    let proof = qualified
        .game_source(
            super::providers::GameSource::Breakout,
            &receipt,
            "q2-source-evidence",
        )
        .expect("actual typed PE/PDB/source proof must be available");
    let value = serde_json::to_value(proof).unwrap();
    assert_eq!(value["sources"].as_object().unwrap().len(), 7);
    assert_eq!(value["types"]["Platform"], "Assets/Scripts/Platform.cs");
    assert!(value["api_uses"].as_array().unwrap().iter().any(|api| {
        api["assembly"] == "UnityCompat"
            && api["type_name"] == "UnityEngine.InputSystem.KeyControl"
            && api["member"] == "get_isPressed"
            && api["surface"] == "keyboard_snapshot"
    }));
    assert_eq!(value["closure"], receipt.closure_sha256());
    assert!(!value["assembly_sha256"].as_str().unwrap().is_empty());
}
