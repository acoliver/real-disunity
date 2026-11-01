#![cfg(feature = "ui")]
//! The quads of canvas renderer meshes: corners, texture rectangle and colour, what is left out,
//! and the order they reach the frame in.
use std::sync::Arc;

use glam::UVec2;
use worldrunner_world_simulation::presentation::extraction::{SpriteTextures, extract_world};
use worldrunner_world_simulation::presentation::{Camera, Projection};
use worldrunner_world_simulation::ui::{
    Canvas, CanvasGroup, CanvasMode, CanvasRenderer, RectTransform, UiGeometry, UiGraphic,
    UiVertex, append_graphics, extract_graphics,
};
use worldrunner_world_simulation::{
    ComponentDesc, ObjectDesc, ObjectHandle, ParentSpace, UiError, World, WorldSettings,
};

const VIEWPORT: [u32; 2] = [800, 600];

fn vertex(x: f32, y: f32, u: f32, v: f32) -> UiVertex {
    UiVertex {
        position: [x, y, 0.],
        uv: [u, v],
        color: [1.; 4],
    }
}

/// A 100 by 100 quad around the origin with the sub-rectangle `u0..u1`, `v0..v1` of the texture,
/// in the vertex order and triangles of the usual UI quad: lower left, upper left, upper right,
/// lower right.
fn quad_vertices(u: [f32; 2], v: [f32; 2]) -> Vec<UiVertex> {
    vec![
        vertex(-50., -50., u[0], v[0]),
        vertex(-50., 50., u[0], v[1]),
        vertex(50., 50., u[1], v[1]),
        vertex(50., -50., u[1], v[0]),
    ]
}

fn geometry(vertices: Vec<UiVertex>, indices: Vec<u32>) -> Option<Arc<UiGeometry>> {
    Some(Arc::new(UiGeometry::new(vertices, indices).unwrap()))
}

fn unit_quad() -> Option<Arc<UiGeometry>> {
    geometry(
        quad_vertices([0.25, 0.75], [0.5, 1.]),
        vec![0, 1, 2, 2, 3, 0],
    )
}

fn renderer(geometry: Option<Arc<UiGeometry>>) -> CanvasRenderer {
    CanvasRenderer {
        geometry,
        graphic_texture: 1 << 40,
        ..CanvasRenderer::default()
    }
}

fn canvas_world(mode: CanvasMode) -> (World, ObjectHandle) {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let canvas = world.create_object(ObjectDesc::default()).unwrap();
    world
        .add_component(
            canvas,
            ComponentDesc::RectTransform(RectTransform::default()),
        )
        .unwrap();
    let settings = Canvas {
        mode,
        ..Canvas::default()
    };
    world
        .add_component(canvas, ComponentDesc::Canvas(settings))
        .unwrap();
    (world, canvas)
}

