//! A material's structured buffers, and instanced draws: a read-only storage buffer of
//! per-instance colours that the vertex stage indexes by instance, as a unit icon's instanced draw
//! reads its properties.
//!
//! The vertex stage puts instance `i` in the `i`th third of clip x, so each instance covers two
//! columns of the 6 by 6 target and every row. Each test predicts the texel of each column.

use std::collections::BTreeMap;
use std::sync::Arc;

use worldrunner_render_program::ids::{MaterialId, RendererId, ShaderId, TextureId};
use worldrunner_render_program::layers::PropertyLayer;
use worldrunner_render_program::unity::offscreen::MissingBufferDraws;
use worldrunner_render_program::unity::offscreen::frame::{
    Attachment, Attachments, Clear, DrawList, DrawRaster, Instances, ListDraw, Material,
    OffscreenFrame, Record, Renderer,
};
use worldrunner_render_program::unity::runtime::BufferHandle;
use worldrunner_render_program::unity::shader::{PassSelector, ProgramShader};
use worldrunner_render_program::unity::synthetic::shaders::{no_cull, variant};
use worldrunner_render_program::unity::synthetic::{
    FRAGMENT, PassSpec, VERTEX, VariantSpec, binding, shader, slot,
};
use worldrunner_unity_serialized::shader_parameters::{ResourceKind, VariantParameters};

use super::rig::{CLEAR, QUAD, Rig, pixel, quad, texture};
use crate::common::Gpu;

pub(super) const PROGRAM: ShaderId = ShaderId(1);
pub(super) const TARGET: TextureId = TextureId(10);
const SIDE: u32 = 6;
const CELLS: u32 = 3;
const PROPERTIES: &str = "_Properties";

const FIRST: [u8; 4] = [200, 40, 90, 255];
const SECOND: [u8; 4] = [30, 220, 60, 255];
const THIRD: [u8; 4] = [90, 70, 250, 255];
const OTHER: [[u8; 4]; 3] = [
    [10, 20, 30, 255],
    [110, 120, 130, 255],
    [210, 220, 230, 255],
];

/// The vertex stage of every shader here: instance `i` of a quad over clip space is moved into the
/// `i`th of three cells across clip x, and its colour is `color` of the instance. The fragment
/// stage returns `fragment`, an expression over the vertex stage's output `in`.
fn wgsl(resource: &str, color: &str, fragment: &str) -> String {
    format!(
        "
{resource}
struct VOut {{
    @builtin(position) position: vec4<f32>,
    @location(0) @interpolate(flat) color: vec4<f32>,
}}
@vertex fn vs_main(
    @location(0) position: vec3<f32>,
    @builtin(instance_index) i: u32,
) -> VOut {{
    var out: VOut;
    let x = (position.x * 0.5 + 0.5 + f32(i)) / 3.0 * 2.0 - 1.0;
    out.position = vec4<f32>(x, position.y, position.z, 1.0);
    out.color = {color};
    return out;
}}
@fragment fn fs_main(in: VOut) -> @location(0) vec4<f32> {{ return {fragment}; }}"
    )
}

/// A shader whose vertex stage reads the `_Properties` entry of the instance, declared with the
/// WGSL `access` and type `ty`, and whose parameters say `stages` use it. The fragment stage
/// returns the vertex colour.
fn reading(access: &str, ty: &str, stages: u8) -> ProgramShader {
    declared(access, ty, stages, ("props[i]", "in.color"))
}

/// A shader like [`reading`] whose vertex stage makes the colour `color` and whose fragment stage
/// returns `fragment`.
fn declared(access: &str, ty: &str, stages: u8, (color, fragment): (&str, &str)) -> ProgramShader {
    let parameters = VariantParameters {
        constant_buffers: Vec::new(),
        bindings: vec![binding(
            PROPERTIES,
            slot(0, 0, stages),
            ResourceKind::Buffer,
        )],
    };
    let resource = format!("@group(0) @binding(0) var<storage, {access}> props: {ty};");
    program(&wgsl(&resource, color, fragment), parameters)
}

const COLORS: &str = "array<vec4<f32>>";

