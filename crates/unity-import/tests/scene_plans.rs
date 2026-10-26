//! Scene plans: what the managed loader builds from each game, and what planning refuses.
#[path = "support/assets.rs"]
mod assets;
#[path = "support/fixture.rs"]
mod fixture;
#[path = "support/objects.rs"]
mod objects;

use std::path::Path;

use fixture::Fixture;
use objects::{game_object, transform};
use serde_json::{Value, json};
use worldrunner_original_games::{Game, checkout};
use worldrunner_unity_import::{PlanComponent, Project, ScenePlan, plan_scene};

fn original(game: Game, scene: &str) -> Result<ScenePlan, String> {
    let project = Project::open(&checkout(game).unwrap()).unwrap();
    plan_scene(&project, Path::new(scene)).map_err(|error| error.to_string())
}

fn json(plan: &ScenePlan) -> Value {
    serde_json::to_value(plan).unwrap()
}

fn index(plan: &Value, name: &str) -> usize {
    let objects = plan["objects"].as_array().unwrap();
    objects.iter().position(|o| o["name"] == name).unwrap()
}

/// The fields of the first script of class `class`.
fn fields<'a>(plan: &'a Value, class: &str) -> &'a Value {
    let objects = plan["objects"].as_array().unwrap();
    let components = objects
        .iter()
        .flat_map(|o| o["components"].as_array().unwrap());
    &components
        .into_iter()
        .find(|c| c["class"] == class)
        .unwrap()["fields"]
}

#[test]
fn breakout_plans_its_prefabs_as_templates_and_its_bricks_as_sprite_shaped_tiles() {
    let plan = original(Game::Breakout, "Assets/Scenes/SampleScene.unity").unwrap();
    let value = json(&plan);
    let templates: Vec<_> = plan
        .objects
        .iter()
        .filter(|o| o.template)
        .map(|o| o.name.as_str())
        .collect();
    assert_eq!(templates, ["Ball", "Brick"]);
    assert_eq!(plan.objects.len(), 12);
    assert_eq!(
        fields(&value, "BrickSpawner")["brickPrefab"],
        json!({ "$object": index(&value, "Brick") })
    );
    assert_eq!(
        fields(&value, "BallSpawner")["ballPrefab"],
        json!({ "$object": index(&value, "Ball") })
    );
    assert_eq!(
        fields(&value, "Platform")["normalColliderSize"],
        json!({ "x": "2", "y": "0.62" })
    );
    let tilemap = plan.objects[index(&value, "Tilemap")]
        .components
        .iter()
        .find_map(|c| match c {
            PlanComponent::Tilemap(map) => Some(map),
            _ => None,
        })
        .unwrap();
    assert_eq!(tilemap.cells.len(), 48);
    assert_eq!(
        tilemap.palette.len(),
        1,
        "every brick cell shares one entry"
    );
    let brick = &plan.sprites[tilemap.palette[0].sprite.unwrap()];
    let rectangle = vec![vec![[-1., -0.5], [1., -0.5], [1., 0.5], [-1., 0.5]]];
    assert_eq!(brick.physics_shape.as_ref(), Some(&rectangle));
    assert_eq!(value["assets"][0]["type"], "PhysicsMaterial2D");
}

#[test]
fn pong_plans_scripts_text_scene_events_and_axes() {
    let plan = original(Game::Pong, "Assets/Scenes/Main.unity").unwrap();
    let value = json(&plan);
    let objects = value["objects"].as_array().unwrap();
    for (i, object) in objects.iter().enumerate() {
        assert!(
            object["parent"].as_u64().is_none_or(|p| p < i as u64),
            "parents come first"
        );
    }
    let manager = index(&value, "MatchManager");
    let call = &fields(&value, "Goal")["scoreEvent"]["m_PersistentCalls"]["m_Calls"][0];
    assert_eq!(call["m_Target"], json!({ "$component": [manager, 0] }));
    assert_eq!(call["m_MethodName"], "ScorePoint");
    let scores = [
        index(&value, "PlayerOneScore"),
        index(&value, "PlayerTwoScore"),
    ];
    assert_eq!(
        fields(&value, "ScoreViewManager")["currentScore"],
        json!([{ "$component": [scores[0], 1] }, { "$component": [scores[1], 1] }])
    );
    assert_eq!(objects[scores[0]]["components"][0]["type"], "Text");
    assert_eq!(value["fonts"], json!(["Arial"]));
    assert_eq!(plan.axes.len(), 19);
}

