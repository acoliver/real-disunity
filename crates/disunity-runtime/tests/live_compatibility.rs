#[path = "support/handshake.rs"]
pub mod support;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use worldrunner_disunity_runtime::{CompatibilityHost, ManagedHostConfig};

fn configure() -> (ManagedHostConfig, PathBuf) {
    (support::artifacts().config(), support::artifacts().probe())
}

#[test]
fn live_private_callbacks_bind_before_awake_and_mutate_the_same_native_world() {
    let (config, assembly) = configure();
    let mut host = CompatibilityHost::boot(&config).unwrap();
    assert_eq!(host.store_statistics().unwrap().0, 0);
    for _ in 0..2 {
        let (world, object) = load_probe(&mut host, &assembly);
        let awake = host
            .request(
                world,
                &json!({"version":1,"op":"transform.get","object":object}),
            )
            .unwrap();
        assert_eq!(awake["position"], json!([7., 0., 0.]));
        host.request(
            world,
            &json!({"version":1,"op":"input.set","keys":["W"],"axes":{}}),
        )
        .unwrap();
        host.request(world, &json!({"version":1,"op":"frame.begin","dt":0.25}))
            .unwrap();
        host.dispatch_phase(&json!({"phase":"Update"})).unwrap();
        let moved = host
            .request(
                world,
                &json!({"version":1,"op":"transform.get","object":object}),
            )
            .unwrap();
        assert_eq!(moved["position"], json!([7.25, 1., 0.]));
        let observed = host.observe(&json!({"op":"world.snapshot"})).unwrap();
        assert_eq!(
            observed["data"]["objects"][0]["transform"]["position"],
            moved["position"]
        );
        assert!(host.observe(&json!({"op":"unknown"})).is_err());
        host.unload().unwrap();
        let roots = host.observe(&json!({"op":"managed.statistics"})).unwrap();
        assert_eq!(roots["data"]["scripts"], 0);
        assert_eq!(roots["data"]["coroutines"], 0);
        host.destroy_world(world).unwrap();
        assert_eq!(host.store_statistics().unwrap().0, 0);
    }
}

fn load_probe(host: &mut CompatibilityHost, assembly: &Path) -> (u64, u64) {
    let world = host.create_world().unwrap();
    let request = |value: Value| host.request(world, &value).unwrap();
    let object = request(json!({"version":1,"op":"object.create","name":"live-probe"}))["handle"]
        .as_u64()
        .unwrap();
    let transform = request(
        json!({"version":1,"op":"component.list","object":object,"kind":"Transform"}),
    )[0]["handle"]
        .as_u64()
        .unwrap();
    let script = request(json!({"version":1,"op":"component.add","object":object,"kind":"Script","fields":{"type":"HostProbe"}}))["handle"].as_u64().unwrap();
    host.load_fixture(&json!({"world":world,"assemblies":[assembly],"handles":{"owner":object,"transform":transform,"script":script},"profile":{"id":"compat-6000.3"},"graph":{
            "nodes":[{"id":"owner","template":false,"active":true}],"assets":[],"aliases":{},
            "components":[
                {"id":"transform","object":"owner","script":null},
                {"id":"script","object":"owner","script":{"type_name":"HostProbe","fields":{"displacement":7,"target":{"$ref":"transform"}}}}
            ]}})).unwrap();
    (world, object)
}
