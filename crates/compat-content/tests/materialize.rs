#[test]
fn required_asset_subids_are_validated_even_when_the_asset_guid_exists() {
    let mut graph = snake();
    let asset = graph
        .assets
        .iter()
        .find(|a| a.path.ends_with("GameConfig.asset"))
        .unwrap();
    let reference = format!("{}:999999", asset.guid);
    let script = graph
        .components
        .iter_mut()
        .find(|c| c.native_kind.as_deref() == Some("Script") && c.object.is_some())
        .unwrap();
    script.native_fields["bad_asset"] = json!({"$ref":reference});
    assert!(materialization_plan(&graph, &policy()).is_err());
}

#[test]
fn semantic_error_envelopes_and_malformed_native_handles_stop_execution() {
    for reply in [
        json!({"ok":false,"error":"allocation failed"}),
        json!({"handle":1.5}),
        json!({"handle":0}),
    ] {
        let mut calls = 0;
        let result = materialize(&snake(), &policy(), |_| {
            calls += 1;
            Ok::<_, String>(reply.clone())
        });
        assert!(result.is_err());
        assert_eq!(calls, 1);
    }
}

use serde_json::{Value, json};
use std::{collections::BTreeMap, path::PathBuf};
use worldrunner_compat_content::{
    AdmissionPolicy, ImportProfile, ImportedScene, import_project, materialization_plan,
    materialize,
};

fn snake() -> ImportedScene {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.work/games/snake");
    let sidecar = serde_json::from_value(json!({"reference_substitutions":[{
        "guid":"311925a002f4447b3a28927169b83ea6", "file_id":7_482_667_652_216_324_306_i64,
        "replacement":{"$builtin":"authored.unit-square"}, "reason":"Explicit missing-art substitute"
    }]})).unwrap();
    import_project(
        &root,
        "Assets/Scenes/GameScene.unity".as_ref(),
        &ImportProfile::default(),
        &sidecar,
    )
    .unwrap()
}
fn policy() -> AdmissionPolicy {
    AdmissionPolicy {
        ignored_components: BTreeMap::from([(
            "AudioListener".into(),
            "Audio outside selected admission".into(),
        )]),
        ..AdmissionPolicy::default()
    }
}
fn reply(request: &Value, next: &mut u64) -> Value {
    *next += 2;
    match request["op"].as_str().unwrap() {
        "object.create" => {
            json!({"handle":*next,"components":[{"handle":*next+1,"kind":"Transform"}]})
        }
        "component.add" => json!({"handle":*next}),
        _ => Value::Null,
    }
}

#[test]
fn unknown_participants_are_rejected_before_any_native_side_effects() {
    let mut calls = 0;
    let result = materialize(&snake(), &AdmissionPolicy::default(), |_| {
        calls += 1;
        Ok::<_, String>(Value::Null)
    });
    assert!(
        result.is_err(),
        "unsupported audio requires explicit admission policy"
    );
    assert_eq!(calls, 0);
}

#[test]
fn materialization_maps_real_transforms_forward_refs_and_keeps_data_assets_stable() {
    let graph = snake();
    let mut requests = Vec::new();
    let mut next = 9_007_199_254_740_992_u64;
    let result = materialize(&graph, &policy(), |request| {
        let data = reply(&request, &mut next);
        requests.push(request);
        Ok::<_, String>(data)
    })
    .unwrap();
    assert_eq!(result.object_ids.len(), graph.nodes.len());
    for c in graph
        .components
        .iter()
        .filter(|c| c.native_kind.as_deref() == Some("Transform"))
    {
        assert_ne!(result.handles[&c.id], result.transform_objects[&c.id]);
        assert_eq!(
            result.transform_objects[&c.id],
            result.object_ids[c.object.as_ref().unwrap()]
        );
    }
    let view = graph
        .components
        .iter()
        .find(|c| {
            c.script
                .as_ref()
                .is_some_and(|s| s.type_name == "SnakeView")
        })
        .unwrap();
    let segment = view.script.as_ref().unwrap().fields["segmentPrefab"]["$ref"]
        .as_str()
        .unwrap();
    let patch = requests
        .iter()
        .find(|r| r["op"] == "component.set" && r["component"] == result.handles[&view.id])
        .unwrap();
    assert_eq!(
        patch["fields"]["segmentPrefab"]["$ref"],
        result.handles[segment]
    );
    let loop_component = graph
        .components
        .iter()
        .find(|c| c.script.as_ref().is_some_and(|s| s.type_name == "GameLoop"))
        .unwrap();
    let loop_patch = requests
        .iter()
        .find(|r| {
            r["op"] == "component.set" && r["component"] == result.handles[&loop_component.id]
        })
        .unwrap();
    assert!(loop_patch["fields"]["config"]["$ref"].is_string());
    assert_eq!(
        result.binding_payload(123, &graph)["handles"][&view.id],
        result.handles[&view.id]
    );
}

#[test]
fn templates_stay_native_inactive_and_scene_pose_uses_original_local_basis() {
    let graph = snake();
    let mut requests = Vec::new();
    let mut next = 100_u64;
    let result = materialize(&graph, &policy(), |request| {
        let data = reply(&request, &mut next);
        requests.push(request);
        Ok::<_, String>(data)
    })
    .unwrap();
    assert!(
        requests
            .iter()
            .filter(|r| r["op"] == "object.create")
            .all(|r| r["active"] == false)
    );
    for node in &graph.nodes {
        let handle = result.object_ids[&node.id];
        let activation = requests
            .iter()
            .find(|r| r["op"] == "object.set" && r["object"] == handle)
            .unwrap();
        assert_eq!(activation["active"], node.active && !node.template);
        let pose = requests
            .iter()
            .find(|r| r["op"] == "transform.set" && r["object"] == handle)
            .unwrap();
        assert_eq!(pose["position"], json!(node.transform.local_position));
        assert_eq!(pose["rotation"], json!(node.transform.local_rotation));
    }
}

#[test]
fn dangling_refs_and_empty_omission_reasons_fail_preflight() {
    let mut graph = snake();
    let script = graph
        .components
        .iter_mut()
        .find(|c| c.native_kind.as_deref() == Some("Script") && c.object.is_some())
        .unwrap();
    script.native_fields["bad"] = json!({"$ref":"no-such-source:1"});
    assert!(materialization_plan(&graph, &policy()).is_err());
    let bad_policy = AdmissionPolicy {
        ignored_components: BTreeMap::from([("AudioListener".into(), String::new())]),
        ..AdmissionPolicy::default()
    };
    assert!(materialization_plan(&snake(), &bad_policy).is_err());
}

#[test]
fn failed_native_request_is_reported_without_retry_or_continuation() {
    let mut calls = 0;
    let result = materialize(&snake(), &policy(), |_| {
        calls += 1;
        Err::<Value, _>("native allocation failed")
    });
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("native allocation failed")
    );
    assert_eq!(calls, 1);
}
