use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
};
use worldrunner_compat_content::{
    AdmissionPolicy, ImportProfile, ImportSidecar, ImportedScene, import_project, materialize,
};

fn fixture(game: &str, scene: &str) -> (ImportedScene, AdmissionPolicy) {
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let read = |path| std::fs::read(base.join(path)).unwrap();
    let profile: ImportProfile =
        serde_json::from_slice(&read(format!("profiles/{game}-source.json"))).unwrap();
    let sidecar: ImportSidecar =
        serde_json::from_slice(&read(format!("overlays/{game}.json"))).unwrap();
    let policy = serde_json::from_slice(&read(format!("profiles/{game}-admission.json"))).unwrap();
    let graph = import_project(
        &worldrunner_original_games::checkout(game.parse().unwrap()).unwrap(),
        format!("Assets/Scenes/{scene}.unity").as_ref(),
        &profile,
        &sidecar,
    )
    .unwrap();
    (graph, policy)
}

#[test]
fn all_three_source_profiles_materialize_without_silent_native_reference_omissions() {
    for (game, scene) in [
        ("snake", "GameScene"),
        ("breakout", "SampleScene"),
        ("pong", "Main"),
    ] {
        let (graph, policy) = fixture(game, scene);
        let mut next = 1_000_u64;
        let mut transforms = BTreeMap::new();
        let result = materialize(&graph, &policy, |request| {
            next += 2;
            Ok::<_, String>(match request["op"].as_str().unwrap() {
                "object.create" => {
                    transforms.insert(next, next + 1);
                    json!({"handle":next,"components":[{"handle":next+1,"kind":"Transform"}]})
                }
                "component.add" if request["kind"] == "RectTransform" => {
                    json!({"handle":transforms[&request["object"].as_u64().unwrap()]})
                }
                "component.add" => json!({"handle":next}),
                _ => Value::Null,
            })
        })
        .unwrap_or_else(|e| panic!("{game}: {e}"));
        assert!(
            graph
                .nodes
                .iter()
                .all(|n| result.object_ids.contains_key(&n.id))
        );
        for c in graph.components.iter().filter(|c| c.object.is_some()) {
            assert!(
                result.component_ids.contains_key(&c.id)
                    || result.skipped_components.contains_key(&c.id),
                "{game}: {}",
                c.id
            );
        }
        assert!(
            result
                .skipped_components
                .values()
                .all(|s| !s.trim().is_empty())
        );
    }
}

#[test]
fn pong_goal_enum_dynamic_event_and_score_text_references_keep_actual_scene_values() {
    let (graph, _) = fixture("pong", "Main");
    let goals: Vec<_> = graph
        .components
        .iter()
        .filter(|c| !c.source.instance.is_empty())
        .filter_map(|c| c.script.as_ref())
        .filter(|s| s.type_name == "Goal")
        .collect();
    let values: BTreeSet<_> = goals
        .iter()
        .map(|g| g.fields["scoresTo"].as_i64().unwrap())
        .collect();
    assert_eq!(values, BTreeSet::from([0, 1]));
    for goal in goals {
        assert!(goal.fields.get("team").is_none());
        let event = &goal.fields["scoreEvent"]["m_PersistentCalls"]["m_Calls"][0];
        assert_eq!(
            event["m_Mode"], 0,
            "dynamic enum argument uses the invoked Players value"
        );
        assert_eq!(event["m_CallState"], 2);
        assert_eq!(event["m_MethodName"], "ScorePoint");
        let target = event["m_Target"]["$ref"].as_str().unwrap();
        assert!(graph.components.iter().any(|c| {
            c.id == target
                && c.script
                    .as_ref()
                    .is_some_and(|s| s.type_name == "MatchManager")
        }));
    }
    let text_count = graph
        .components
        .iter()
        .filter(|c| c.native_kind.as_deref() == Some("Text"))
        .count();
    assert_eq!(text_count, 4);
}

#[test]
fn breakout_current_ball_tag_and_migrated_speed_are_source_driven_and_diagnosed() {
    let (graph, _) = fixture("breakout", "SampleScene");
    let ball = graph
        .components
        .iter()
        .filter_map(|c| c.script.as_ref())
        .find(|s| s.type_name == "Ball")
        .unwrap();
    assert_eq!(ball.fields["speed"], 6);
    assert!(ball.fields.get("startSpeed").is_none());
    assert_eq!(
        graph.nodes.iter().find(|n| n.name == "Ball").unwrap().tag,
        "Ball"
    );
    assert!(
        graph
            .diagnostics
            .iter()
            .any(|d| d.code == "source_field_alias"
                && d.field.as_deref() == Some("startSpeed")
                && d.message.contains("speed"))
    );
}
