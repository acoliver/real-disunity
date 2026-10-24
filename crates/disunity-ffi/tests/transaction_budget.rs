mod support;
use serde_json::json;
use support::World;

#[test]
fn aggregate_budget_stops_batch_before_later_operations_and_retains_rollback() {
    let world = World::new();
    let object =
        world.call(json!({"op":"object.create","name":"n".repeat(600_000)}))["data"]["handle"]
            .as_u64()
            .unwrap();
    let before = world.snapshot();
    let mut operations = vec![json!({"op":"object.set","object":object,"active":false})];
    operations.extend((0..30).map(|_| json!({"op":"object.get","object":object})));
    operations.push(json!({"op":"object.set","object":0,"name":"unreachable"}));
    let request = json!({"version":1,"requestId":"bounded-results","op":"transaction","operations":operations});
    let error = world.call(request.clone());
    assert_eq!(error["ok"], false);
    assert_eq!(
        error["error"]["message"], "response budget exceeded; operation rolled back",
        "an oversized prefix must terminate result accumulation"
    );
    assert_eq!(world.snapshot(), before);
    world.call(json!({"op":"object.set","object":object,"name":"small"}));
    assert_eq!(world.call(request), error);
    assert_eq!(world.snapshot()["objects"][0]["active"], true);
}
