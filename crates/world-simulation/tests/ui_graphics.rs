#![cfg(feature = "ui")]
//! The quads of canvas renderer meshes: corners, texture rectangle and colour, what is left out,
//! and the order they reach the frame in.
use std::sync::Arc;

use glam::UVec2;
use worldrunner_world_simulation::graphics::{
    MaterialEdit, MaterialId, PropertyValue, RenderTextureId, ShaderSource, WorldRenderTexture,
    WorldSampler, WorldShader,
};
use worldrunner_world_simulation::presentation::extraction::extract_world;
use worldrunner_world_simulation::presentation::{Camera, Projection};
use worldrunner_world_simulation::ui::{
    Canvas, CanvasGroup, CanvasMode, CanvasRenderer, FontBook, RectTransform, UiGeometry,
    UiGraphic, UiLimits, UiMaterial, UiTexture, UiVertex, append_graphics, extract_graphics,
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

fn geometry(vertices: Vec<UiVertex>, indices: Vec<u32>) -> Arc<UiGeometry> {
    Arc::new(UiGeometry::new(vertices, indices).unwrap())
}

fn unit_quad() -> Arc<UiGeometry> {
    geometry(
        quad_vertices([0.25, 0.75], [0.5, 1.]),
        vec![0, 1, 2, 2, 3, 0],
    )
}

fn renderer(geometry: impl Into<Option<Arc<UiGeometry>>>) -> CanvasRenderer {
    CanvasRenderer {
        geometry: geometry.into(),
        graphic_texture: Some(UiTexture::Image(1 << 40)),
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
fn detached_graphics_keep_layout_draw_order_texture_identity_and_clipping() {
    use worldrunner_world_simulation::ui::{
        FontBook, UiLimits, contribution::extract_ui, presentation::UiPresentation,
    };
    let (mut world, canvas) = canvas_world(CanvasMode::ScreenSpaceOverlay);
    let child = element(&mut world, canvas);
    let parent = element(&mut world, canvas);
    world
        .set_parent(child, Some(parent), ParentSpace::KeepLocal)
        .unwrap();
    draw(&mut world, parent, renderer(unit_quad()));
    draw(
        &mut world,
        child,
        CanvasRenderer {
            clip_rect: Some([-8., -8., 16., 16.]),
            ..renderer(unit_quad())
        },
    );
    let fonts = FontBook::new(UiLimits::default()).unwrap();
    let plan = extract_ui(&world, &fonts, VIEWPORT).unwrap();
    let snapshot = UiPresentation::from_plan(&plan);
    assert_eq!(snapshot.draws.len(), 2);
    for (draw, object) in snapshot.draws.iter().zip([parent, child]) {
        assert_eq!(
            (draw.sorting_layer, draw.sorting_order, draw.hierarchy_order),
            plan.layout.rect(object).unwrap().draw_order()
        );
        assert_eq!(draw.texture, Some(UiTexture::Image(1 << 40)));
    }
    assert_eq!(snapshot.draws[1].clip_rect, [392, 292, 16, 16]);
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
    assert_eq!(graphic.texture, Some(UiTexture::Image(1 << 40)));
    assert!(graphic.triangles.is_empty());
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
    assert!(found[0].triangles.is_empty());
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
    assert!(found[0].triangles.is_empty());
    let quad = found[0].quads[0];
    near(&quad.uv_rect, &[0.25, 0., 0.75, 0.5]);
    near(&quad.corners[0], &[350., 250., 0.]);
    near(&quad.corners[2], &[450., 350., 0.]);
}

#[test]
fn triangles_that_make_no_quad_are_not_omitted() {
    let (mut world, canvas) = canvas_world(CanvasMode::ScreenSpaceOverlay);
    let object = element(&mut world, canvas);
    let mut vertices = quad_vertices([0., 1.], [0., 1.]);
    vertices.push(vertex(0., 80., 0.5, 0.5));
    let indices = vec![0, 1, 2, 2, 3, 0, 1, 4, 2];
    draw(&mut world, object, renderer(geometry(vertices, indices)));
    let found = graphics(&world);
    assert_eq!(found[0].quads.len(), 1);
    assert_eq!(found[0].triangles.len(), 1);
    let fonts = FontBook::new(UiLimits::default()).unwrap();
    let plan = worldrunner_world_simulation::ui::contribution::extract_ui(&world, &fonts, VIEWPORT)
        .unwrap();
    let detached = worldrunner_world_simulation::ui::presentation::UiPresentation::from_plan(&plan);
    let uv = detached
        .draws
        .iter()
        .find_map(|draw| draw.triangle_uv)
        .unwrap();
    assert_eq!(
        uv,
        [[0., 0.], [0.5, 0.5], [1., 0.]],
        "triangle UVs use top-row texture coordinates"
    );
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
    assert_eq!(found[1].triangles.len(), 2);
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
    let mut untextured = renderer(unit_quad());
    untextured.graphic_texture = None;
    draw(&mut world, second, renderer(unit_quad()));
    draw(&mut world, first, untextured);
    let mut plan = extract_world(&world, VIEWPORT).unwrap();
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
    let mut plan = extract_world(&world, VIEWPORT).unwrap();
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

/// A canvas the game added at run time and never gave a mode, below a plain object it placed in
/// the world, with a quad on one element.
fn runtime_canvas_at(
    world: &mut World,
    translation: glam::Vec3,
    scale: f32,
) -> (ObjectHandle, ObjectHandle) {
    let parent = world.create_object(ObjectDesc::default()).unwrap();
    world
        .set_local_transform(
            parent,
            worldrunner_world_simulation::LocalTransform {
                translation,
                scale: glam::Vec3::splat(scale),
                ..Default::default()
            },
        )
        .unwrap();
    let canvas = element(world, parent);
    world
        .add_component(canvas, ComponentDesc::Canvas(Canvas::default()))
        .unwrap();
    let object = element(world, canvas);
    draw(world, object, renderer(unit_quad()));
    (canvas, object)
}

fn camera_object(world: &mut World) {
    let camera = world.create_object(ObjectDesc::default()).unwrap();
    world
        .add_component(camera, ComponentDesc::Camera(Camera::default()))
        .unwrap();
}

#[test]
fn a_canvas_created_without_a_mode_draws_at_its_place_in_the_world() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    camera_object(&mut world);
    let at = glam::Vec3::new(25.98, 0., -17.89);
    let (canvas, object) = runtime_canvas_at(&mut world, at, 0.059);
    let found = graphics(&world);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].object, object);
    assert_eq!(found[0].canvas, canvas);
    assert!(!found[0].overlay, "the canvas is drawn in the world");
    let expected = [[-50., -50.], [50., -50.], [50., 50.], [-50., 50.]];
    for (corner, offset) in found[0].quads[0].corners.iter().zip(expected) {
        near(
            corner,
            &[at.x + 0.059 * offset[0], at.y + 0.059 * offset[1], at.z],
        );
    }
}

fn material_named(world: &mut World, name: &str, with_source: bool) -> MaterialId {
    let source = with_source.then(|| ShaderSource {
        file: "shaders.bundle".into(),
        path_id: 12,
    });
    let shader = world
        .create_shader(WorldShader {
            name: name.into(),
            source,
        })
        .unwrap();
    world.create_material(shader).unwrap()
}

/// One overlay element whose renderer draws the unit quad with a game material as its slot 0.
fn element_with_material() -> (World, ObjectHandle, MaterialId) {
    let (mut world, canvas) = canvas_world(CanvasMode::ScreenSpaceOverlay);
    let own = material_named(&mut world, "Game/Silhouette", true);
    let object = element(&mut world, canvas);
    let drawn = CanvasRenderer {
        material_count: 1,
        world_material: Some(own),
        ..renderer(unit_quad())
    };
    draw(&mut world, object, drawn);
    (world, object, own)
}

#[test]
fn a_graphic_names_the_world_material_of_its_renderer() {
    use worldrunner_world_simulation::ui::{
        contribution::extract_ui, presentation::UiPresentation,
    };
    let (world, _, material) = element_with_material();
    let expected = UiMaterial {
        id: material,
        generation: world.material(material).unwrap().generation,
    };
    assert_eq!(graphics(&world)[0].material, Some(expected));
    let fonts = FontBook::new(UiLimits::default()).unwrap();
    let plan = extract_ui(&world, &fonts, VIEWPORT).unwrap();
    let draws = UiPresentation::from_plan(&plan).draws;
    assert_eq!(draws.len(), 1);
    assert_eq!(draws[0].material, Some(expected));
}

#[test]
fn a_material_on_an_engine_supplied_shader_is_drawn_natively() {
    for (name, with_source) in [("Game/Plain", false), ("UI/Default", true)] {
        let (mut world, canvas) = canvas_world(CanvasMode::ScreenSpaceOverlay);
        let engine = material_named(&mut world, name, with_source);
        let object = element(&mut world, canvas);
        let drawn = CanvasRenderer {
            material_count: 1,
            world_material: Some(engine),
            ..renderer(unit_quad())
        };
        draw(&mut world, object, drawn);
        assert_eq!(graphics(&world)[0].material, None, "{name}");
    }
}

fn render_texture(world: &mut World) -> RenderTextureId {
    world
        .create_render_texture(WorldRenderTexture {
            width: 64,
            height: 32,
            format: 8,
            depth_bits: 0,
            mip_count: 1,
            srgb: false,
            dimension: 2,
            volume_depth: 1,
            auto_generate_mips: false,
            sampler: WorldSampler::default(),
            generation: 0,
        })
        .unwrap()
}

fn over_target(target: RenderTextureId) -> CanvasRenderer {
    CanvasRenderer {
        graphic_texture: Some(UiTexture::Target(target)),
        ..renderer(unit_quad())
    }
}

#[test]
fn canvases_created_without_a_mode_under_different_parents_draw_in_different_places() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    camera_object(&mut world);
    let first = glam::Vec3::new(10., 0., 4.);
    let second = glam::Vec3::new(-6., 2., 9.);
    let (_, near_object) = runtime_canvas_at(&mut world, first, 0.1);
    let (_, far_object) = runtime_canvas_at(&mut world, second, 0.1);
    let found = graphics(&world);
    assert_eq!(found.len(), 2);
    let centre = |object: ObjectHandle| {
        let quad = found.iter().find(|g| g.object == object).unwrap().quads[0];
        let sum = quad
            .corners
            .iter()
            .fold(glam::Vec3::ZERO, |sum, c| sum + glam::Vec3::from(*c));
        sum / 4.
    };
    near(&centre(near_object).to_array(), &first.to_array());
    near(&centre(far_object).to_array(), &second.to_array());
}