/// A shader with no resources whose colour is red, green or blue for instance 0, 1 or 2.
fn counting() -> ProgramShader {
    let colors = "vec4<f32>(select(0.0, 1.0, i == 0u), select(0.0, 1.0, i == 1u), \
                  select(0.0, 1.0, i == 2u), 1.0)";
    program(
        &wgsl("", colors, "in.color"),
        VariantParameters {
            constant_buffers: Vec::new(),
            bindings: Vec::new(),
        },
    )
}

fn program(wgsl: &str, parameters: VariantParameters) -> ProgramShader {
    let spec = VariantSpec {
        channels: vec![[0, 0]],
        ..variant(&[], wgsl, "fs_main", parameters)
    };
    let synthetic = no_cull(shader(
        &[],
        &[PassSpec {
            tags: vec![("LIGHTMODE", "Alpha")],
            variants: vec![spec],
        }],
    ));
    ProgramShader::new(synthetic.shader, synthetic.chunks).unwrap()
}

pub(super) fn alpha() -> PassSelector {
    PassSelector::Tag {
        key: "LightMode".to_owned(),
        values: vec!["Alpha".to_owned()],
    }
}

/// A buffer of `colors`, one vec4 of floats for each.
fn colors(rig: &mut Rig, colors: &[[u8; 4]]) -> BufferHandle {
    let bytes: Vec<u8> = colors
        .iter()
        .flatten()
        .flat_map(|&byte| (f32::from(byte) / 255.0).to_le_bytes())
        .collect();
    rig.runtime.create_buffer("colors", &bytes).unwrap()
}

fn with_buffer(buffer: BufferHandle) -> Material {
    let mut properties = PropertyLayer::default();
    properties.buffers.insert(PROPERTIES.to_owned(), buffer);
    Material {
        shader: Some(PROGRAM),
        keywords: Vec::new(),
        properties,
        fallback: PropertyLayer::default(),
        below_globals: PropertyLayer::default(),
        render_queue: 2000,
    }
}

/// A material that names no buffer.
pub(super) fn bare() -> Material {
    Material {
        shader: Some(PROGRAM),
        keywords: Vec::new(),
        properties: PropertyLayer::default(),
        fallback: PropertyLayer::default(),
        below_globals: PropertyLayer::default(),
        render_queue: 2000,
    }
}

fn item(material: u64, instances: u32) -> ListDraw {
    ListDraw {
        renderer: RendererId(1),
        slot: 0,
        material: Some(MaterialId(material)),
        pass: alpha(),
        instances: Instances::Count(instances),
        per_draw: PropertyLayer::default(),
    }
}

/// A frame that clears the target and draws `items` of renderer 1, a quad, in order.
pub(super) fn frame(
    rig: &mut Rig,
    program: ProgramShader,
    materials: Vec<Material>,
    items: Vec<ListDraw>,
) -> OffscreenFrame {
    rig.state.add_shader(PROGRAM, program);
    rig.state
        .set_mesh(&mut rig.runtime, QUAD, &quad(0.5))
        .unwrap();
    let whole = Attachments {
        colors: vec![Attachment {
            texture: TARGET,
            mip: 0,
        }],
        depth: None,
    };
    OffscreenFrame {
        render_textures: vec![texture(10, [SIDE, SIDE])],
        materials: (1..).map(MaterialId).zip(materials).collect(),
        renderers: BTreeMap::from([(
            RendererId(1),
            Renderer {
                mesh: QUAD,
                materials: Vec::new(),
                layer: 0,
                block: PropertyLayer::default(),
                slot_blocks: std::collections::BTreeMap::new(),
                per_draw: PropertyLayer::default(),
            },
        )]),
        records: vec![
            Record::Clear {
                target: whole.clone(),
                clear: Clear {
                    color: Some(CLEAR.map(|c| f64::from(c) / 255.0)),
                    depth: None,
                    stencil: None,
                },
            },
            Record::Draws(Arc::new(DrawList {
                raster: DrawRaster::default(),
                target: whole,
                values: PropertyLayer::default(),
                front_face: wgpu::FrontFace::Cw,
                draws: items,
            })),
        ],
        ..OffscreenFrame::default()
    }
}

