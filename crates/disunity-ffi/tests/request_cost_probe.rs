//! Measurement aid, ignored by default: per-request cost against world size.
//! Mutations stage a whole-world copy for rollback; reads do not. Comparing the two
//! at increasing sizes separates staging cost from operation cost. No assertions on time.

mod support;

use serde_json::json;
use std::time::Instant;
use support::World;

fn populate(world: &World, objects: usize) -> u64 {
    let mut last = 0;
    for index in 0..objects {
        let handle = world.call(json!({"op":"object.create","name":format!("brick-{index}")}))["data"]["handle"]
            .as_u64()
            .unwrap();
        world.call(
            json!({"op":"component.add","object":handle,"kind":"SpriteRenderer",
            "fields":{"color":[1.,1.,1.,1.],"sorting_layer":0,"sorting_order":0}}),
        );
        world.call(json!({"op":"component.add","object":handle,"kind":"BoxCollider2D","fields":{"size":[1.,0.5]}}));
        last = handle;
    }
    last
}

fn mean_micros(world: &World, request: &serde_json::Value, repeats: u32) -> f64 {
    let started = Instant::now();
    for _ in 0..repeats {
        assert_eq!(world.call(request.clone())["ok"], true);
    }
    started.elapsed().as_secs_f64() * 1e6 / f64::from(repeats)
}

#[test]
#[ignore = "timing measurement; run explicitly in a release build"]
fn request_cost_against_world_size() {
    let mut rows = Vec::new();
    for size in [10, 50, 150, 300] {
        let world = World::new();
        let handle = populate(&world, size);
        assert_eq!(world.snapshot()["objects"].as_array().unwrap().len(), size);
        let read = json!({"op":"object.get","object":handle});
        let write = json!({"op":"transform.set","object":handle,"position":[1.,2.,0.]});
        let snapshot = json!({"op":"snapshot"});
        rows.push(json!({"objects":size,"object_get_us":mean_micros(&world,&read,200),
            "transform_set_us":mean_micros(&world,&write,200),"snapshot_us":mean_micros(&world,&snapshot,50)}));
    }
    println!("{}", serde_json::to_string_pretty(&rows).unwrap());
}

fn replay(world: &World, description: &serde_json::Value) -> Vec<u64> {
    let objects = description["objects"].as_array().unwrap();
    let mut handles = Vec::new();
    for object in objects {
        let created = world.call(json!({"op":"object.create","name":object["name"],
            "tag":object["tag"],"layer":object["layer"]}));
        handles.push(created["data"]["handle"].as_u64().unwrap());
    }
    for (object, handle) in objects.iter().zip(&handles) {
        if let Some(parent) = object["parent"].as_u64() {
            let index = usize::try_from(parent).unwrap();
            world.call(json!({"op":"transform.parent","object":handle,"parent":handles[index]}));
        }
        world.call(json!({"op":"transform.set","object":handle,"position":object["position"],"space":"local"}));
        for component in object["components"].as_array().unwrap() {
            let added = world.call(json!({"op":"component.add","object":handle,
                "kind":component["kind"],"fields":component["fields"]}));
            assert_eq!(added["ok"], true, "{component}: {added}");
        }
    }
    handles
}

#[test]
#[ignore = "timing measurement; run explicitly in a release build"]
fn request_cost_in_replayed_world() {
    let description = json!({"objects":[
        {"name":"root","tag":"Untagged","layer":0,"parent":null,"position":[1,2,0],
            "components":[{"kind":"SpriteRenderer","fields":{"color":[1,1,1,1]}}]},
        {"name":"brick","tag":"Untagged","layer":0,"parent":0,"position":[2,0,0],
            "components":[{"kind":"BoxCollider2D","fields":{"size":[1,0.5]}}]},
        {"name":"ball","tag":"Untagged","layer":0,"parent":0,"position":[0,1,0],
            "components":[{"kind":"Rigidbody2D","fields":{"gravity_scale":0}}]}
    ]});
    let world = World::new();
    let handles = replay(&world, &description);
    let target = *handles.last().unwrap();
    assert_eq!(
        world.snapshot()["objects"].as_array().unwrap().len(),
        handles.len()
    );
    let read = json!({"op":"object.get","object":target});
    let write = json!({"op":"transform.set","object":target,"position":[1.,2.,0.]});
    let snapshot = json!({"op":"snapshot"});
    println!(
        "{}",
        json!({"objects":handles.len(),"object_get_us":mean_micros(&world,&read,200),
            "transform_set_us":mean_micros(&world,&write,200),"snapshot_us":mean_micros(&world,&snapshot,50)})
    );
}
