//! A batch draw: the instances a game shader reads from a window of a compute buffer, drawn
//! through the shader's `DOTS_INSTANCING_ON` variant. The shader is the synthetic one the
//! render-program tests use. Instance `i` reads the matrix of element `visible[i]` at the byte
//! offset its metadata word gives, and the matrix of element `e` moves the quad of clip space
//! into the column the test chose for it, so each test predicts every column from the data.

use glam::{Mat4, Quat, Vec3, Vec4};
use worldrunner_render_program::unity::synthetic::SyntheticShader;
use worldrunner_render_program::unity::synthetic::dots::{
    HEADER, METADATA, ambient_dots_passes, dots_passes, lit_dots_passes, matrix, word,
};
use worldrunner_world_simulation::ComponentDesc;
use worldrunner_world_simulation::graphics::{ComputeBufferId, DrawBatch, RenderTextureId};
use worldrunner_world_simulation::presentation::{Camera, Light, LightKind, LightSettings};

use super::super::gpu::target;
use super::super::support::{camera, object, world};
use super::{
    ARGB32, GREEN, MESH, MaterialId, OffscreenWork, Rectangle, World, bind_and_clear, draw_mesh,
    flush, identity_view, mesh_of, record, scene, shader, stored,
};

const COLUMNS: usize = 4;

/// The shader of the file `dots`: a visible block of four entries.
pub(super) fn passes() -> SyntheticShader {
    dots_passes(COLUMNS, false)
}

/// The shader of the file `lit_dots`: the same block, drawing `unity_LightData.z` in red.
pub(super) fn lit_passes() -> SyntheticShader {
    lit_dots_passes(COLUMNS)
}

/// The shader of the file `ambient_dots`: the same block, drawing the fourth component of
/// `unity_SHAr` in blue.
pub(super) fn ambient_passes() -> SyntheticShader {
    ambient_dots_passes(COLUMNS)
}

/// The colour the shader gives element `e`.
fn element_color(element: u8) -> [u8; 4] {
    [element * 32, 102, 153, 255]
}

fn whole() -> super::MeshChannels {
    mesh_of(&[Rectangle {
        x: (-1.0, 1.0),
        y: (-1.0, 1.0),
    }])
}

/// A compute buffer of `lead` junk bytes, then `HEADER` junk bytes and the matrix of each element
/// of `column_of`, where element `e` moves the quad into column `column_of[e]`. The window starts
/// after the lead.
fn instances(world: &mut World, column_of: &[usize], lead: usize) -> (ComputeBufferId, u64, u64) {
    let mut bytes = vec![0xBB; lead];
    bytes.extend([0xAA; HEADER]);
    for &column in column_of {
        bytes.extend(
            matrix(column, COLUMNS)
                .iter()
                .flat_map(|value| value.to_le_bytes()),
        );
    }
    let buffer = world
        .create_compute_buffer(u32::try_from(bytes.len() / 4).unwrap(), 4, 0)
        .unwrap();
    world.set_compute_buffer_data(buffer, 0, &bytes).unwrap();
    (buffer, lead as u64, (bytes.len() - lead) as u64)
}

fn material(world: &mut World) -> MaterialId {
    let shader = shader(world, "dots");
    world.create_material(shader).unwrap()
}

fn draw_batch(world: &mut World, material: MaterialId, batch: DrawBatch) {
    record(
        world,
        OffscreenWork::DrawMeshBatch {
            mesh: MESH,
            material,
            submesh: 0,
            pass: -1,
            batch,
        },
    );
}

fn batch(data: (ComputeBufferId, u64, u64), visible: Vec<u32>) -> DrawBatch {
    DrawBatch {
        buffer: data.0,
        offset: data.1,
        size: data.2,
        metadata: vec![(METADATA.to_owned(), word())],
        visible,
        pass_tags: Vec::new(),
    }
}

