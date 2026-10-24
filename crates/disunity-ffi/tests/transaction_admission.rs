mod support;
use serde_json::json;
use support::World;

#[test]
fn fixed_admission_budget_rolls_back_prior_input_time_random_lifetime_and_objects() {
    let world = World::new();
    assert_eq!(
        world.call(json!({"op":"random.seed","seed":123}))["ok"],
        true
    );
    let draw = json!({"op":"random.range","min":0,"max":1000000,"integer":true});
    let expected = world.call(draw.clone());
    world.call(json!({"op":"random.seed","seed":123}));
    let before = world.snapshot();
    let request = json!({"version":1,"requestId":"admission-budget","op":"transaction","operations":[
        {"op":"object.create"}, {"op":"lifetime.begin"},
        {"op":"input.set","keys":["Space"]},
        {"op":"input.configure_axes","axes":{"Horizontal":{"positive_keys":["D"]}}},
        {"op":"random.range","min":0,"max":1000000,"integer":true},
        {"op":"time.set","fixed_delta_time":0.000001}, {"op":"frame.begin","dt":1}
    ]});
    let failed = world.call(request.clone());
    assert_eq!(failed["ok"], false);
    assert_eq!(
        failed["error"]["message"],
        "fixed-step admission exceeds 1024 steps per frame"
    );
    assert_eq!(world.snapshot(), before);
    assert_eq!(world.call(draw), expected);
    assert_eq!(world.call(json!({"op":"lifetime.end"}))["ok"], false);
    assert_eq!(world.call(request), failed);
    assert_eq!(
        world.call(json!({"op":"input.set","keys":["D"]}))["data"]["axes"],
        json!({})
    );
    assert_eq!(
        world.call(json!({"op":"frame.begin","dt":0.02}))["data"]["fixed_steps"],
        1
    );
}
