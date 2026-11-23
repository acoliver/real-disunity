//! Culling records and renderer lists: which renderers a list admits, in what order, and the
//! records they become. The worlds are synthetic; each test predicts the draws.

use std::collections::BTreeMap;

use glam::{Mat4, Quat, Vec3};
use worldrunner_render_program::properties::PropertyValue;
use worldrunner_render_program::unity::offscreen::frame::{DrawList, ListDraw, Record};
use worldrunner_render_program::unity::shader::PassSelector;
use worldrunner_world_simulation::graphics::{
    MaterialId, OffscreenWork, PropertyValue as WorldValue, RendererListDesc, TargetRef,
    WorldRenderTexture, WorldSampler,
};
use worldrunner_world_simulation::presentation::Camera;
use worldrunner_world_simulation::{
    ComponentDesc, ComponentHandle, LocalTransform, MeshFilter, MeshRenderer, ObjectDesc, World,
};

use super::super::bounds::Bounds;
use super::super::frame::{Adapted, Available, Settings, Skip, adapt};
use super::super::ids;
use super::support::{available, material, render_texture, shader, snapshot, world};

const FORWARD: &str = "UniversalForward";
const CASTER: &str = "ShadowCaster";
const UNLIT: &str = "Unlit";
const DEFAULT_UNLIT: &str = "SRPDefaultUnlit";
/// The meshes the renderers draw, one for each so a test can tell them apart in the frame.
pub(super) const MESH: u64 = 100;

mod shadow;
mod shadows_only;

/// A renderer of mesh `MESH + number` at `position` on `layer`, one slot for each material, that
/// casts shadows.
pub(super) fn placed(
    world: &mut World,
    place: (u64, u8, Vec3),
    materials: &[MaterialId],
    queue: Option<i32>,
) {
    placed_casting(world, place, materials, (queue, 1));
}

/// Like [`placed`], with the render queue and the shadow casting mode (0 off, 1 on, 2 two-sided,
/// 3 shadows only) of the renderer.
fn placed_casting(
    world: &mut World,
    (number, layer, position): (u64, u8, Vec3),
    materials: &[MaterialId],
    (queue, casting): (Option<i32>, u32),
) {
    let desc = ObjectDesc {
        layer,
        local_transform: LocalTransform {
            translation: position,
            rotation: Quat::IDENTITY,
            scale: Vec3::ONE,
        },
        ..ObjectDesc::default()
    };
    let object = world.create_object(desc).unwrap();
    let filter = MeshFilter {
        mesh: 3,
        drawn_mesh: MESH + number,
    };
    world
        .add_component(object, ComponentDesc::MeshFilter(filter))
        .unwrap();
    let renderer = MeshRenderer {
        material_count: u32::try_from(materials.len()).unwrap(),
        slot_materials: materials.iter().copied().map(Some).collect(),
        render_queue: queue,
        shadow_casting_mode: casting,
        ..MeshRenderer::default()
    };
    world
        .add_component(object, ComponentDesc::MeshRenderer(renderer))
        .unwrap();
}

/// A world with a target bound, a camera set up with `view` and a culling result 5 of `mask`.
///
/// The world copies the renderers of the layers that camera renders name, and only those, so the
/// scene also has a camera render of every layer into a texture of its own.
pub(super) fn scene(world: &mut World, view: Mat4, mask: u32) {
    scene_with_planes(world, view, mask, Vec::new());
}

/// Like [`scene`], with the planes of the volume that culling result 5 keeps. A world holds each
/// culling result id once, so a test that wants planes asks for them here and records no second
/// result 5.
pub(super) fn scene_with_planes(world: &mut World, view: Mat4, mask: u32, planes: Vec<[f32; 4]>) {
    let target = render_texture(world, [8, 8]);
    let object = super::support::object(world, 0, Vec3::ZERO, Quat::IDENTITY);
    let camera = world
        .add_component(object, ComponentDesc::Camera(Camera::default()))
        .unwrap();
    let unrelated = render_texture(world, [2, 2]);
    world
        .record_offscreen(OffscreenWork::CameraRender {
            camera,
            target: unrelated,
        })
        .unwrap();
    setup(world, camera, view, TargetRef::Render(target));
    cull(world, 5, mask, planes);
}

