use super::*;
use worldrunner_render_plan::{MaterialHandle, MeshHandle, SceneRotation, SceneTransform};

fn host() -> RetainedHost {
    RetainedHost::with_preparation_limit(
        crate::renderer::retained_tests::source(),
        crate::tests::options(),
        64,
    )
    .unwrap()
}

fn object_at(x_millimeters: i16) -> SceneObject {
    SceneObject::new(
        SceneTransform {
            position_millimeters: [i64::from(x_millimeters), 0, 0],
            rotation: SceneRotation::IDENTITY,
            scale_millionths: 1_000_000,
        },
        MeshHandle::from_raw(0),
        MaterialHandle::from_raw(0),
    )
}

/// Screen position of the point `x_millimeters` along the scene's X axis, at the origin's depth.
fn pixel(host: &RetainedHost, x_millimeters: i16) -> [f64; 2] {
    let point = host
        .view()
        .unwrap()
        .unwrap()
        .resolved_view()
        .world_to_screen(glam::Vec3::new(f32::from(x_millimeters) / 1000.0, 0.0, 0.0))
        .unwrap();
    [f64::from(point.x), f64::from(point.y)]
}

fn present(host: &mut RetainedHost) {
    let view = *host.view().unwrap().unwrap().resolved_view();
    host.present_pick(&view);
}

fn publish_all(host: &mut RetainedHost) {
    while !host.readiness.queue.is_empty() {
        host.publish_pick_front();
    }
}

fn hit(host: &RetainedHost, x_millimeters: i16) -> Option<(ObjectId, u64)> {
    host.pick(pixel(host, x_millimeters), |_| true)
        .map(|hit| (hit.hit.object, hit.generation))
}

#[test]
fn each_frame_sees_the_edits_published_up_to_it_and_no_later_ones() {
    let mut host = host();
    present(&mut host);
    assert_eq!(hit(&host, 0), Some((ObjectId(0), 0)));
    assert_eq!(hit(&host, 1500), None);
    for round in 1..=6_u64 {
        let x = if round % 2 == 1 { 1500 } else { 0 };
        let (from, to) = if x == 1500 { (0, 1500) } else { (1500, 0) };
        host.apply(RetainedUpdate::Replace {
            id: ObjectId(0),
            object: object_at(x),
        })
        .unwrap();
        host.publish_pick_front();
        let presented = round - 1;
        assert_eq!(hit(&host, from), Some((ObjectId(0), presented)));
        assert_eq!(hit(&host, to), None);
        present(&mut host);
        assert_eq!(hit(&host, to), Some((ObjectId(0), round)));
        assert_eq!(hit(&host, from), None);
    }
}

#[test]
fn many_published_edits_between_presentations_arrive_as_their_final_state() {
    let mut host = host();
    present(&mut host);
    host.apply(RetainedUpdate::Objects(vec![
        SceneObjectEdit::Add {
            id: ObjectId(5),
            object: object_at(-1500),
        },
        SceneObjectEdit::Add {
            id: ObjectId(6),
            object: object_at(1500),
        },
    ]))
    .unwrap();
    host.apply(RetainedUpdate::Remove(ObjectId(5))).unwrap();
    host.apply(RetainedUpdate::Objects(vec![
        SceneObjectEdit::Add {
            id: ObjectId(5),
            object: object_at(1500),
        },
        SceneObjectEdit::Remove(ObjectId(6)),
        SceneObjectEdit::Remove(ObjectId(0)),
    ]))
    .unwrap();
    publish_all(&mut host);
    assert_eq!(hit(&host, 0), Some((ObjectId(0), 0)));
    assert_eq!(hit(&host, 1500), None);
    present(&mut host);
    assert_eq!(hit(&host, 0), None);
    assert_eq!(hit(&host, -1500), None);
    assert_eq!(hit(&host, 1500), Some((ObjectId(5), 3)));
    host.apply(RetainedUpdate::Remove(ObjectId(5))).unwrap();
    host.apply(RetainedUpdate::Add {
        id: ObjectId(7),
        object: object_at(-1500),
    })
    .unwrap();
    publish_all(&mut host);
    present(&mut host);
    assert_eq!(hit(&host, 1500), None);
    assert_eq!(hit(&host, -1500), Some((ObjectId(7), 5)));
}

#[test]
fn presenting_twice_without_edits_keeps_the_same_objects() {
    let mut host = host();
    host.apply(RetainedUpdate::Replace {
        id: ObjectId(0),
        object: object_at(1500),
    })
    .unwrap();
    publish_all(&mut host);
    present(&mut host);
    present(&mut host);
    assert_eq!(hit(&host, 1500), Some((ObjectId(0), 1)));
    assert_eq!(hit(&host, 0), None);
}