#[test]
fn a_destroyed_material_is_no_material() {
    let (mut world, _, material) = element_with_material();
    assert!(graphics(&world)[0].material.is_some());
    world.destroy_material(material).unwrap();
    assert_eq!(graphics(&world)[0].material, None);
}

#[test]
fn a_property_change_gives_the_draw_a_new_material_generation() {
    let (mut world, _, material) = element_with_material();
    let before = graphics(&world)[0].material.unwrap();
    let edit = MaterialEdit::Property(PropertyValue::Float("_Cutoff".into(), 0.5));
    world.edit_material(material, edit).unwrap();
    let after = graphics(&world)[0].material.unwrap();
    assert_eq!(after.id, before.id);
    assert_ne!(after.generation, before.generation);
}

#[test]
fn a_2d_graphic_with_a_game_material_is_left_out() {
    let (mut world, canvas) = canvas_world(CanvasMode::ScreenSpaceOverlay);
    world
        .add_component(canvas, ComponentDesc::Camera(Camera::default()))
        .unwrap();
    let own = material_named(&mut world, "Game/Silhouette", true);
    let (engine_look, game_look) = (element(&mut world, canvas), element(&mut world, canvas));
    draw(&mut world, engine_look, renderer(unit_quad()));
    let drawn = CanvasRenderer {
        material_count: 1,
        world_material: Some(own),
        ..renderer(unit_quad())
    };
    draw(&mut world, game_look, drawn);
    assert_eq!(graphics(&world).len(), 2);
    let mut plan = extract_world(&world, VIEWPORT).unwrap();
    append_graphics(&mut plan, graphics(&world)).unwrap();
    assert_eq!(plan.quads.len(), 1, "only the engine's own look is drawn");
}