fn setup(world: &mut World, camera: ComponentHandle, view: Mat4, target: TargetRef) {
    world
        .record_offscreen(OffscreenWork::SetupCamera {
            camera,
            view: view.to_cols_array(),
            projection: Mat4::IDENTITY.to_cols_array(),
            target,
            buffers: None,
        })
        .unwrap();
}

fn cull(world: &mut World, id: u64, mask: u32, planes: Vec<[f32; 4]>) {
    let object = super::support::object(world, 0, Vec3::ZERO, Quat::IDENTITY);
    let camera = world
        .add_component(object, ComponentDesc::Camera(Camera::default()))
        .unwrap();
    world
        .record_offscreen(OffscreenWork::Cull {
            id,
            camera,
            culling_mask: mask,
            planes,
            options: 0,
        })
        .unwrap();
}

pub(super) fn drawing(tags: &[&str]) -> RendererListDesc {
    RendererListDesc::Draw {
        cull: 5,
        pass_tags: tags.iter().map(|tag| (*tag).to_owned()).collect(),
        queue_min: 0,
        queue_max: 2500,
        layer_mask: u32::MAX,
        rendering_layer_mask: u32::MAX,
        sorting: 0,
        per_object: 0,
        override_material: None,
        override_pass: 0,
    }
}

pub(super) fn draw(world: &mut World, desc: RendererListDesc) {
    let list = world.create_renderer_list(desc).unwrap();
    world
        .record_offscreen(OffscreenWork::DrawRendererList { list })
        .unwrap();
}

/// The shaders `available` knows, each with the `LightMode` values and the pass with no tag that
/// `MODES` gives its name.
fn with_modes(snapshot: &worldrunner_world_simulation::graphics::GraphicsSnapshot) -> Available {
    let mut available = available(snapshot, 3, &[]);
    for (id, info) in &mut available.shaders {
        let name = &snapshot.shaders[id].name;
        let (_, modes, untagged) = MODES.iter().find(|(held, ..)| held == name).unwrap();
        info.light_modes = modes.iter().map(|mode| (*mode).to_owned()).collect();
        info.has_untagged = *untagged;
    }
    available
}

pub(super) fn run(world: &mut World, edit: impl FnOnce(&mut Available)) -> Adapted {
    run_with(world, &Settings::default(), edit)
}

fn run_with(world: &mut World, settings: &Settings, edit: impl FnOnce(&mut Available)) -> Adapted {
    let snapshot = snapshot(world);
    let mut available = with_modes(&snapshot);
    edit(&mut available);
    adapt(&snapshot, &available, settings)
}

/// The lists the frame draws.
pub(super) fn lists(adapted: &Adapted) -> Vec<&DrawList> {
    adapted
        .frame
        .records
        .iter()
        .filter_map(|record| match record {
            Record::Draws(list) => Some(&**list),
            _ => None,
        })
        .collect()
}

/// The numbers of the meshes of a list's draws, in order, and their slots.
pub(super) fn meshes(adapted: &Adapted, list: &DrawList) -> Vec<(u64, usize)> {
    list.draws
        .iter()
        .map(|draw: &ListDraw| {
            let renderer = &adapted.frame.renderers[&draw.renderer];
            (renderer.mesh.0 - MESH, draw.slot)
        })
        .collect()
}

pub(super) fn counts(pairs: &[(&str, usize)]) -> BTreeMap<String, usize> {
    pairs
        .iter()
        .map(|(name, count)| ((*name).to_owned(), *count))
        .collect()
}

/// Three shaders: 0 draws forward and casts, 1 is unlit, 2 draws forward only.
pub(super) fn shaders(world: &mut World) -> [MaterialId; 3] {
    [
        shader(world, "forward-and-caster"),
        shader(world, "unlit"),
        shader(world, "forward"),
    ]
    .map(|shader| material(world, shader))
}

/// Three more: 3 has one pass with no tag, 4 has a forward pass and a pass with no tag, and 5 has
/// one pass tagged `Always`.
fn untagged_shaders(world: &mut World) -> [MaterialId; 3] {
    [
        shader(world, "untagged"),
        shader(world, "forward-and-untagged"),
        shader(world, "always"),
    ]
    .map(|shader| material(world, shader))
}

/// A shader whose shadow caster pass has the `LightMode` value in capitals, as the shaders of a
/// player build spell it.
fn shouting_caster(world: &mut World) -> MaterialId {
    let shader = shader(world, "forward-and-shouting-caster");
    material(world, shader)
}

