mod support;
use serde_json::{Value, json};
use support::World;
use worldrunner_disunity_ffi::abi::*;

fn queries(world: &World, object: u64, component: u64) -> Vec<(Value, Value)> {
    [
        json!({"op":"describe"}),
        json!({"op":"snapshot"}),
        json!({"op":"lifecycle.snapshot"}),
        json!({"op":"lifecycle.scripts"}),
        json!({"op":"object.alive","object":object}),
        json!({"op":"component.alive","component":component}),
        json!({"op":"object.get","object":object}),
        json!({"op":"object.find","include_inactive":true}),
        json!({"op":"transform.get","object":object}),
        json!({"op":"component.get","component":component}),
        json!({"op":"component.list","object":object}),
        json!({"op":"input.get"}),
        json!({"op":"time.get"}),
        json!({"op":"physics.overlap","point":[0,0]}),
        json!({"op":"physics.raycast","origin":[-2,0],"direction":[1,0],"distance":4}),
        json!({"op":"ui.layout"}),
    ]
    .into_iter()
    .map(|query| {
        let expected = world.call(query.clone());
        assert_eq!(expected["ok"], true);
        (query, expected)
    })
    .collect()
}

#[test]
fn unretained_reads_cross_threads_but_retained_reads_and_replay_require_owner() {
    let world = World::new();
    let created = world.call(json!({"version":1,"requestId":"created","op":"object.create"}));
    let object = created["data"]["handle"].as_u64().unwrap();
    let component = created["data"]["components"][0]["handle"].as_u64().unwrap();
    let reads = queries(&world, object, component);
    let denied = json!({"version":1,"requestId":"wrong-thread","op":"time.get"});
    let handle = world.0;
    let first = denied.clone();
    std::thread::spawn(move || {
        for (query, expected) in reads {
            assert_eq!(
                support::call(handle, &serde_json::to_vec(&query).unwrap()),
                expected
            );
        }
        for request in [
            first,
            json!({"version":1,"requestId":"created","op":"object.create"}),
            json!({"version":1,"op":"transaction","operations":[{"op":"time.get"}]}),
            json!({"op":"events.read","clear":false}),
            json!({"op":"physics.step","dt":0}),
            json!({"op":"time.phase","phase":"fixed"}),
        ] {
            let error = support::call(handle, &serde_json::to_vec(&request).unwrap());
            assert_eq!(error["ok"], false);
            assert_eq!(
                error["error"]["message"],
                "world mutation requires its owner thread"
            );
        }
    })
    .join()
    .unwrap();
    assert_eq!(
        world.call(denied)["ok"],
        true,
        "wrong-thread admission must not retain an error"
    );
    assert_eq!(
        world.call(json!({"version":1,"requestId":"created","op":"object.create"})),
        created
    );
    assert_eq!(world.snapshot()["objects"].as_array().unwrap().len(), 1);
}

#[test]
fn response_copy_and_free_cross_threads_and_survive_owner_world_teardown() {
    let world = World::new();
    let (code, response) = support::submit(
        world.0,
        br#"{"version":1,"requestId":"owned","op":"object.create"}"#,
    );
    assert_eq!(code, 0);
    let expected = support::copy(response);
    drop(world);
    std::thread::spawn(move || {
        assert_eq!(support::copy(response), expected);
        assert_eq!(unsafe { disunity_response_free(response) }.code, 0);
        assert_eq!(unsafe { disunity_response_free(response) }.code, 1);
    })
    .join()
    .unwrap();
}
