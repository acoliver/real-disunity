use std::path::PathBuf;
use worldrunner_compat_content::yaml::parse_unity_yaml;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.work/games")
}

#[test]
fn decodes_original_multidocument_scenes_and_prefab_headers() {
    for (game, scene, minimum) in [
        ("snake", "GameScene", 15),
        ("breakout", "SampleScene", 35),
        ("pong", "Main", 35),
    ] {
        let text = std::fs::read_to_string(
            root()
                .join(game)
                .join(format!("Assets/Scenes/{scene}.unity")),
        )
        .unwrap();
        let docs = parse_unity_yaml(&text, 10_000).unwrap();
        assert!(
            docs.len() >= minimum,
            "{game}: no original documents decoded"
        );
        assert!(
            docs.iter()
                .any(|d| d.kind == "Camera" && d.fields["m_GameObject"]["fileID"].is_i64())
        );
        assert!(docs.iter().any(|d| d.kind == "MonoBehaviour"));
    }
}

#[test]
fn preserves_reference_ids_floats_escaped_text_and_stripped_flag() {
    // Minimal text shaped by original Pong MonoBehaviour and Snake stripped Transform records.
    let text = "%YAML 1.1\n%TAG !u! tag:unity3d.com,2011:\n--- !u!114 &-123\nMonoBehaviour:\n  m_Text: \"score\\n\\\"two\\\"\"\n  speed: 1.25e-2\n  target: {fileID: 114504255927865218, guid: 4597034a74042724da78e04cd9cd6c19, type: 3}\n--- !u!4 &888 stripped\nTransform:\n  m_Father: {fileID: 0}\n";
    let docs = parse_unity_yaml(text, 10).unwrap();
    assert_eq!(docs.len(), 2);
    assert_eq!(docs[0].file_id, -123);
    assert_eq!(docs[0].fields["m_Text"], "score\n\"two\"");
    assert_eq!(docs[0].fields["speed"], 0.0125);
    assert_eq!(
        docs[0].fields["target"]["fileID"],
        114_504_255_927_865_218_i64
    );
    assert!(docs[1].stripped);
}

#[test]
fn rejects_document_limits_duplicate_ids_and_nonfinite_values() {
    let text = "--- !u!4 &1\nTransform:\n  x: 1\n--- !u!4 &2\nTransform:\n  x: 2\n";
    assert!(parse_unity_yaml(text, 1).is_err());
    assert!(parse_unity_yaml(&text.replace("&2", "&1"), 10).is_err());
    assert!(parse_unity_yaml("--- !u!4 &1\nTransform:\n  x: .nan\n", 10).is_err());
}

#[test]
fn builtin_guids_are_identity_strings_even_when_yaml_sees_exponents() {
    let text = std::fs::read_to_string(root().join("pong/Assets/Scenes/Main.unity")).unwrap();
    let docs = parse_unity_yaml(&text, 10_000).unwrap();
    let settings = docs.iter().find(|d| d.kind == "RenderSettings").unwrap();
    assert_eq!(
        settings.fields["m_SpotCookie"]["guid"],
        "0000000000000000e000000000000000"
    );
}

#[test]
fn guid_normalization_preserves_unicode_quoted_text_and_literal_string_bodies() {
    let text = "--- !u!114 &1\nMonoBehaviour:\n  title: \"Joueur ☃ guid: 0000000000000000e000000000000000\"\n  literal: |\n    guid: 0000000000000000e000000000000000\n  target: {fileID: 10102, guid: 0000000000000000e000000000000000, type: 0}\n";
    let docs = parse_unity_yaml(text, 10).unwrap();
    assert_eq!(
        docs[0].fields["title"],
        "Joueur ☃ guid: 0000000000000000e000000000000000"
    );
    assert_eq!(
        docs[0].fields["literal"],
        "guid: 0000000000000000e000000000000000\n"
    );
    assert_eq!(
        docs[0].fields["target"]["guid"],
        "0000000000000000e000000000000000"
    );
}