/// The name of each shader, the `LightMode` values of its tagged passes and whether it has a pass
/// with no tag.
const MODES: [(&str, &[&str], bool); 7] = [
    ("forward-and-caster", &[FORWARD, CASTER], false),
    ("unlit", &[UNLIT], false),
    ("forward", &[FORWARD], false),
    ("untagged", &[], true),
    ("forward-and-untagged", &[FORWARD], true),
    ("always", &["Always"], false),
    (
        "forward-and-shouting-caster",
        &[FORWARD, "SHADOWCASTER"],
        false,
    ),
];

#[test]
fn a_culling_result_keeps_the_renderers_of_the_layers_in_its_mask() {
    let mut world = world();
    let [lit, ..] = shaders(&mut world);
    for (number, layer) in [(0, 1), (1, 2), (2, 3), (3, 1)] {
        placed(&mut world, (number, layer, Vec3::ZERO), &[lit], None);
    }
    scene(&mut world, Mat4::IDENTITY, (1 << 1) | (1 << 3));
    let adapted = run(&mut world, |_| {});
    assert_eq!(adapted.report.lists.culls, 1);
    assert_eq!(adapted.report.lists.kept, 3);
}

#[test]
fn a_culling_result_leaves_out_the_renderers_wholly_outside_a_plane() {
    let mut world = world();
    let [lit, ..] = shaders(&mut world);
    placed(&mut world, (0, 0, Vec3::ZERO), &[lit], None);
    placed(&mut world, (1, 0, Vec3::new(10.0, 0.0, 0.0)), &[lit], None);
    placed(&mut world, (2, 0, Vec3::new(5.0, 0.0, 0.0)), &[lit], None);
    let camera_object = super::support::object(&mut world, 0, Vec3::ZERO, Quat::IDENTITY);
    let camera = world
        .add_component(camera_object, ComponentDesc::Camera(Camera::default()))
        .unwrap();
    let unrelated = render_texture(&mut world, [2, 2]);
    world
        .record_offscreen(OffscreenWork::CameraRender {
            camera,
            target: unrelated,
        })
        .unwrap();
    // The volume x >= 5: a unit normal along x and the distance -5, inside where x - 5 >= 0.
    world
        .record_offscreen(OffscreenWork::Cull {
            id: 9,
            camera,
            culling_mask: u32::MAX,
            planes: vec![[1.0, 0.0, 0.0, -5.0]],
            options: 0,
        })
        .unwrap();
    let adapted = run(&mut world, |available| {
        // Every mesh is the box from -1 to 1 on each axis.
        let cube = Bounds {
            min: [-1.0; 3],
            max: [1.0; 3],
        };
        for number in 0..3 {
            available.bounds.insert(MESH + number, cube);
        }
    });
    // The renderer at 0 spans x from -1 to 1 and is outside; the one at 5 spans 4 to 6 and touches
    // the volume; the one at 10 is inside.
    assert_eq!(adapted.report.lists.kept, 2);
    assert_eq!(adapted.report.lists.out_by_planes, 1);
}

#[test]
fn a_drawing_list_admits_by_layer_queue_and_pass_tag_and_counts_the_rest_by_reason() {
    let mut world = world();
    let [lit, unlit, _] = shaders(&mut world);
    placed(&mut world, (0, 1, Vec3::ZERO), &[lit], None);
    placed(&mut world, (1, 2, Vec3::ZERO), &[unlit], None);
    placed(&mut world, (2, 3, Vec3::ZERO), &[lit], None);
    placed(&mut world, (3, 1, Vec3::ZERO), &[lit], Some(3000));
    // Slot 0 is admitted and slot 1, of the unlit shader, has no pass with the tag.
    placed(&mut world, (4, 2, Vec3::ZERO), &[lit, unlit], None);
    scene(&mut world, Mat4::IDENTITY, u32::MAX);
    let mut desc = drawing(&[FORWARD]);
    if let RendererListDesc::Draw { layer_mask, .. } = &mut desc {
        *layer_mask = (1 << 1) | (1 << 2);
    }
    draw(&mut world, desc);
    let adapted = run(&mut world, |_| {});
    let listed = lists(&adapted);
    assert_eq!(listed.len(), 1);
    let mut drawn = meshes(&adapted, listed[0]);
    drawn.sort_unstable();
    assert_eq!(drawn, [(0, 0), (4, 0)]);
    assert_eq!(
        listed[0].draws[0].pass,
        PassSelector::TagWithUntagged {
            key: "LightMode".to_owned(),
            values: vec![FORWARD.to_owned()],
            untagged: DEFAULT_UNLIT.to_owned(),
        }
    );
    let report = &adapted.report.lists;
    assert_eq!(report.draws_by_tag, counts(&[(FORWARD, 2)]));
    assert_eq!(
        report.left_out,
        counts(&[
            ("layer mask", 1),
            ("no pass with the list's tags", 2),
            ("render queue range", 1),
        ])
    );
    assert_eq!(report.drawn, 1);
}