#[test]
fn a_batch_with_three_visible_instances_draws_three_quads_at_their_matrices() {
    let (mut world, drawn) = scene();
    identity_view(&mut world);
    let material = material(&mut world);
    // Element 0 sits in column 3, element 1 in column 0 and element 2 in column 2, so the visible
    // order [2, 0, 1] draws instance 0 at element 2.
    let data = instances(&mut world, &[3, 0, 2], 0);
    draw_batch(&mut world, material, batch(data, vec![2, 0, 1]));

    let run = flush(&mut world, drawn, &stored([(MESH, whole())]));

    assert_eq!(
        run.columns(),
        [element_color(1), GREEN, element_color(2), element_color(0)]
    );
    assert!(
        run.outcome.report.failures.is_empty(),
        "{:?}",
        run.outcome.report.failures
    );
    assert!(run.outcome.report.missing_buffers.is_empty());
    assert!(
        run.outcome.writes.is_empty(),
        "a draw only reads the buffer"
    );
}

/// A batch of the draw's data that names `tags`, with pass -1.
fn tagged(data: (ComputeBufferId, u64, u64), tags: &[&str]) -> DrawBatch {
    DrawBatch {
        pass_tags: tags.iter().map(|tag| (*tag).to_owned()).collect(),
        ..batch(data, vec![0])
    }
}

#[test]
fn a_batch_with_pass_tags_draws_the_pass_the_first_tag_a_pass_has_names() {
    let (mut world, drawn) = scene();
    identity_view(&mut world);
    let material = material(&mut world);
    let data = instances(&mut world, &[2], 0);
    draw_batch(
        &mut world,
        material,
        tagged(data, &["ShadowCaster", "Alpha"]),
    );

    let run = flush(&mut world, drawn, &stored([(MESH, whole())]));

    assert_eq!(run.columns(), [GREEN, GREEN, element_color(0), GREEN]);
    assert!(run.outcome.report.skipped.is_empty());
}

#[test]
fn a_batch_whose_pass_tags_match_no_pass_is_skipped_by_name_and_keeps_its_neighbours() {
    let (mut world, drawn) = scene();
    identity_view(&mut world);
    let material = material(&mut world);
    let (before, left_out, after) = (
        instances(&mut world, &[0], 0),
        instances(&mut world, &[1], 0),
        instances(&mut world, &[3], 0),
    );
    draw_batch(&mut world, material, batch(before, vec![0]));
    draw_batch(&mut world, material, tagged(left_out, &["ShadowCaster"]));
    draw_batch(&mut world, material, batch(after, vec![0]));

    let run = flush(&mut world, drawn, &stored([(MESH, whole())]));

    assert_eq!(
        run.columns(),
        [element_color(0), GREEN, GREEN, element_color(0)]
    );
    assert_eq!(run.outcome.report.skipped[&super::Skip::NoTaggedPass], 1);
}

#[test]
fn a_window_that_starts_inside_the_buffer_is_what_the_metadata_offsets_count_from() {
    let (mut world, drawn) = scene();
    identity_view(&mut world);
    let material = material(&mut world);
    let data = instances(&mut world, &[1, 2], 256);
    draw_batch(&mut world, material, batch(data, vec![0, 1]));

    let run = flush(&mut world, drawn, &stored([(MESH, whole())]));

    assert_eq!(
        run.columns(),
        [GREEN, element_color(0), element_color(1), GREEN]
    );
}

#[test]
fn a_batch_over_a_buffer_the_host_does_not_hold_is_skipped_and_the_draws_around_it_stay_in_order() {
    let (mut world, drawn) = scene();
    identity_view(&mut world);
    let material = material(&mut world);
    let gone = instances(&mut world, &[0], 0);
    let before = instances(&mut world, &[1], 0);
    let after = instances(&mut world, &[3], 0);
    draw_batch(&mut world, material, batch(before, vec![0]));
    draw_batch(&mut world, material, batch(gone, vec![0]));
    draw_batch(&mut world, material, batch(after, vec![0]));
    world.destroy_compute_buffer(gone.0).unwrap();

    let run = flush(&mut world, drawn, &stored([(MESH, whole())]));

    assert_eq!(
        run.columns(),
        [GREEN, element_color(0), GREEN, element_color(0)]
    );
    assert_eq!(
        run.outcome.report.skipped[&super::Skip::BufferUnavailable],
        1
    );
}

