mod support;

use serde_json::{Value, json};
use support::World;

fn scripts(world: &World) -> Value {
    let result = world.call(json!({"op":"lifecycle.scripts"}));
    assert_eq!(result["ok"], true, "{result}");
    result["data"].clone()
}

fn equivalent(world: &World) {
    let full = world.call(json!({"op":"lifecycle.snapshot"}));
    let expected: Vec<_> = full["data"]["scripts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| json!([s["handle"], s["active_in_hierarchy"], s["enabled"]]))
        .collect();
    assert_eq!(scripts(world), json!(expected));
}

#[test]
fn fresh_script_projection_tracks_mutations_without_component_payloads() {
    let world = World::new();
    let parent = world.call(json!({"op":"object.create"}))["data"]["handle"].clone();
    let child = world.call(json!({"op":"object.create"}))["data"]["handle"].clone();
    world.call(json!({"op":"transform.parent","object":child,"parent":parent}));
    let script = world.call(
        json!({"op":"component.add","object":child,"kind":"Script","fields":{"type":"Probe"}}),
    )["data"]["handle"]
        .clone();
    equivalent(&world);
    assert_eq!(scripts(&world), json!([[script, true, true]]));
    for mutation in [
        json!({"op":"object.set","object":parent,"active":false}),
        json!({"op":"component.set","component":script,"enabled":false}),
        json!({"op":"transform.parent","object":child,"parent":null}),
        json!({"op":"component.set","component":script,"enabled":true}),
        json!({"op":"object.clone","object":child}),
        json!({"op":"component.remove","component":script}),
        json!({"op":"object.destroy","object":child}),
    ] {
        assert_eq!(world.call(mutation)["ok"], true);
        equivalent(&world);
    }
}

#[test]
fn script_projection_has_bounded_per_script_bytes_and_no_non_script_growth() {
    let world = World::new();
    let object = world.call(json!({"op":"object.create"}))["data"]["handle"].clone();
    world.call(json!({"op":"component.add","object":object,"kind":"Script"}));
    let before = scripts(&world);
    let initial = world.snapshot();
    for _ in 0..100 {
        world.call(json!({"op":"component.add","object":object,"kind":"BoxCollider2D"}));
    }
    assert_eq!(scripts(&world), before);
    assert_eq!(world.snapshot()["time"], initial["time"]);
    assert!(serde_json::to_vec(&before).unwrap().len() <= 48);
    assert_eq!(
        world.call(json!({"op":"lifecycle.scripts","unknown":1}))["ok"],
        false
    );
}

#[test]
fn compact_capability_is_explicit_and_pure() {
    let world = World::new();
    let before = world.snapshot();
    let description = world.call(json!({"op":"describe"}));
    assert!(
        description["data"]["capabilities"]
            .as_array()
            .unwrap()
            .contains(&json!("lifecycle_scripts"))
    );
    assert_eq!(world.snapshot(), before);
}
