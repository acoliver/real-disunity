#![cfg(feature = "ui")]

use glam::{UVec2, Vec2, Vec3};
use worldrunner_world_simulation::ui::*;
use worldrunner_world_simulation::{
    ComponentDesc, DestroyWhen, LocalTransform, ObjectDesc, ObjectHandle, ParentSpace, World,
    WorldSettings,
};

fn world() -> World {
    World::new(WorldSettings::default()).unwrap()
}

fn rect(w: &mut World, parent: Option<ObjectHandle>, r: RectTransform) -> ObjectHandle {
    let o = w.create_object(ObjectDesc::default()).unwrap();
    w.add_component(o, ComponentDesc::RectTransform(r)).unwrap();
    w.set_parent(o, parent, ParentSpace::KeepLocal).unwrap();
    o
}

fn viewport() -> UVec2 {
    UVec2::new(800, 600)
}

fn stretched(inset: f32) -> RectTransform {
    RectTransform {
        anchor_min: Vec2::ZERO,
        anchor_max: Vec2::ONE,
        size_delta: Vec2::splat(-2. * inset),
        ..Default::default()
    }
}

fn screen_canvas(w: &mut World, parent: Option<ObjectHandle>) -> ObjectHandle {
    let root = rect(w, parent, RectTransform::default());
    w.add_component(root, ComponentDesc::Canvas(Canvas::default()))
        .unwrap();
    root
}

#[test]
fn a_rectangle_under_no_canvas_has_its_own_size() {
    let mut w = world();
    let alone = rect(
        &mut w,
        None,
        RectTransform {
            size_delta: Vec2::new(30., 20.),
            pivot: Vec2::new(0.25, 1.),
            ..Default::default()
        },
    );
    let child = rect(&mut w, Some(alone), stretched(5.));
    assert!(w.ui_layout(viewport()).unwrap().rect(alone).is_none());
    let extent = w.ui_extent(viewport(), alone).unwrap().unwrap();
    assert_eq!(extent.size, Vec2::new(30., 20.));
    assert_eq!(extent.pivot, Vec2::new(0.25, 1.));
    let inner = w.ui_extent(viewport(), child).unwrap().unwrap();
    assert_eq!(inner.size, Vec2::new(20., 10.));
}

#[test]
fn an_inactive_rectangle_keeps_its_size() {
    let mut w = world();
    let root = screen_canvas(&mut w, None);
    let panel = rect(&mut w, Some(root), stretched(100.));
    w.set_active(panel, false).unwrap();
    assert!(w.ui_layout(viewport()).unwrap().rect(panel).is_none());
    let extent = w.ui_extent(viewport(), panel).unwrap().unwrap();
    assert_eq!(extent.size, Vec2::new(600., 400.));
}

#[test]
fn an_object_without_a_rectangle_has_no_extent() {
    let mut w = world();
    let plain = w.create_object(ObjectDesc::default()).unwrap();
    assert_eq!(w.ui_extent(viewport(), plain).unwrap(), None);
}

#[test]
fn a_canvas_inside_a_canvas_is_laid_out_as_a_rectangle() {
    let mut w = world();
    let root = screen_canvas(&mut w, None);
    let nested = rect(&mut w, Some(root), stretched(50.));
    w.add_component(
        nested,
        ComponentDesc::Canvas(Canvas {
            sorting_order: 4,
            ..Default::default()
        }),
    )
    .unwrap();
    let label = rect(&mut w, Some(nested), stretched(10.));
    let out = w.ui_layout(viewport()).unwrap();
    let n = out.rect(nested).unwrap();
    assert_eq!(n.size, Vec2::new(700., 500.));
    assert_eq!(n.canvas, nested);
    assert_eq!(n.sorting_order, 4);
    let screen = n.screen_corners.unwrap();
    assert!(screen[0].abs_diff_eq(Vec2::new(50., 50.), 0.001));
    assert!(screen[2].abs_diff_eq(Vec2::new(750., 550.), 0.001));
    let l = out.rect(label).unwrap();
    assert_eq!(l.size, Vec2::new(680., 480.));
    assert_eq!(l.canvas, nested);
}

#[test]
fn a_world_canvas_inside_a_screen_canvas_is_drawn_on_the_screen() {
    let mut w = world();
    let root = screen_canvas(&mut w, None);
    let nested = rect(&mut w, Some(root), stretched(0.));
    w.add_component(
        nested,
        ComponentDesc::Canvas(Canvas {
            mode: CanvasMode::WorldSpace,
            ..Default::default()
        }),
    )
    .unwrap();
    let n = w
        .ui_layout(viewport())
        .unwrap()
        .rect(nested)
        .cloned()
        .unwrap();
    assert!(n.pixels_from_world.is_some());
    assert_eq!(n.size, Vec2::new(800., 600.));
}

#[test]
fn a_screen_canvas_under_a_plain_object_fills_the_screen() {
    let mut w = world();
    let holder = w
        .create_object(ObjectDesc {
            local_transform: LocalTransform {
                translation: Vec3::new(40., 30., 0.),
                scale: Vec3::splat(3.),
                ..Default::default()
            },
            ..Default::default()
        })
        .unwrap();
    let root = screen_canvas(&mut w, Some(holder));
    let out = w.ui_layout(viewport()).unwrap();
    let r = out.rect(root).unwrap();
    assert_eq!(r.size, Vec2::new(800., 600.));
    let screen = r.screen_corners.unwrap();
    assert!(screen[0].abs_diff_eq(Vec2::ZERO, 0.001));
    assert!(screen[2].abs_diff_eq(Vec2::new(800., 600.), 0.001));
}