pub(super) fn render(rig: &mut Rig, frame: &OffscreenFrame) -> Vec<u8> {
    rig.run(frame, &[(TARGET, 0, 0)]).unwrap().remove(0)
}

/// Checks every texel against the colour of its cell: `cells[x * 3 / 6]`, the clear colour for a
/// cell nothing drew.
fn assert_cells(image: &[u8], cells: [Option<[u8; 4]>; 3]) {
    for y in 0..SIDE {
        for x in 0..SIDE {
            let expected = cells[(x * CELLS / SIDE) as usize].unwrap_or(CLEAR);
            let texel = pixel(image, SIDE, (x, y));
            assert!(
                texel
                    .iter()
                    .zip(expected)
                    .all(|(&got, want)| got.abs_diff(want) <= 1),
                "({x}, {y}) is {texel:?}, expected {expected:?}"
            );
        }
    }
}

#[test]
fn a_mesh_draw_issues_the_instances_it_names() {
    let mut rig = Rig::new();
    let frame = frame(&mut rig, counting(), vec![bare()], vec![item(1, 2)]);
    let image = render(&mut rig, &frame);
    assert_cells(
        &image,
        [Some([255, 0, 0, 255]), Some([0, 255, 0, 255]), None],
    );
}

#[test]
fn three_instances_read_the_three_entries_of_the_materials_buffer() {
    let mut rig = Rig::new();
    let buffer = colors(&mut rig, &[FIRST, SECOND, THIRD]);
    let frame = frame(
        &mut rig,
        reading("read", COLORS, VERTEX),
        vec![with_buffer(buffer)],
        vec![item(1, 3)],
    );
    let operations = rig.build(&frame).unwrap();
    assert!(operations.missing_buffer_draws().is_empty());
    let image = render(&mut rig, &frame);
    assert_cells(&image, [Some(FIRST), Some(SECOND), Some(THIRD)]);
}

#[test]
fn the_buffer_is_bound_to_the_fragment_stage_when_the_shader_declares_it_there() {
    let mut rig = Rig::new();
    let buffer = colors(&mut rig, &[FIRST, SECOND, THIRD]);
    let frame = frame(
        &mut rig,
        declared(
            "read",
            COLORS,
            VERTEX | FRAGMENT,
            ("props[i]", "props[u32(in.position.x) / 2u]"),
        ),
        vec![with_buffer(buffer)],
        vec![item(1, 3)],
    );
    let image = render(&mut rig, &frame);
    assert_cells(&image, [Some(FIRST), Some(SECOND), Some(THIRD)]);
}

#[test]
fn a_per_draw_buffer_replaces_the_materials() {
    let mut rig = Rig::new();
    let material = colors(&mut rig, &[FIRST, SECOND, THIRD]);
    let own = colors(&mut rig, &OTHER);
    let mut frame = frame(
        &mut rig,
        reading("read", COLORS, VERTEX),
        vec![with_buffer(material)],
        vec![item(1, 3)],
    );
    let renderer = frame.renderers.get_mut(&RendererId(1)).unwrap();
    renderer.per_draw.buffers.insert(PROPERTIES.to_owned(), own);
    let image = render(&mut rig, &frame);
    assert_cells(&image, OTHER.map(Some));
}

#[test]
fn the_materials_buffer_replaces_the_globals() {
    let mut rig = Rig::new();
    let material = colors(&mut rig, &[FIRST, SECOND, THIRD]);
    let global = colors(&mut rig, &OTHER);
    let mut frame = frame(
        &mut rig,
        reading("read", COLORS, VERTEX),
        vec![with_buffer(material)],
        vec![item(1, 3)],
    );
    frame.globals.buffers.insert(PROPERTIES.to_owned(), global);
    let image = render(&mut rig, &frame);
    assert_cells(&image, [Some(FIRST), Some(SECOND), Some(THIRD)]);
}

#[test]
fn a_global_buffer_fills_in_for_a_material_that_names_none() {
    let mut rig = Rig::new();
    let global = colors(&mut rig, &OTHER);
    let mut frame = frame(
        &mut rig,
        reading("read", COLORS, VERTEX),
        vec![bare()],
        vec![item(1, 3)],
    );
    frame.globals.buffers.insert(PROPERTIES.to_owned(), global);
    let operations = rig.build(&frame).unwrap();
    assert!(operations.missing_buffer_draws().is_empty());
    let image = render(&mut rig, &frame);
    assert_cells(&image, OTHER.map(Some));
}

