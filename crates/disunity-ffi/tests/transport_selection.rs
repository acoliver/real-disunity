mod transport_support;
use serde_json::json;
use transport_support::*;

#[test]
fn explicit_selection_is_immutable_and_effective_query_is_readonly() {
    let world = World::new();
    assert_eq!(
        world.call(json!({"version":1,"op":"transport.get"}))["data"]["transport_profile"],
        "legacy"
    );
    let before = world.call(json!({"op":"snapshot"}));
    let selected = world.select();
    assert_eq!(selected["ok"], true, "{selected}");
    assert_eq!(selected["data"]["transport_profile"], PROFILE);
    assert_eq!(selected["data"]["limits"]["max_response_bytes"], REPLY);
    assert_eq!(
        selected["data"]["limits"]["max_outstanding_bytes"],
        OUTSTANDING
    );
    assert_eq!(selected["data"]["managed_integration"], false);
    assert_eq!(world.call(json!({"op":"snapshot"})), before);
    assert_eq!(world.select()["ok"], false);
    assert_eq!(
        world.call(json!({"version":1,"op":"transport.select","transport_profile":"legacy"}))["ok"],
        false
    );
    let handle = world.0;
    let effective =
        std::thread::spawn(move || call(handle, json!({"version":1,"op":"transport.get"})))
            .join()
            .unwrap();
    assert_eq!(effective["data"], selected["data"]);
}

#[test]
fn invalid_late_retained_and_nested_selection_leave_world_and_profile_unchanged() {
    let world = World::new();
    for request in [
        json!({"op":"transport.select","transport_profile":PROFILE}),
        json!({"version":1,"op":"transport.select","transport_profile":"unknown"}),
        json!({"version":1,"op":"transport.select","transport_profile":PROFILE,"secret":"ignored"}),
        json!({"version":1,"requestId":"select","op":"transport.select","transport_profile":PROFILE}),
        json!({"version":1,"op":"transaction","operations":[{"op":"transport.select","transport_profile":PROFILE}]}),
    ] {
        assert_eq!(world.call(request)["ok"], false);
    }
    assert_eq!(
        world.call(json!({"version":1,"op":"transport.get"}))["data"]["transport_profile"],
        "legacy"
    );
    world.call(json!({"op":"object.create"}));
    assert_eq!(world.select()["ok"], false);
    assert_eq!(
        world.call(json!({"op":"snapshot"}))["data"]["objects"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn rejected_retained_transport_control_does_not_consume_identity_or_prevent_selection() {
    let world = World::new();
    let invalid = json!({"version":1,"requestId":"unused","op":"transport.select","transport_profile":PROFILE});
    assert_eq!(world.call(invalid)["ok"], false);
    assert_eq!(world.select()["ok"], true);
    assert_eq!(
        world.call(json!({"version":1,"requestId":"unused","op":"object.create"}))["ok"],
        true
    );
}