/// The red the lit shader draws where `unity_LightData.z` is 1.
const LIT: [u8; 4] = [255, 0, 0, 255];

/// A world whose frame has a directional light, with a render target bound and cleared to green.
/// A snapshot holds lights only for a frame some camera renders, so a camera renders into a second
/// target first.
fn lit_scene() -> (World, RenderTextureId) {
    let mut world = world();
    let (drawn, aside) = (target(&mut world, ARGB32), target(&mut world, ARGB32));
    let camera = camera(&mut world, Camera::default(), Vec3::ZERO, Quat::IDENTITY);
    let holder = object(&mut world, 0, Vec3::ZERO, Quat::IDENTITY);
    let light = Light {
        enabled: true,
        settings: LightSettings {
            kind: LightKind::Directional,
            ..LightSettings::default()
        },
    };
    world
        .add_component(holder, ComponentDesc::Light(light))
        .unwrap();
    record(
        &mut world,
        OffscreenWork::CameraRender {
            camera,
            target: aside,
        },
    );
    bind_and_clear(&mut world, drawn);
    (world, drawn)
}

/// The matrix of `matrix(column, COLUMNS)` as a 4x4: the object-to-world a plain draw gives the
/// quad that a batch draw reads from its buffer.
fn placed_in(column: usize) -> Mat4 {
    let columns: Vec<Vec4> = matrix(column, COLUMNS)
        .chunks_exact(3)
        .map(|axis| Vec4::new(axis[0], axis[1], axis[2], 0.0))
        .collect();
    Mat4::from_cols(columns[0], columns[1], columns[2], columns[3].with_w(1.0))
}

#[test]
fn a_batch_draw_gets_the_light_factor_a_mesh_draw_gets() {
    let (mut world, drawn) = lit_scene();
    identity_view(&mut world);
    let lit = shader(&mut world, "lit_dots");
    let material = world.create_material(lit).unwrap();
    // The mesh draw fills column 1 and the batch draws element 0 into column 2.
    draw_mesh(&mut world, (MESH, placed_in(1)), material, (0, -1));
    let data = instances(&mut world, &[2], 0);
    draw_batch(&mut world, material, batch(data, vec![0]));

    let run = flush(&mut world, drawn, &stored([(MESH, whole())]));

    assert_eq!(run.columns(), [GREEN, LIT, LIT, GREEN]);
}

#[test]
fn a_batch_draw_reads_the_ambient_a_mesh_draw_reads() {
    let (mut world, drawn) = scene();
    identity_view(&mut world);
    let ambient = shader(&mut world, "ambient_dots");
    let material = world.create_material(ambient).unwrap();
    // The mesh draw fills column 1 and the batch draws element 0 into column 2.
    draw_mesh(&mut world, (MESH, placed_in(1)), material, (0, -1));
    let data = instances(&mut world, &[2], 0);
    draw_batch(&mut world, material, batch(data, vec![0]));

    let run = flush(&mut world, drawn, &stored([(MESH, whole())]));

    let columns = run.columns();
    assert!(columns[1][2] > 0, "the mesh draw reads the default ambient");
    assert_eq!(columns[2], columns[1]);
}

#[test]
fn a_batch_draw_is_counted_by_its_shader_pass_tags_and_properties() {
    let (mut world, drawn) = scene();
    identity_view(&mut world);
    let material = material(&mut world);
    let data = instances(&mut world, &[1], 0);
    draw_batch(&mut world, material, tagged(data, &["Alpha", "Other"]));

    let run = flush(&mut world, drawn, &stored([(MESH, whole())]));

    let note = format!("batch draw: dots [Alpha+Other] properties {METADATA}");
    assert_eq!(run.outcome.report.notes.get(&note), Some(&1));
}
