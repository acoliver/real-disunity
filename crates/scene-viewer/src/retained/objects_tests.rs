use super::*;
use crate::RetainedUpdate;
use std::time::Duration;
use worldrunner_render_plan::{
    MaterialHandle, MeshHandle, SceneObject, SceneRotation, SceneTransform,
};

fn host() -> RetainedHost {
    RetainedHost::with_preparation_limit(
        crate::renderer::retained_tests::source(),
        crate::tests::options(),
        64,
    )
    .unwrap()
}

fn object_at(x: i64) -> SceneObject {
    SceneObject::new(
        SceneTransform {
            position_millimeters: [x, 0, 0],
            rotation: SceneRotation::IDENTITY,
            scale_millionths: 1_000_000,
        },
        MeshHandle::from_raw(0),
        MaterialHandle::from_raw(0),
    )
}

fn add(id: u64, x: i64) -> SceneObjectEdit {
    SceneObjectEdit::Add {
        id: ObjectId(id),
        object: object_at(x),
    }
}

fn table(host: &RetainedHost) -> Vec<SceneObject> {
    host.objects().map(|(_, object)| object).collect()
}

#[test]
fn object_edits_leave_the_materialized_objects_until_they_are_needed() {
    let mut host = host();
    let before = host.scene.clone();
    host.apply(RetainedUpdate::Objects(vec![add(5, 50), add(6, 60)]))
        .unwrap();
    host.apply(RetainedUpdate::Remove(ObjectId(5))).unwrap();
    assert_eq!(host.scene, before);
    host.materialize_objects().unwrap();
    assert_eq!(host.scene.geometry().objects(), table(&host).as_slice());
    assert_eq!(host.scene.resources(), before.resources());
    assert_eq!(host.scene.effects(), before.effects());
    let materialized = host.scene.clone();
    host.materialize_objects().unwrap();
    assert_eq!(host.scene, materialized);
}

#[test]
fn a_batch_may_remove_and_add_back_an_id_it_touches() {
    let mut host = host();
    host.apply(RetainedUpdate::Objects(vec![
        add(5, 50),
        SceneObjectEdit::Remove(ObjectId(5)),
        add(5, 51),
        SceneObjectEdit::Remove(ObjectId(0)),
        add(0, 1),
    ]))
    .unwrap();
    assert_eq!(
        host.object(ObjectId(5))
            .unwrap()
            .transform()
            .position_millimeters[0],
        51
    );
    assert_eq!(
        host.object(ObjectId(0))
            .unwrap()
            .transform()
            .position_millimeters[0],
        1
    );
}

#[test]
fn a_batch_rejects_edits_that_depend_on_its_own_removals() {
    let mut host = host();
    let before: Vec<_> = host.objects().collect();
    for (edits, message) in [
        (
            vec![
                SceneObjectEdit::Remove(ObjectId(0)),
                SceneObjectEdit::Remove(ObjectId(0)),
            ],
            "object edit 1: live driver failed: unknown object ID",
        ),
        (
            vec![
                SceneObjectEdit::Remove(ObjectId(0)),
                SceneObjectEdit::Replace {
                    id: ObjectId(0),
                    object: object_at(3),
                },
            ],
            "object edit 1: live driver failed: unknown object ID",
        ),
        (
            vec![add(5, 50), add(6, 60), add(5, 52)],
            "object edit 2: live driver failed: duplicate object ID",
        ),
    ] {
        let error = host.apply(RetainedUpdate::Objects(edits)).unwrap_err();
        assert!(
            matches!(&error, ViewerError::Driver(text) if text == message),
            "{error:?}"
        );
        assert_eq!(host.objects().collect::<Vec<_>>(), before);
    }
}

#[test]
fn a_rebuild_after_object_edits_prepares_the_edited_objects() {
    let mut host = host();
    host.apply(RetainedUpdate::Objects(vec![
        add(5, 50),
        SceneObjectEdit::Replace {
            id: ObjectId(0),
            object: object_at(7),
        },
    ]))
    .unwrap();
    host.apply(RetainedUpdate::Resources {
        resources: host.scene.resources().clone(),
        meshes: host.scene.geometry().meshes().to_vec(),
    })
    .unwrap();
    let preparation = host.readiness.queue.back().unwrap();
    let ids: Vec<_> = host.objects().map(|(id, _)| id).collect();
    assert_eq!(preparation.ids, ids);
    let prepared = preparation
        .scene
        .as_ref()
        .unwrap()
        .recv_timeout(Duration::from_secs(10))
        .unwrap()
        .unwrap();
    assert_eq!(
        prepared.scene().geometry().objects(),
        table(&host).as_slice()
    );
    assert_eq!(prepared.scene(), &host.scene);
}

#[test]
fn terrain_replacement_after_object_edits_keeps_the_edited_objects() {
    let mut host = host();
    host.apply(RetainedUpdate::Objects(vec![add(5, 50)]))
        .unwrap();
    host.apply(RetainedUpdate::Terrain(None)).unwrap();
    assert_eq!(host.scene.geometry().objects(), table(&host).as_slice());
}

/// Prints the cost of one stream-sized batch on a large scene. Run with `--ignored --nocapture`
/// in a release build; it asserts only that the batch applied.
#[test]
#[ignore = "timing probe for #444; prints, does not assert a duration"]
fn time_one_large_batch_on_a_large_scene() {
    let source = crate::renderer::retained_tests::source();
    let objects = (0..300_000).map(object_at).collect();
    let scene = source
        .with_objects_and_effects(objects, source.effects().cloned())
        .unwrap();
    let mut host =
        RetainedHost::with_preparation_limit(scene, crate::tests::options(), 64).unwrap();
    let mut edits: Vec<_> = (0..50_000)
        .map(|id| SceneObjectEdit::Remove(ObjectId(id)))
        .collect();
    edits.extend((0..50_000_i64).map(|x| add(1_000_000 + x.unsigned_abs(), x)));
    let start = std::time::Instant::now();
    host.apply(RetainedUpdate::Objects(edits)).unwrap();
    println!(
        "apply of 100000 edits on 300000 objects: {:?}",
        start.elapsed()
    );
}