#[test]
fn a_draw_whose_buffer_no_layer_names_is_left_out_and_counted_by_name() {
    let mut rig = Rig::new();
    let buffer = colors(&mut rig, &[FIRST, SECOND, THIRD]);
    let frame = frame(
        &mut rig,
        reading("read", COLORS, VERTEX),
        vec![bare(), with_buffer(buffer)],
        vec![item(1, 3), item(2, 1)],
    );
    let operations = rig.build(&frame).unwrap();
    assert_eq!(
        operations.missing_buffer_draws(),
        [MissingBufferDraws {
            buffer: PROPERTIES.to_owned(),
            draws: 1,
        }]
    );
    assert!(operations.missing_texture_draws().is_empty());
    let image = render(&mut rig, &frame);
    // Only the second item drew: one instance, in the first cell.
    assert_cells(&image, [Some(FIRST), None, None]);
}

#[test]
fn a_writable_storage_buffer_is_refused_by_name() {
    let mut rig = Rig::new();
    let buffer = colors(&mut rig, &[FIRST, SECOND, THIRD]);
    let frame = frame(
        &mut rig,
        // A writable storage buffer in a vertex stage is refused when the program is made, and
        // the fragment stage is where a draw could bind one.
        declared(
            "read_write",
            COLORS,
            FRAGMENT,
            ("vec4<f32>(0.0)", "props[u32(in.position.x) / 2u]"),
        ),
        vec![with_buffer(buffer)],
        vec![item(1, 3)],
    );
    let message = rig
        .build(&frame)
        .err()
        .expect("the frame is refused")
        .to_string();
    assert!(
        message.contains(PROPERTIES) && message.contains("writable storage buffer"),
        "{message}"
    );
}

#[test]
fn a_buffer_smaller_than_the_shader_reads_is_refused_by_name() {
    let mut rig = Rig::new();
    let buffer = colors(&mut rig, &[FIRST, SECOND]);
    let frame = frame(
        &mut rig,
        reading("read", "array<vec4<f32>, 3>", VERTEX),
        vec![with_buffer(buffer)],
        vec![item(1, 3)],
    );
    let message = rig
        .build(&frame)
        .err()
        .expect("the frame is refused")
        .to_string();
    assert!(
        message.contains(PROPERTIES) && message.contains("32 bytes") && message.contains("48"),
        "{message}"
    );
}

#[test]
fn a_device_with_no_storage_buffers_in_a_stage_refuses_the_buffer_by_name() {
    let mut rig = Rig::on(Gpu::with_limits(|limits| {
        limits.max_storage_buffers_per_shader_stage = 0;
    }));
    let buffer = colors(&mut rig, &[FIRST, SECOND, THIRD]);
    let frame = frame(
        &mut rig,
        reading("read", COLORS, VERTEX),
        vec![with_buffer(buffer)],
        vec![item(1, 3)],
    );
    let message = rig
        .build(&frame)
        .err()
        .expect("the frame is refused")
        .to_string();
    assert!(
        message.contains(PROPERTIES) && message.contains("vertex"),
        "{message}"
    );
}

#[test]
fn an_item_of_no_instances_draws_nothing_and_the_items_around_it_still_draw() {
    let mut rig = Rig::new();
    let first = colors(&mut rig, &[FIRST; 3]);
    let second = colors(&mut rig, &[SECOND; 3]);
    let third = colors(&mut rig, &[THIRD; 3]);
    let frame = frame(
        &mut rig,
        reading("read", COLORS, VERTEX),
        vec![
            with_buffer(first),
            with_buffer(second),
            with_buffer(third),
            bare(),
        ],
        vec![item(1, 1), item(2, 0), item(3, 2), item(4, 0)],
    );
    let operations = rig.build(&frame).unwrap();
    // The last item names no buffer, but with no instances it plans nothing and is not refused.
    assert!(operations.missing_buffer_draws().is_empty());
    let image = render(&mut rig, &frame);
    assert_cells(&image, [Some(THIRD), Some(THIRD), None]);
}