#[test]
fn the_first_listed_tag_a_shader_has_names_the_pass_a_draw_is_counted_under() {
    let mut world = world();
    let [lit, unlit, _] = shaders(&mut world);
    placed(&mut world, (0, 0, Vec3::ZERO), &[lit], None);
    placed(&mut world, (1, 0, Vec3::ZERO), &[unlit], None);
    scene(&mut world, Mat4::IDENTITY, u32::MAX);
    draw(&mut world, drawing(&["Missing", UNLIT, FORWARD]));
    let adapted = run(&mut world, |_| {});
    assert_eq!(
        adapted.report.lists.draws_by_tag,
        counts(&[(FORWARD, 1), (UNLIT, 1)])
    );
}

#[test]
fn a_list_naming_srp_default_unlit_draws_a_shader_with_a_pass_with_no_light_mode() {
    let mut world = world();
    let [_, _, forward] = shaders(&mut world);
    let [untagged, ..] = untagged_shaders(&mut world);
    placed(&mut world, (0, 0, Vec3::ZERO), &[untagged], None);
    placed(&mut world, (1, 0, Vec3::ZERO), &[forward], None);
    scene(&mut world, Mat4::IDENTITY, u32::MAX);
    draw(&mut world, drawing(&[FORWARD, DEFAULT_UNLIT]));
    let adapted = run(&mut world, |_| {});
    let listed = lists(&adapted);
    assert_eq!(listed.len(), 1);
    assert_eq!(meshes(&adapted, listed[0]), [(0, 0), (1, 0)]);
    let report = &adapted.report.lists;
    assert_eq!(
        report.draws_by_tag,
        counts(&[(FORWARD, 1), (DEFAULT_UNLIT, 1)])
    );
    assert!(report.left_out.is_empty(), "{:?}", report.left_out);
}

#[test]
fn a_list_not_naming_srp_default_unlit_leaves_out_a_shader_with_a_pass_with_no_light_mode() {
    let mut world = world();
    let [_, _, forward] = shaders(&mut world);
    let [untagged, ..] = untagged_shaders(&mut world);
    placed(&mut world, (0, 0, Vec3::ZERO), &[untagged], None);
    placed(&mut world, (1, 0, Vec3::ZERO), &[forward], None);
    scene(&mut world, Mat4::IDENTITY, u32::MAX);
    draw(&mut world, drawing(&[FORWARD]));
    let adapted = run(&mut world, |_| {});
    let listed = lists(&adapted);
    assert_eq!(listed.len(), 1);
    assert_eq!(meshes(&adapted, listed[0]), [(1, 0)]);
    let report = &adapted.report.lists;
    assert_eq!(report.draws_by_tag, counts(&[(FORWARD, 1)]));
    assert_eq!(
        report.left_out,
        counts(&[("no pass with the list's tags", 1)])
    );
}

#[test]
fn a_list_with_nothing_but_untagged_shaders_is_empty_unless_it_names_srp_default_unlit() {
    let mut world = world();
    let [untagged, ..] = untagged_shaders(&mut world);
    placed(&mut world, (0, 0, Vec3::ZERO), &[untagged], None);
    scene(&mut world, Mat4::IDENTITY, u32::MAX);
    draw(&mut world, drawing(&[FORWARD, "UniversalForwardOnly"]));
    draw(
        &mut world,
        drawing(&["UniversalForwardOnly", DEFAULT_UNLIT]),
    );
    let adapted = run(&mut world, |_| {});
    assert_eq!(lists(&adapted).len(), 1);
    assert_eq!(adapted.report.lists.empty, 1);
    assert_eq!(adapted.report.lists.drawn, 1);
}