fn element(world: &mut World, parent: ObjectHandle) -> ObjectHandle {
    let object = world.create_object(ObjectDesc::default()).unwrap();
    world
        .add_component(
            object,
            ComponentDesc::RectTransform(RectTransform::default()),
        )
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

fn near(actual: &[f32], expected: &[f32]) {
    assert_eq!(actual.len(), expected.len());
    for (a, e) in actual.iter().zip(expected) {
        assert!((a - e).abs() < 1e-4, "{actual:?} is not {expected:?}");
    }
}

#[test]
fn a_quad_on_an_overlay_canvas_has_pixel_corners_texture_rectangle_and_colour() {
    let (mut world, canvas) = canvas_world(CanvasMode::ScreenSpaceOverlay);
    let object = element(&mut world, canvas);
    let mut tinted = renderer(unit_quad());
    tinted.color = [1., 0.5, 0.25, 0.8];
    tinted.alpha = 0.5;
    draw(&mut world, object, tinted);
    let found = graphics(&world);
    assert_eq!(found.len(), 1);
    let graphic = &found[0];
    assert!(graphic.overlay);
    assert_eq!(graphic.object, object);
    assert_eq!(graphic.texture, Some(1 << 40));
    assert_eq!(graphic.left_out_triangles, 0);
    assert_eq!(graphic.quads.len(), 1);
    let quad = graphic.quads[0];
    let centre = [400., 300.];
    let expected = [[-50., -50.], [50., -50.], [50., 50.], [-50., 50.]];
    for (corner, offset) in quad.corners.iter().zip(expected) {
        near(corner, &[centre[0] + offset[0], centre[1] + offset[1], 0.]);
    }
    near(&quad.uv_rect, &[0.25, 0., 0.75, 0.5]);
    near(&quad.color, &[1., 0.5, 0.25, 0.4]);
}

#[test]
fn texture_coordinates_that_run_backwards_flip_the_rectangle() {
    let (mut world, canvas) = canvas_world(CanvasMode::ScreenSpaceOverlay);
    let object = element(&mut world, canvas);
    let backwards = geometry(
        quad_vertices([0.75, 0.25], [1., 0.5]),
        vec![0, 1, 2, 2, 3, 0],
    );
    draw(&mut world, object, renderer(backwards));
    let quad = graphics(&world)[0].quads[0];
    near(&quad.uv_rect, &[0.75, 0.5, 0.25, 0.]);
    near(&quad.corners[0], &[350., 250., 0.]);
}

#[test]
fn the_other_winding_makes_the_same_quad() {
    let (mut world, canvas) = canvas_world(CanvasMode::ScreenSpaceOverlay);
    let object = element(&mut world, canvas);
    let other = geometry(
        quad_vertices([0.25, 0.75], [0.5, 1.]),
        vec![0, 1, 2, 0, 2, 3],
    );
    draw(&mut world, object, renderer(other));
    let found = graphics(&world);
    assert_eq!(found[0].left_out_triangles, 0);
    let quad = found[0].quads[0];
    near(&quad.uv_rect, &[0.25, 0., 0.75, 0.5]);
    near(&quad.corners[2], &[450., 350., 0.]);
}

#[test]
fn vertices_listed_lower_left_lower_right_upper_right_upper_left_make_the_same_quad() {
    let (mut world, canvas) = canvas_world(CanvasMode::ScreenSpaceOverlay);
    let object = element(&mut world, canvas);
    let vertices = vec![
        vertex(-50., -50., 0.25, 0.5),
        vertex(50., -50., 0.75, 0.5),
        vertex(50., 50., 0.75, 1.),
        vertex(-50., 50., 0.25, 1.),
    ];
    let listed = geometry(vertices, vec![0, 1, 2, 2, 3, 0]);
    draw(&mut world, object, renderer(listed));
    let found = graphics(&world);
    assert_eq!(found[0].left_out_triangles, 0);
    let quad = found[0].quads[0];
    near(&quad.uv_rect, &[0.25, 0., 0.75, 0.5]);
    near(&quad.corners[0], &[350., 250., 0.]);
    near(&quad.corners[2], &[450., 350., 0.]);
}

#[test]
fn triangles_that_make_no_quad_are_counted_and_not_drawn() {
    let (mut world, canvas) = canvas_world(CanvasMode::ScreenSpaceOverlay);
    let object = element(&mut world, canvas);
    let mut vertices = quad_vertices([0., 1.], [0., 1.]);
    vertices.push(vertex(0., 80., 0.5, 0.5));
    let indices = vec![0, 1, 2, 2, 3, 0, 1, 4, 2];
    draw(&mut world, object, renderer(geometry(vertices, indices)));
    let found = graphics(&world);
    assert_eq!(found[0].quads.len(), 1);
    assert_eq!(found[0].left_out_triangles, 1);
    let slanted = vec![
        vertex(-50., -50., 0., 0.),
        vertex(-50., 50., 0.2, 1.),
        vertex(50., 50., 1., 1.),
        vertex(50., -50., 1., 0.),
    ];
    let other = element(&mut world, canvas);
    draw(
        &mut world,
        other,
        renderer(geometry(slanted, vec![0, 1, 2, 2, 3, 0])),
    );
    let found = graphics(&world);
    assert_eq!(found[1].quads.len(), 0);
    assert_eq!(found[1].left_out_triangles, 2);
}

#[test]
fn a_culled_renderer_a_missing_mesh_and_an_inactive_object_draw_nothing() {
    let (mut world, canvas) = canvas_world(CanvasMode::ScreenSpaceOverlay);
    let culled = element(&mut world, canvas);
    let mut hidden = renderer(unit_quad());
    hidden.flags.cull = true;
    draw(&mut world, culled, hidden);
    let bare = element(&mut world, canvas);
    draw(&mut world, bare, renderer(None));
    let inactive = element(&mut world, canvas);
    draw(&mut world, inactive, renderer(unit_quad()));
    world.set_active(inactive, false).unwrap();
    assert!(graphics(&world).is_empty());
    world.set_active(inactive, true).unwrap();
    assert_eq!(graphics(&world).len(), 1);
}

#[test]
fn inherited_alpha_scales_the_colour_and_zero_alpha_draws_nothing() {
    let (mut world, canvas) = canvas_world(CanvasMode::ScreenSpaceOverlay);
    let group = ComponentDesc::CanvasGroup(CanvasGroup {
        alpha: 0.5,
        ..CanvasGroup::default()
    });
    world.add_component(canvas, group).unwrap();
    let object = element(&mut world, canvas);
    let mut half = renderer(unit_quad());
    half.alpha = 0.5;
    draw(&mut world, object, half);
    near(&graphics(&world)[0].quads[0].color, &[1., 1., 1., 0.25]);
    let clear = element(&mut world, canvas);
    let mut gone = renderer(unit_quad());
    gone.alpha = 0.;
    draw(&mut world, clear, gone);
    assert_eq!(graphics(&world).len(), 1);
}

#[test]
fn a_vertex_colour_multiplies_the_renderer_colour() {
    let (mut world, canvas) = canvas_world(CanvasMode::ScreenSpaceOverlay);
    let object = element(&mut world, canvas);
    let mut vertices = quad_vertices([0., 1.], [0., 1.]);
    vertices[0].color = [0.5, 1., 0., 1.];
    vertices[1].color = [0., 0., 0., 0.];
    draw(
        &mut world,
        object,
        renderer(geometry(vertices, vec![0, 1, 2, 2, 3, 0])),
    );
    near(&graphics(&world)[0].quads[0].color, &[0.5, 1., 0., 1.]);
}

#[test]
fn quads_follow_hierarchy_order_whatever_order_the_renderers_were_added() {
    let (mut world, canvas) = canvas_world(CanvasMode::ScreenSpaceOverlay);
    let first = element(&mut world, canvas);
    let second = element(&mut world, canvas);
    let inside_first = element(&mut world, first);
    for object in [second, inside_first, first] {
        draw(&mut world, object, renderer(unit_quad()));
    }
    let order: Vec<_> = graphics(&world).iter().map(|g| g.object).collect();
    assert_eq!(order, [first, inside_first, second]);
}

#[test]
fn appended_quads_keep_hierarchy_order_for_equal_sorting_keys() {
    let (mut world, canvas) = canvas_world(CanvasMode::ScreenSpaceOverlay);
    world
        .add_component(canvas, ComponentDesc::Camera(Camera::default()))
        .unwrap();
    let first = element(&mut world, canvas);
    let second = element(&mut world, canvas);
    let mut plain = renderer(unit_quad());
    plain.graphic_texture = 0;
    draw(&mut world, second, renderer(unit_quad()));
    draw(&mut world, first, plain);
    let mut plan = extract_world(&world, &SpriteTextures::default(), VIEWPORT).unwrap();
    append_graphics(&mut plan, graphics(&world)).unwrap();
    let textures: Vec<_> = plan.quads.iter().map(|q| q.texture).collect();
    assert_eq!(textures, [None, Some(1 << 40)]);
    assert!(plan.quads.iter().all(|q| q.overlay));
    near(&plan.quads[0].uv_rect, &[0.25, 0., 0.75, 0.5]);
}

#[test]
fn a_world_canvas_on_a_layer_the_camera_leaves_out_draws_nothing() {
    let (mut world, canvas) = canvas_world(CanvasMode::WorldSpace);
    world.set_layer(canvas, 5).unwrap();
    let camera = world.create_object(ObjectDesc::default()).unwrap();
    let masked = Camera {
        culling_mask: 1,
        ..Camera::default()
    };
    world
        .add_component(camera, ComponentDesc::Camera(masked))
        .unwrap();
    let object = element(&mut world, canvas);
    world.set_layer(object, 5).unwrap();
    draw(&mut world, object, renderer(unit_quad()));
    assert!(graphics(&world).is_empty());
}

#[test]
fn a_mesh_is_checked_when_it_is_built() {
    let two = vec![vertex(0., 0., 0., 0.), vertex(1., 0., 1., 0.)];
    assert_eq!(
        UiGeometry::new(two.clone(), vec![0, 1]),
        Err(UiError::InvalidCanvasRenderer),
        "indices come in threes"
    );
    assert_eq!(
        UiGeometry::new(two.clone(), vec![0, 1, 2]),
        Err(UiError::InvalidCanvasRenderer),
        "an index past the vertices"
    );
    let mut nan = two.clone();
    nan[1].position[0] = f32::NAN;
    assert!(UiGeometry::new(nan, vec![0, 1, 0]).is_err());
    let mut infinite = two.clone();
    infinite[0].uv[1] = f32::INFINITY;
    assert!(UiGeometry::new(infinite, vec![0, 1, 0]).is_err());
    let mut colour = two.clone();
    colour[0].color[2] = f32::NAN;
    assert!(UiGeometry::new(colour, vec![0, 1, 0]).is_err());
    assert!(UiGeometry::new(two, vec![0, 1, 1]).is_ok());
    assert!(UiGeometry::new(Vec::new(), Vec::new()).is_ok());
}

#[test]
fn overlay_quads_under_a_perspective_camera_lie_inside_its_view() {
    let (mut world, canvas) = canvas_world(CanvasMode::ScreenSpaceOverlay);
    let perspective = Camera {
        projection: Projection::Perspective,
        near: 0.3,
        far: 1000.,
        ..Camera::default()
    };
    world
        .add_component(canvas, ComponentDesc::Camera(perspective))
        .unwrap();
    let object = element(&mut world, canvas);
    draw(&mut world, object, renderer(unit_quad()));
    let mut plan = extract_world(&world, &SpriteTextures::default(), VIEWPORT).unwrap();
    append_graphics(&mut plan, graphics(&world)).unwrap();
    let view = plan.camera.expect("a presented camera");
    assert_eq!(plan.quads.len(), 1);
    for corner in plan.quads[0].corners {
        assert!(
            view.contains_point(glam::Vec3::from(corner)),
            "{corner:?} is clipped"
        );
    }
}

#[test]
fn a_nested_canvas_under_a_camera_canvas_draws_at_its_screen_pixels() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let camera = world.create_object(ObjectDesc::default()).unwrap();
    let root = world.create_object(ObjectDesc::default()).unwrap();
    world
        .add_component(root, ComponentDesc::RectTransform(RectTransform::default()))
        .unwrap();
    let settings = Canvas {
        mode: CanvasMode::ScreenSpaceCamera {
            camera,
            orthographic_size: 3.,
            plane_distance: 10.,
        },
        ..Canvas::default()
    };
    world
        .add_component(root, ComponentDesc::Canvas(settings))
        .unwrap();
    let nested = element(&mut world, root);
    world
        .add_component(nested, ComponentDesc::Canvas(Canvas::default()))
        .unwrap();
    let object = element(&mut world, nested);
    draw(&mut world, object, renderer(unit_quad()));
    let found = graphics(&world);
    assert_eq!(found.len(), 1);
    assert!(found[0].overlay, "a camera canvas is drawn on the screen");
    let expected = [[350., 250.], [450., 250.], [450., 350.], [350., 350.]];
    for (corner, pixel) in found[0].quads[0].corners.iter().zip(expected) {
        near(&corner[..2], &pixel);
    }
}