#[test]
fn snake_fails_on_the_square_sprite_its_scene_misses() {
    let error = original(Game::Snake, "Assets/Scenes/GameScene.unity").unwrap_err();
    assert!(
        error.starts_with("SpriteRenderer ") && error.contains("GameScene.unity"),
        "{error}"
    );
    assert!(
        error.contains("311925a002f4447b3a28927169b83ea6"),
        "{error}"
    );
}

const SCRIPT: &str = "d0000000000000000000000000000001";
const CONFIG_SCRIPT: &str = "d0000000000000000000000000000002";
const CONFIG: &str = "d0000000000000000000000000000003";
const CLIP: &str = "d0000000000000000000000000000004";

fn script(id: u64, object: u64, fields: &str) -> String {
    format!(
        "--- !u!114 &{id}\nMonoBehaviour:\n  m_GameObject: {{fileID: {object}}}\n  m_Enabled: 0\n  m_Script: {{fileID: 11500000, guid: {SCRIPT}, type: 3}}\n  m_Name: \n{fields}"
    )
}

/// A scene with `Spawner` holding a script with `fields`, and its child `Target` with a script.
fn plan(fields: &str, extra: &str) -> Result<ScenePlan, String> {
    let fixture = Fixture::new("6000.3.5f2");
    fixture.asset(
        "Assets/Spawner.cs",
        SCRIPT,
        "MonoImporter",
        "class Spawner {}",
    );
    fixture.asset(
        "Assets/Config.cs",
        CONFIG_SCRIPT,
        "MonoImporter",
        "class Config {}",
    );
    let config = format!(
        "--- !u!114 &11400000\nMonoBehaviour:\n  m_Script: {{fileID: 11500000, guid: {CONFIG_SCRIPT}, type: 3}}\n  m_Name: Fast\n  speed: 3\n"
    );
    fixture.asset("Assets/Fast.asset", CONFIG, "NativeFormatImporter", config);
    fixture.asset("Assets/hit.wav", CLIP, "AudioImporter", "RIFF");
    let text = [
        game_object(1, "Spawner", &[2, 12]),
        transform(2, 1, 0.0, 0).replace("m_Children: []", "m_Children:\n  - {fileID: 4}"),
        script(12, 1, fields),
        game_object(3, "Target", &[4, 14]),
        transform(4, 3, 1.0, 2),
        script(14, 3, ""),
        extra.to_owned(),
    ]
    .concat();
    objects::import(&fixture, "Assets/Main.unity", &text).map_err(|e| e.to_string())?;
    plan_scene(&fixture.project(), Path::new("Assets/Main.unity")).map_err(|e| e.to_string())
}

#[test]
fn script_fields_name_objects_transforms_components_and_assets_by_index() {
    let fields = "  target: {fileID: 3}\n  anchor: {fileID: 4}\n  peer: {fileID: 14}\n  config: {fileID: 11400000, guid: d0000000000000000000000000000003, type: 2}\n  waves:\n  - count: 2\n    nobody: {fileID: 0}\n";
    let plan = plan(fields, "").unwrap();
    let value = json(&plan);
    let spawner = &value["objects"][0]["components"][0];
    assert_eq!(spawner["class"], "Spawner");
    assert_eq!(spawner["enabled"], false);
    assert_eq!(
        spawner["fields"],
        json!({
            "target": { "$object": 1 },
            "anchor": { "$transform": 1 },
            "peer": { "$component": [1, 0] },
            "config": { "$asset": 0 },
            "waves": [{ "count": "2", "nobody": null }],
        })
    );
    assert_eq!(value["objects"][1]["parent"], 0);
    assert_eq!(
        value["assets"][0],
        json!({ "type": "Script", "script": "Assets/Config.cs", "class": "Config", "name": "Fast", "fields": { "speed": "3" } })
    );
}

#[test]
fn planning_refuses_what_the_engine_cannot_build_and_names_where() {
    let clip = "  hit: {fileID: 8300000, guid: d0000000000000000000000000000004, type: 3}\n";
    let error = plan(clip, "").unwrap_err();
    assert!(
        error.contains("MonoBehaviour 12") && error.contains("hit: references asset"),
        "{error}"
    );
    let animator = "--- !u!95 &15\nAnimator:\n  m_GameObject: {fileID: 3}\n  m_Enabled: 1\n";
    let error = plan("", animator).unwrap_err();
    assert!(
        error.contains("Animator 15") && error.contains("not supported"),
        "{error}"
    );
    let orphan = game_object(5, "Orphan", &[]);
    let error = plan("", &orphan).unwrap_err();
    assert!(
        error.contains("GameObject 5") && error.contains("outside every"),
        "{error}"
    );
}