#[test]
fn a_renderer_over_a_render_texture_draws_with_that_target() {
    use worldrunner_world_simulation::ui::{
        contribution::extract_ui, presentation::UiPresentation,
    };
    let (mut world, canvas) = canvas_world(CanvasMode::ScreenSpaceOverlay);
    let target = render_texture(&mut world);
    let object = element(&mut world, canvas);
    draw(&mut world, object, over_target(target));
    assert_eq!(graphics(&world)[0].texture, Some(UiTexture::Target(target)));
    let fonts = FontBook::new(UiLimits::default()).unwrap();
    let plan = extract_ui(&world, &fonts, VIEWPORT).unwrap();
    let draws = UiPresentation::from_plan(&plan).draws;
    assert_eq!(draws.len(), 1);
    assert_eq!(draws[0].texture, Some(UiTexture::Target(target)));
    assert_eq!(draws[0].validate(), Ok(()));
}

#[test]
fn a_target_the_world_no_longer_holds_is_kept_with_its_id() {
    let (mut world, canvas) = canvas_world(CanvasMode::ScreenSpaceOverlay);
    let target = render_texture(&mut world);
    let object = element(&mut world, canvas);
    draw(&mut world, object, over_target(target));
    world.destroy_render_texture(target).unwrap();
    let found = graphics(&world);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].texture, Some(UiTexture::Target(target)));
}