#[test]
fn a_list_counts_a_shader_with_a_tagged_and_an_untagged_pass_under_the_first_tag_it_names() {
    let mut world = world();
    let [_, mixed, _] = untagged_shaders(&mut world);
    placed(&mut world, (0, 0, Vec3::ZERO), &[mixed], None);
    scene(&mut world, Mat4::IDENTITY, u32::MAX);
    draw(&mut world, drawing(&[FORWARD, DEFAULT_UNLIT]));
    draw(&mut world, drawing(&[DEFAULT_UNLIT, FORWARD]));
    let adapted = run(&mut world, |_| {});
    // The list's own order of tags decides, as it does for tagged passes.
    assert_eq!(
        adapted.report.lists.draws_by_tag,
        counts(&[(FORWARD, 1), (DEFAULT_UNLIT, 1)])
    );
    assert_eq!(lists(&adapted).len(), 2);
}

#[test]
fn a_pass_tagged_always_is_not_a_pass_with_no_light_mode_for_a_list() {
    let mut world = world();
    let [.., always] = untagged_shaders(&mut world);
    placed(&mut world, (0, 0, Vec3::ZERO), &[always], None);
    scene(&mut world, Mat4::IDENTITY, u32::MAX);
    draw(&mut world, drawing(&[FORWARD, DEFAULT_UNLIT]));
    let adapted = run(&mut world, |_| {});
    assert!(lists(&adapted).is_empty());
    assert_eq!(
        adapted.report.lists.left_out,
        counts(&[("no pass with the list's tags", 1)])
    );
}

#[test]
fn an_override_material_draws_every_admitted_slot_with_the_pass_it_names() {
    let mut world = world();
    let [lit, unlit, over] = shaders(&mut world);
    placed(&mut world, (0, 0, Vec3::ZERO), &[lit], None);
    placed(&mut world, (1, 0, Vec3::ZERO), &[unlit], None);
    // The snapshot holds the materials of renderers and blits only, so the override material is
    // also a renderer's, a renderer the queue range leaves out.
    placed(&mut world, (2, 0, Vec3::ZERO), &[over], Some(4000));
    scene(&mut world, Mat4::IDENTITY, u32::MAX);
    let mut desc = drawing(&["Whatever"]);
    if let RendererListDesc::Draw {
        override_material,
        override_pass,
        ..
    } = &mut desc
    {
        *override_material = Some(over);
        *override_pass = 2;
    }
    draw(&mut world, desc);
    let adapted = run(&mut world, |_| {});
    let listed = lists(&adapted);
    let mut drawn = meshes(&adapted, listed[0]);
    drawn.sort_unstable();
    assert_eq!(
        drawn,
        [(0, 0), (1, 0)],
        "the renderer in queue 4000 stays out"
    );
    for draw in &listed[0].draws {
        assert_eq!(draw.material, Some(ids::material(over)));
        assert_eq!(draw.pass, PassSelector::Index(2));
    }
    assert_eq!(
        adapted.report.lists.draws_by_tag,
        counts(&[("override pass 2", 2)])
    );
    assert_eq!(
        adapted.report.lists.left_out,
        counts(&[("render queue range", 1)])
    );
}

fn sorted(sorting: u32) -> Vec<(u64, usize)> {
    let mut world = world();
    let [lit, ..] = shaders(&mut world);
    // Meshes 0 to 3 at distances 3, 1, 2 and 2 along z from the camera at the origin, in queues
    // 2000, 2100, 2100 and 2000.
    placed(
        &mut world,
        (0, 0, Vec3::new(0.0, 0.0, 3.0)),
        &[lit],
        Some(2000),
    );
    placed(
        &mut world,
        (1, 0, Vec3::new(0.0, 0.0, 1.0)),
        &[lit],
        Some(2100),
    );
    placed(
        &mut world,
        (2, 0, Vec3::new(0.0, 0.0, 2.0)),
        &[lit],
        Some(2100),
    );
    placed(
        &mut world,
        (3, 0, Vec3::new(0.0, 0.0, -2.0)),
        &[lit],
        Some(2000),
    );
    scene(&mut world, Mat4::IDENTITY, u32::MAX);
    let mut desc = drawing(&[FORWARD]);
    if let RendererListDesc::Draw {
        sorting: bits,
        queue_max,
        ..
    } = &mut desc
    {
        *bits = sorting;
        *queue_max = 5000;
    }
    draw(&mut world, desc);
    let adapted = run(&mut world, |_| {});
    let listed = lists(&adapted);
    meshes(&adapted, listed[0])
}

