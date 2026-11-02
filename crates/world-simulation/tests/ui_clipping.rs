#![cfg(feature = "ui")]
//! Rect clipping of canvas renderer quads: the clip rectangle is in root canvas space, quads
//! outside it are dropped and quads across its edge are cut with their texture rectangle.
use std::sync::Arc;

use glam::{Quat, UVec2, Vec2};
use worldrunner_world_simulation::ui::{
    Canvas, CanvasRenderer, CanvasScale, RectTransform, UiGeometry, UiGraphic, UiVertex,
    extract_graphics,
};
use worldrunner_world_simulation::{
    ComponentDesc, LocalTransform, ObjectDesc, ObjectHandle, ParentSpace, World, WorldSettings,
};

const VIEWPORT: [u32; 2] = [800, 600];

fn vertex(x: f32, y: f32, u: f32, v: f32) -> UiVertex {
    UiVertex {
        position: [x, y, 0.],
        uv: [u, v],
        color: [1.; 4],
    }
}

/// A 100 by 100 quad around the origin showing `u` by `v` of the texture, in the vertex order
/// of the usual UI quad: lower left, upper left, upper right, lower right.
fn quad(u: [f32; 2], v: [f32; 2]) -> Arc<UiGeometry> {
    let vertices = vec![
        vertex(-50., -50., u[0], v[0]),
        vertex(-50., 50., u[0], v[1]),
        vertex(50., 50., u[1], v[1]),
        vertex(50., -50., u[1], v[0]),
    ];
    Arc::new(UiGeometry::new(vertices, vec![0, 1, 2, 2, 3, 0]).unwrap())
}

fn clipped(clip: Option<[f32; 4]>) -> CanvasRenderer {
    CanvasRenderer {
        geometry: Some(quad([0.25, 0.75], [0.5, 1.])),
        graphic_texture: 1 << 40,
        clip_rect: clip,
        ..CanvasRenderer::default()
    }
}

fn scene(scale: f32) -> (World, ObjectHandle) {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let canvas = world.create_object(ObjectDesc::default()).unwrap();
    let settings = Canvas {
        scale: CanvasScale::Constant(scale),
        ..Canvas::default()
    };
    world
        .add_component(
            canvas,
            ComponentDesc::RectTransform(RectTransform::default()),
        )
        .unwrap();
    world
        .add_component(canvas, ComponentDesc::Canvas(settings))
        .unwrap();
    (world, canvas)
}

fn element(world: &mut World, parent: ObjectHandle, at: Vec2) -> ObjectHandle {
    let object = world.create_object(ObjectDesc::default()).unwrap();
    let rect = RectTransform {
        anchored_position: at,
        ..RectTransform::default()
    };
    world
        .add_component(object, ComponentDesc::RectTransform(rect))
        .unwrap();
    world
        .set_parent(object, Some(parent), ParentSpace::KeepLocal)
        .unwrap();
    object
}

fn draw(world: &mut World, object: ObjectHandle, renderer: CanvasRenderer) {
    world
        .add_component(object, ComponentDesc::CanvasRenderer(renderer))
        .unwrap();
}

fn graphics(world: &World) -> Vec<UiGraphic> {
    let layout = world.ui_layout(UVec2::from(VIEWPORT)).unwrap();
    extract_graphics(world, &layout).unwrap()
}

/// The pixel bounds `[min x, min y, max x, max y]` of the only quad drawn.
fn bounds(world: &World) -> [f32; 4] {
    let found = graphics(world);
    assert_eq!(found[0].quads.len(), 1);
    let corners = found[0].quads[0].corners;
    let xs = corners.map(|c| c[0]);
    let ys = corners.map(|c| c[1]);
    [
        xs.into_iter().fold(f32::MAX, f32::min),
        ys.into_iter().fold(f32::MAX, f32::min),
        xs.into_iter().fold(f32::MIN, f32::max),
        ys.into_iter().fold(f32::MIN, f32::max),
    ]
}

fn uv_rect(world: &World) -> [f32; 4] {
    graphics(world)[0].quads[0].uv_rect
}

fn near(actual: &[f32], expected: &[f32]) {
    assert_eq!(actual.len(), expected.len());
    for (a, e) in actual.iter().zip(expected) {
        assert!((a - e).abs() < 1e-4, "{actual:?} is not {expected:?}");
    }
}

fn one_quad(clip: Option<[f32; 4]>) -> World {
    let (mut world, canvas) = scene(1.);
    let object = element(&mut world, canvas, Vec2::ZERO);
    draw(&mut world, object, clipped(clip));
    world
}

#[test]
fn a_quad_outside_the_clip_rectangle_is_dropped() {
    let world = one_quad(Some([100., -50., 100., 100.]));
    let found = graphics(&world);
    assert_eq!(found.len(), 1);
    assert!(found[0].quads.is_empty());
}