#[test]
fn a_renderer_cannot_name_a_coverage_texture() {
    let (mut world, canvas) = canvas_world(CanvasMode::ScreenSpaceOverlay);
    let object = element(&mut world, canvas);
    let coverage = CanvasRenderer {
        graphic_texture: Some(UiTexture::Coverage(0)),
        ..renderer(unit_quad())
    };
    let added = world.add_component(object, ComponentDesc::CanvasRenderer(coverage));
    assert!(added.is_err());
}

#[test]
fn only_a_graphic_draw_may_name_a_target_and_only_text_a_coverage() {
    use worldrunner_world_simulation::ui::presentation::UiDrawKind;
    use worldrunner_world_simulation::ui::{
        contribution::extract_ui, presentation::UiPresentation,
    };
    let (mut world, canvas) = canvas_world(CanvasMode::ScreenSpaceOverlay);
    let target = render_texture(&mut world);
    let object = element(&mut world, canvas);
    draw(&mut world, object, over_target(target));
    let fonts = FontBook::new(UiLimits::default()).unwrap();
    let plan = extract_ui(&world, &fonts, VIEWPORT).unwrap();
    let graphic = UiPresentation::from_plan(&plan).draws.remove(0);
    let mut text = graphic.clone();
    text.kind = UiDrawKind::Text;
    assert!(text.validate().is_err());
    let mut coverage = graphic;
    coverage.texture = Some(UiTexture::Coverage(0));
    assert!(coverage.validate().is_err());
}

#[test]
fn a_target_graphic_adds_no_sprite_quad() {
    let (mut world, canvas) = canvas_world(CanvasMode::ScreenSpaceOverlay);
    world
        .add_component(canvas, ComponentDesc::Camera(Camera::default()))
        .unwrap();
    let target = render_texture(&mut world);
    let object = element(&mut world, canvas);
    draw(&mut world, object, over_target(target));
    let mut plan = extract_world(&world, VIEWPORT).unwrap();
    append_graphics(&mut plan, graphics(&world)).unwrap();
    assert!(plan.quads.is_empty());
}