#[test]
fn a_list_keeps_the_culling_order_when_it_asks_for_no_sorting() {
    let order = sorted(0);
    let mut expected = order.clone();
    expected.sort_unstable();
    // The four draws are all there; with no sort bit they are in the order the world listed them.
    let mut found = order;
    found.sort_unstable();
    assert_eq!(found, expected);
    assert_eq!(found, [(0, 0), (1, 0), (2, 0), (3, 0)]);
}

#[test]
fn sorting_by_queue_orders_the_queues_and_keeps_the_culling_order_within_one() {
    let order = sorted(2);
    let queues: Vec<i32> = order
        .iter()
        .map(|(mesh, _)| [2000, 2100, 2100, 2000][usize::try_from(*mesh).unwrap()])
        .collect();
    assert_eq!(queues, [2000, 2000, 2100, 2100]);
}

#[test]
fn sorting_back_to_front_puts_the_farthest_first_and_front_to_back_the_nearest() {
    // The distances are 3, 1, 2 and 2 (the last behind the camera): by distance squared 9, 1, 4, 4.
    let far_first = sorted(4);
    assert_eq!(far_first[0], (0, 0));
    assert_eq!(far_first[3], (1, 0));
    let near_first = sorted(8);
    assert_eq!(near_first[0], (1, 0));
    assert_eq!(near_first[3], (0, 0));
}

#[test]
fn queue_comes_before_distance_when_both_are_asked_for() {
    let order = sorted(2 | 4);
    // Queue 2000 holds meshes 0 (distance 3) and 3 (distance 2): the farther first. Queue 2100 holds
    // meshes 1 (distance 1) and 2 (distance 2): the farther first.
    assert_eq!(order, [(0, 0), (3, 0), (2, 0), (1, 0)]);
}

#[test]
fn a_draw_carries_the_view_the_script_set_up_and_the_targets_it_bound() {
    let mut world = world();
    let [lit, ..] = shaders(&mut world);
    placed(&mut world, (0, 0, Vec3::ZERO), &[lit], None);
    super::support::sun(&mut world, Quat::IDENTITY);
    scene(
        &mut world,
        Mat4::from_translation(Vec3::new(1.0, 2.0, 3.0)),
        u32::MAX,
    );
    draw(&mut world, drawing(&[FORWARD]));
    let adapted = run(&mut world, |_| {});
    let listed = lists(&adapted);
    let first = listed[0];
    let values = &first.values.values;
    // The light shines along +Z, so the direction toward it is -Z. It is the host's fallback in
    // the frame's starting globals, below whatever the script sets, and not in the view.
    assert_eq!(
        adapted.frame.globals.values.get("_MainLightPosition"),
        Some(&PropertyValue::Vector([0.0, 0.0, -1.0, 0.0]))
    );
    assert_eq!(values.get("_MainLightPosition"), None);
    // The script's projection is the identity, which has no perspective divide, puts the near
    // plane at 1 and the far plane at -1 and has the half sizes 1. The target is 8 by 8.
    assert_eq!(
        values.get("unity_OrthoParams"),
        Some(&PropertyValue::Vector([1.0, 1.0, 0.0, 1.0]))
    );
    assert_eq!(
        values.get("_ProjectionParams"),
        Some(&PropertyValue::Vector([1.0, 1.0, -1.0, -1.0]))
    );
    assert_eq!(
        values.get("_ScreenParams"),
        Some(&PropertyValue::Vector([8.0, 8.0, 1.125, 1.125]))
    );
    assert_eq!(
        values.get("unity_MatrixV"),
        Some(&PropertyValue::Matrix([
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 1.0, 2.0, 3.0, 1.0
        ]))
    );
    // The clip depth takes (w - z) / 2, so the translation's z of 3 becomes -1 and z scales by
    // -1/2. The player's stages negate clip Y, so the matrix negates it first: y scales by -1 and
    // the translation's y of 2 becomes -2.
    assert_eq!(
        values.get("unity_MatrixVP"),
        Some(&PropertyValue::Matrix([
            1.0, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0, 0.0, 0.0, 0.0, -0.5, 0.0, 1.0, -2.0, -1.0, 1.0
        ]))
    );
    assert_eq!(
        values.get("_WorldSpaceCameraPos"),
        Some(&PropertyValue::Vector([-1.0, -2.0, -3.0, 0.0]))
    );
    assert!(!first.target.colors.is_empty() && first.target.depth.is_some());
}

