use super::*;
use worldrunner_render_plan::{MaterialHandle, MeshHandle, SceneRotation, SceneTransform};

fn host(limit: usize) -> RetainedHost {
    RetainedHost::with_preparation_limit(
        crate::renderer::retained_tests::source(),
        crate::tests::options(),
        limit,
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

fn x_of(host: &RetainedHost, id: u64) -> Option<i64> {
    host.object(ObjectId(id))
        .map(|object| object.transform().position_millimeters[0])
}

fn snapshot(host: &RetainedHost) -> Vec<(ObjectId, SceneObject)> {
    host.objects().collect()
}

#[test]
fn batch_adds_replaces_and_removes_in_one_generation() {
    let mut host = host(4);
    host.apply(RetainedUpdate::Add {
        id: ObjectId(7),
        object: object_at(7),
    })
    .unwrap();
    let generation = host.accepted_generation();
    host.apply(RetainedUpdate::Objects(vec![
        SceneObjectEdit::Add {
            id: ObjectId(9),
            object: object_at(90),
        },
        SceneObjectEdit::Replace {
            id: ObjectId(0),
            object: object_at(5),
        },
        SceneObjectEdit::Remove(ObjectId(7)),
    ]))
    .unwrap();
    assert_eq!(host.accepted_generation(), generation + 1);
    assert_eq!(x_of(&host, 9), Some(90));
    assert_eq!(x_of(&host, 0), Some(5));
    assert_eq!(x_of(&host, 7), None);
    let ids: Vec<_> = host.objects().map(|(id, _)| id).collect();
    assert_eq!(ids, [ObjectId(0), ObjectId(9)]);
    assert_eq!(host.scene.geometry().objects().len(), 2);
}

#[test]
fn invalid_edit_leaves_host_unchanged_and_names_its_index() {
    let mut host = host(4);
    let before = snapshot(&host);
    let generation = host.accepted_generation();
    let in_flight = host.preparations_in_flight();
    let error = host
        .apply(RetainedUpdate::Objects(vec![
            SceneObjectEdit::Replace {
                id: ObjectId(0),
                object: object_at(1),
            },
            SceneObjectEdit::Remove(ObjectId(99)),
            SceneObjectEdit::Add {
                id: ObjectId(5),
                object: object_at(2),
            },
        ]))
        .unwrap_err();
    assert!(error.to_string().contains("object edit 1"), "{error}");
    assert_eq!(snapshot(&host), before);
    assert_eq!(host.scene.geometry().objects().len(), before.len());
    assert_eq!(host.accepted_generation(), generation);
    assert_eq!(host.preparations_in_flight(), in_flight);
}

#[test]
fn edit_referencing_a_missing_resource_is_rejected_with_its_index() {
    let mut host = host(4);
    let before = snapshot(&host);
    let missing = SceneObject::new(
        *object_at(0).transform(),
        MeshHandle::from_raw(9),
        MaterialHandle::from_raw(0),
    );
    let error = host
        .apply(RetainedUpdate::Objects(vec![
            SceneObjectEdit::Replace {
                id: ObjectId(0),
                object: object_at(3),
            },
            SceneObjectEdit::Add {
                id: ObjectId(1),
                object: missing,
            },
        ]))
        .unwrap_err();
    assert!(error.to_string().contains("object edit 1"), "{error}");
    assert_eq!(snapshot(&host), before);
}

#[test]
fn thousand_replaces_use_one_preparation_slot() {
    let mut singles = host(2);
    for x in 0..2 {
        singles
            .apply(RetainedUpdate::Replace {
                id: ObjectId(0),
                object: object_at(x),
            })
            .unwrap();
    }
    assert!(matches!(
        singles.apply(RetainedUpdate::Replace {
            id: ObjectId(0),
            object: object_at(2),
        }),
        Err(ViewerError::BackPressure { limit: 2 })
    ));

    let mut batched = host(2);
    let edits = (0..1_000)
        .map(|x| SceneObjectEdit::Replace {
            id: ObjectId(0),
            object: object_at(x),
        })
        .collect();
    batched.apply(RetainedUpdate::Objects(edits)).unwrap();
    assert_eq!(batched.preparations_in_flight(), 1);
    assert_eq!(batched.accepted_generation(), 1);
    assert_eq!(x_of(&batched, 0), Some(999));
}

#[test]
fn edits_apply_in_order_so_the_last_edit_to_an_id_wins() {
    let mut host = host(4);
    host.apply(RetainedUpdate::Objects(vec![
        SceneObjectEdit::Add {
            id: ObjectId(3),
            object: object_at(30),
        },
        SceneObjectEdit::Replace {
            id: ObjectId(3),
            object: object_at(31),
        },
        SceneObjectEdit::Remove(ObjectId(0)),
        SceneObjectEdit::Add {
            id: ObjectId(0),
            object: object_at(8),
        },
        SceneObjectEdit::Replace {
            id: ObjectId(0),
            object: object_at(9),
        },
    ]))
    .unwrap();
    assert_eq!(x_of(&host, 3), Some(31));
    assert_eq!(x_of(&host, 0), Some(9));
}

#[test]
fn adding_an_id_twice_in_one_batch_rejects_the_batch() {
    let mut host = host(4);
    let before = snapshot(&host);
    let error = host
        .apply(RetainedUpdate::Objects(vec![
            SceneObjectEdit::Add {
                id: ObjectId(3),
                object: object_at(30),
            },
            SceneObjectEdit::Add {
                id: ObjectId(3),
                object: object_at(31),
            },
        ]))
        .unwrap_err();
    assert!(error.to_string().contains("object edit 1"), "{error}");
    assert_eq!(snapshot(&host), before);
}

#[test]
fn empty_batch_changes_nothing_and_takes_no_slot() {
    let mut host = host(1);
    host.apply(RetainedUpdate::Objects(Vec::new())).unwrap();
    assert_eq!(host.accepted_generation(), 0);
    assert_eq!(host.preparations_in_flight(), 0);
}