#[test]
fn any_finite_anchors_and_pivot_are_a_rectangle() {
    let mut w = world();
    let root = screen_canvas(&mut w, None);
    let crossed = rect(
        &mut w,
        Some(root),
        RectTransform {
            anchor_min: Vec2::new(0.75, 1.5),
            anchor_max: Vec2::new(0.25, 1.0),
            pivot: Vec2::new(-1., 2.),
            size_delta: Vec2::ZERO,
            ..Default::default()
        },
    );
    let extent = w.ui_extent(viewport(), crossed).unwrap().unwrap();
    assert_eq!(extent.size, Vec2::new(-400., -300.));
    assert_eq!(extent.pivot, Vec2::new(-1., 2.));
    for bad in [Vec2::new(f32::NAN, 0.), Vec2::new(0., f32::INFINITY)] {
        let o = w.create_object(ObjectDesc::default()).unwrap();
        for r in [
            RectTransform {
                anchor_min: bad,
                ..Default::default()
            },
            RectTransform {
                anchor_max: bad,
                ..Default::default()
            },
            RectTransform {
                size_delta: bad,
                ..Default::default()
            },
            RectTransform {
                anchored_position: bad,
                ..Default::default()
            },
        ] {
            assert!(w.add_component(o, ComponentDesc::RectTransform(r)).is_err());
        }
    }
}

fn anchored(min: Vec2, max: Vec2, delta: Vec2, pivot: Vec2) -> RectTransform {
    RectTransform {
        anchor_min: min,
        anchor_max: max,
        size_delta: delta,
        pivot,
        anchored_position: Vec2::new(7., -3.),
    }
}

fn camera_canvas(w: &mut World, camera: ObjectHandle) -> ObjectHandle {
    let root = rect(w, None, RectTransform::default());
    w.add_component(
        root,
        ComponentDesc::Canvas(Canvas {
            mode: CanvasMode::ScreenSpaceCamera {
                camera,
                orthographic_size: 5.,
                plane_distance: 10.,
            },
            ..Default::default()
        }),
    )
    .unwrap();
    root
}

#[test]
fn each_extent_matches_the_full_layout() {
    let mut w = world();
    let root = screen_canvas(&mut w, None);
    let panel = rect(
        &mut w,
        Some(root),
        anchored(
            Vec2::new(0.1, 0.2),
            Vec2::new(0.6, 0.9),
            Vec2::new(-4., 6.),
            Vec2::new(0.3, 0.7),
        ),
    );
    let nested = rect(&mut w, Some(panel), stretched(3.));
    w.add_component(nested, ComponentDesc::Canvas(Canvas::default()))
        .unwrap();
    let label = rect(
        &mut w,
        Some(nested),
        anchored(
            Vec2::ZERO,
            Vec2::new(0.5, 1.),
            Vec2::new(10., -2.),
            Vec2::splat(0.5),
        ),
    );
    let camera = w.create_object(ObjectDesc::default()).unwrap();
    let other = camera_canvas(&mut w, camera);
    let item = rect(
        &mut w,
        Some(other),
        anchored(Vec2::splat(0.25), Vec2::splat(0.75), Vec2::ZERO, Vec2::ONE),
    );
    let layout = w.ui_layout(viewport()).unwrap();
    for object in [root, panel, nested, label, other, item] {
        let drawn = layout.rect(object).unwrap();
        let extent = w.ui_extent(viewport(), object).unwrap().unwrap();
        assert_eq!(
            (extent.size, extent.pivot),
            (drawn.size, drawn.pivot),
            "{object:?}"
        );
    }
}

#[test]
fn an_extent_through_a_camera_inside_a_canvas_fails_like_the_layout() {
    let mut w = world();
    let holder = screen_canvas(&mut w, None);
    let camera = w.create_object(ObjectDesc::default()).unwrap();
    w.set_parent(camera, Some(holder), ParentSpace::KeepLocal)
        .unwrap();
    let root = camera_canvas(&mut w, camera);
    let item = rect(&mut w, Some(root), stretched(1.));
    assert!(w.ui_layout(viewport()).is_err());
    assert!(w.ui_extent(viewport(), item).is_err());
}

#[test]
fn an_object_without_a_rectangle_above_rectangles_has_no_extent() {
    let mut w = world();
    let holder = w.create_object(ObjectDesc::default()).unwrap();
    let inner = rect(&mut w, Some(holder), RectTransform::default());
    assert!(w.ui_extent(viewport(), holder).unwrap().is_none());
    assert!(w.ui_extent(viewport(), inner).unwrap().is_some());
}

#[test]
fn a_canvas_awaiting_destruction_no_longer_lays_out_its_children() {
    let mut w = world();
    let root = rect(&mut w, None, RectTransform::default());
    let canvas = w
        .add_component(root, ComponentDesc::Canvas(Canvas::default()))
        .unwrap();
    let child = rect(&mut w, Some(root), stretched(10.));
    let before = w.ui_extent(viewport(), child).unwrap().unwrap();
    assert_eq!(before.size, Vec2::new(780., 580.));
    w.destroy_component(canvas).unwrap();
    let after = w.ui_extent(viewport(), child).unwrap().unwrap();
    assert_eq!(after.size, Vec2::new(80., 80.));
}

#[test]
fn an_object_awaiting_destruction_has_no_extent() {
    let mut w = world();
    let root = screen_canvas(&mut w, None);
    let child = rect(&mut w, Some(root), stretched(10.));
    w.destroy(root, DestroyWhen::EndOfFrame).unwrap();
    assert!(w.ui_extent(viewport(), child).unwrap().is_none());
}