#[test]
fn a_view_change_after_the_setup_reaches_the_draw_and_keeps_the_bound_targets() {
    let mut world = world();
    let [lit, ..] = shaders(&mut world);
    placed(&mut world, (0, 0, Vec3::ZERO), &[lit], None);
    scene(&mut world, Mat4::IDENTITY, u32::MAX);
    world
        .record_offscreen(OffscreenWork::SetViewProjection {
            view: Mat4::from_translation(Vec3::new(4.0, 5.0, 6.0)).to_cols_array(),
            projection: Mat4::IDENTITY.to_cols_array(),
        })
        .unwrap();
    draw(&mut world, drawing(&[FORWARD]));
    let adapted = run(&mut world, |_| {});
    let first = lists(&adapted)[0];
    assert_eq!(
        first.values.values.get("_WorldSpaceCameraPos"),
        Some(&PropertyValue::Vector([-4.0, -5.0, -6.0, 0.0])),
        "the draw sees the view the command buffer set"
    );
    assert!(!first.target.colors.is_empty() && first.target.depth.is_some());
}

#[test]
fn globals_set_before_a_draw_reach_the_draw_through_a_record_in_front_of_it() {
    let mut world = world();
    let [lit, ..] = shaders(&mut world);
    placed(&mut world, (0, 0, Vec3::ZERO), &[lit], None);
    scene(&mut world, Mat4::IDENTITY, u32::MAX);
    world
        .record_offscreen(OffscreenWork::SetGlobal(WorldValue::Float(
            "_Fade".to_owned(),
            0.5,
        )))
        .unwrap();
    draw(&mut world, drawing(&[FORWARD]));
    let adapted = run(&mut world, |_| {});
    let kinds: Vec<_> = adapted
        .frame
        .records
        .iter()
        .map(|record| match record {
            Record::Globals(_) => "globals",
            Record::Draws(_) => "draws",
            _ => "other",
        })
        .collect();
    assert_eq!(
        kinds,
        ["other", "globals", "draws"],
        "the camera render, then the draw"
    );
}

#[test]
fn a_draw_with_no_target_bound_or_with_nothing_to_draw_makes_no_record() {
    let mut world = world();
    let [lit, unlit, _] = shaders(&mut world);
    placed(&mut world, (0, 0, Vec3::ZERO), &[unlit], None);
    let object = super::support::object(&mut world, 0, Vec3::ZERO, Quat::IDENTITY);
    let camera = world
        .add_component(object, ComponentDesc::Camera(Camera::default()))
        .unwrap();
    let unrelated = render_texture(&mut world, [2, 2]);
    world
        .record_offscreen(OffscreenWork::CameraRender {
            camera,
            target: unrelated,
        })
        .unwrap();
    cull(&mut world, 5, u32::MAX, Vec::new());
    draw(&mut world, drawing(&[FORWARD]));
    let adapted = run(&mut world, |_| {});
    assert_eq!(
        adapted.report.skipped_by_kind["DrawRendererList: UnboundTarget"],
        1
    );
    // With a target bound the list is admitted and empty: the one renderer has no forward pass.
    let mut world = super::support::world();
    let [_, unlit, _] = shaders(&mut world);
    let _ = lit;
    placed(&mut world, (0, 0, Vec3::ZERO), &[unlit], None);
    scene(&mut world, Mat4::IDENTITY, u32::MAX);
    draw(&mut world, drawing(&[FORWARD]));
    let adapted = run(&mut world, |_| {});
    assert!(lists(&adapted).is_empty());
    assert_eq!(adapted.report.lists.empty, 1);
    assert!(adapted.report.skipped.is_empty());
}

