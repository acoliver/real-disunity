mod transport_support;
use serde_json::json;
use transport_support::*;

#[test]
fn new_world_limits_are_immutable_and_cross_thread_query_is_readonly() {
    let world = World::new();
    let before = world.call(json!({"op":"snapshot"}));
    let effective = world.call(json!({"version":1,"op":"transport.get"}));
    assert_eq!(effective["data"]["limits"]["max_response_bytes"], REPLY);
    assert_eq!(
        effective["data"]["limits"]["max_outstanding_bytes"],
        OUTSTANDING
    );
    world.call(json!({"op":"object.create"}));
    let handle = world.0;
    let queried =
        std::thread::spawn(move || call(handle, json!({"version":1,"op":"transport.get"})))
            .join()
            .unwrap();
    assert_eq!(queried, effective);
    assert_eq!(before["data"]["objects"], json!([]));
    assert_eq!(
        world.call(json!({"op":"snapshot"}))["data"]["objects"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn invalid_retained_query_does_not_consume_identity() {
    let world = World::new();
    for request in [
        json!({"op":"transport.get"}),
        json!({"version":1,"op":"transport.get","extra":true}),
        json!({"version":1,"requestId":"unused","op":"transport.get"}),
        json!({"version":1,"op":"transaction","operations":[{"op":"transport.get"}]}),
    ] {
        assert_eq!(world.call(request)["ok"], false);
    }
    assert_eq!(
        world.call(json!({"version":1,"requestId":"unused","op":"object.create"}))["ok"],
        true
    );
}