#[test]
fn a_quad_across_the_edge_is_cut_and_its_texture_rectangle_scaled() {
    let world = one_quad(Some([-200., -200., 200., 400.]));
    near(&bounds(&world), &[350., 250., 400., 350.]);
    near(&uv_rect(&world), &[0.25, 0., 0.5, 0.5]);
}

#[test]
fn a_quad_cut_on_both_axes_keeps_the_matching_part_of_the_texture() {
    let world = one_quad(Some([0., 0., 100., 100.]));
    near(&bounds(&world), &[400., 300., 450., 350.]);
    near(&uv_rect(&world), &[0.5, 0., 0.75, 0.25]);
}

#[test]
fn a_quad_inside_the_clip_rectangle_is_drawn_whole() {
    let world = one_quad(Some([-60., -60., 120., 120.]));
    near(&bounds(&world), &[350., 250., 450., 350.]);
    near(&uv_rect(&world), &[0.25, 0., 0.75, 0.5]);
}

#[test]
fn without_clipping_everything_is_drawn() {
    let world = one_quad(None);
    near(&bounds(&world), &[350., 250., 450., 350.]);
}

#[test]
fn an_empty_clip_rectangle_drops_everything() {
    let world = one_quad(Some([-10., -10., 0., 20.]));
    assert!(graphics(&world)[0].quads.is_empty());
}

#[test]
fn the_clip_rectangle_is_in_canvas_space_whatever_the_element_position() {
    let (mut world, canvas) = scene(1.);
    let object = element(&mut world, canvas, Vec2::new(100., 0.));
    draw(&mut world, object, clipped(Some([0., -100., 100., 200.])));
    near(&bounds(&world), &[450., 250., 500., 350.]);
    near(&uv_rect(&world), &[0.25, 0., 0.5, 0.5]);
}

#[test]
fn the_clip_rectangle_is_in_canvas_units_when_the_canvas_is_scaled() {
    let (mut world, canvas) = scene(2.);
    let object = element(&mut world, canvas, Vec2::ZERO);
    draw(
        &mut world,
        object,
        clipped(Some([-200., -200., 200., 400.])),
    );
    near(&bounds(&world), &[300., 200., 400., 400.]);
    near(&uv_rect(&world), &[0.25, 0., 0.5, 0.5]);
}

#[test]
fn texture_coordinates_that_run_backwards_are_cut_from_the_same_side() {
    let (mut world, canvas) = scene(1.);
    let object = element(&mut world, canvas, Vec2::ZERO);
    let mut renderer = clipped(Some([-200., -200., 200., 400.]));
    renderer.geometry = Some(quad([0.75, 0.25], [0.5, 1.]));
    draw(&mut world, object, renderer);
    near(&bounds(&world), &[350., 250., 400., 350.]);
    near(&uv_rect(&world), &[0.75, 0., 0.5, 0.5]);
}

#[test]
fn a_flipped_vertical_axis_is_cut_from_the_same_side() {
    let (mut world, canvas) = scene(1.);
    let object = element(&mut world, canvas, Vec2::ZERO);
    let mut renderer = clipped(Some([-200., 0., 400., 200.]));
    renderer.geometry = Some(quad([0.25, 0.75], [1., 0.5]));
    draw(&mut world, object, renderer);
    near(&bounds(&world), &[350., 300., 450., 350.]);
    near(&uv_rect(&world), &[0.25, 0.5, 0.75, 0.25]);
}

#[test]
fn a_quad_rotated_against_the_clip_rectangle_is_kept_whole() {
    let (mut world, canvas) = scene(1.);
    let object = element(&mut world, canvas, Vec2::ZERO);
    let turned = LocalTransform {
        rotation: Quat::from_rotation_z(0.6),
        ..LocalTransform::default()
    };
    world.set_local_transform(object, turned).unwrap();
    draw(
        &mut world,
        object,
        clipped(Some([-200., -200., 200., 400.])),
    );
    let whole = graphics(&world)[0].quads[0];
    assert!(whole.corners.iter().any(|c| c[0] > 400.));
    near(&whole.uv_rect, &[0.25, 0., 0.75, 0.5]);
}

#[test]
fn a_nested_canvas_measures_the_clip_rectangle_in_its_root_canvas() {
    let (mut world, canvas) = scene(1.);
    let nested = element(&mut world, canvas, Vec2::new(100., 0.));
    world
        .add_component(nested, ComponentDesc::Canvas(Canvas::default()))
        .unwrap();
    let object = element(&mut world, nested, Vec2::ZERO);
    draw(&mut world, object, clipped(Some([0., -100., 100., 200.])));
    near(&bounds(&world), &[450., 250., 500., 350.]);
    near(&uv_rect(&world), &[0.25, 0., 0.5, 0.5]);
}

#[test]
fn a_clip_rectangle_without_height_drops_everything() {
    let world = one_quad(Some([-10., -10., 20., 0.]));
    assert!(graphics(&world)[0].quads.is_empty());
}