#[test]
fn a_renderer_whose_mesh_the_host_lacks_is_left_out_of_the_list_by_reason() {
    let mut world = world();
    let [lit, ..] = shaders(&mut world);
    placed(&mut world, (0, 0, Vec3::ZERO), &[lit], None);
    placed(&mut world, (1, 0, Vec3::ZERO), &[lit], None);
    scene(&mut world, Mat4::IDENTITY, u32::MAX);
    draw(&mut world, drawing(&[FORWARD]));
    let adapted = run(&mut world, |available| {
        available.meshes.remove(&(MESH + 1));
    });
    let listed = lists(&adapted);
    assert_eq!(meshes(&adapted, listed[0]), [(0, 0)]);
    assert_eq!(
        adapted.report.lists.left_out,
        counts(&[("renderer MeshUnavailable", 1)])
    );
    assert_eq!(
        adapted.report.renderers_skipped.values().sum::<usize>(),
        1,
        "counted once, not again by the frame's own pass over the renderers"
    );
}

#[test]
fn a_draw_into_a_camera_with_part_of_its_target_is_skipped_by_reason() {
    let mut world = world();
    let [lit, ..] = shaders(&mut world);
    placed(&mut world, (0, 0, Vec3::ZERO), &[lit], None);
    let target = render_texture(&mut world, [8, 8]);
    let object = super::support::object(&mut world, 0, Vec3::ZERO, Quat::IDENTITY);
    let settings = Camera {
        viewport: [0.0, 0.0, 0.5, 1.0],
        ..Camera::default()
    };
    let camera = world
        .add_component(object, ComponentDesc::Camera(settings))
        .unwrap();
    // A camera render names the camera, so the snapshot holds its rectangle.
    let unrelated = render_texture(&mut world, [4, 4]);
    world
        .record_offscreen(OffscreenWork::CameraRender {
            camera,
            target: unrelated,
        })
        .unwrap();
    setup(
        &mut world,
        camera,
        Mat4::IDENTITY,
        TargetRef::Render(target),
    );
    cull(&mut world, 5, u32::MAX, Vec::new());
    draw(&mut world, drawing(&[FORWARD]));
    let adapted = run(&mut world, |_| {});
    assert_eq!(
        adapted.report.skipped_by_kind["DrawRendererList: PartialViewport"],
        1
    );
    assert_eq!(adapted.report.skipped.get(&Skip::PartialViewport), Some(&2));
    assert!(lists(&adapted).is_empty());
}

#[test]
fn a_draw_into_a_mip_has_the_size_of_that_mip() {
    let mut world = world();
    let [lit, ..] = shaders(&mut world);
    placed(&mut world, (0, 0, Vec3::ZERO), &[lit], None);
    scene(&mut world, Mat4::IDENTITY, u32::MAX);
    let mipped = world
        .create_render_texture(WorldRenderTexture {
            width: 16,
            height: 8,
            format: 8,
            depth_bits: 0,
            mip_count: 3,
            srgb: false,
            dimension: 2,
            volume_depth: 1,
            auto_generate_mips: false,
            sampler: WorldSampler::default(),
            generation: 0,
        })
        .unwrap();
    world
        .record_offscreen(OffscreenWork::SetRenderTarget {
            color: vec![TargetRef::Render(mipped)],
            depth: None,
            mip: 2,
            face: -1,
            slice: -1,
        })
        .unwrap();
    draw(&mut world, drawing(&[FORWARD]));
    let adapted = run(&mut world, |_| {});
    assert_eq!(
        lists(&adapted)[0].values.values.get("_ScreenParams"),
        Some(&PropertyValue::Vector([4.0, 2.0, 1.25, 1.5])),
        "level 2 of a 16 by 8 texture is 4 by 2"
    );
}

#[test]
fn a_draw_into_a_depth_target_alone_has_the_size_of_the_depth_target() {
    let mut world = world();
    let [lit, ..] = shaders(&mut world);
    placed(&mut world, (0, 0, Vec3::ZERO), &[lit], None);
    scene(&mut world, Mat4::IDENTITY, u32::MAX);
    let depth = render_texture(&mut world, [6, 2]);
    world
        .record_offscreen(OffscreenWork::SetRenderTarget {
            color: Vec::new(),
            depth: Some(TargetRef::Render(depth)),
            mip: 0,
            face: -1,
            slice: -1,
        })
        .unwrap();
    draw(&mut world, drawing(&[FORWARD]));
    let adapted = run(&mut world, |_| {});
    assert_eq!(
        lists(&adapted)[0].values.values.get("_ScreenParams"),
        Some(&PropertyValue::Vector([6.0, 2.0, 1.0 + 1.0 / 6.0, 1.5]))
    );
}
