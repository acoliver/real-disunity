//! Instances a game shader reads from a raw buffer, drawn through the shader's `DOTS_INSTANCING_ON`
//! variant.
//!
//! The synthetic shader has the shape of a game's DOTS variant: a read-only raw buffer
//! `unity_DOTSInstanceData` at set 0, binding 5, a metadata block whose words are byte offsets
//! into it, and a block of visible-instance entries indexed by the instance index. The vertex
//! stage reads one matrix of four packed float3 columns, 48 bytes, at the metadata offset plus 48
//! times the visible entry. The target is `CELLS` cells across, two texels each, and the matrix of
//! element `e` moves the quad into the cell the test chose for it.

use worldrunner_render_program::ids::{MaterialId, RendererId};
use worldrunner_render_program::layers::PropertyLayer;
use worldrunner_render_program::unity::offscreen::frame::{
    DotsBatch, Instances, ListDraw, OffscreenFrame,
};
use worldrunner_render_program::unity::runtime::BufferHandle;
use worldrunner_render_program::unity::shader::ProgramShader;
use worldrunner_render_program::unity::synthetic::shaders::{no_cull, variant};
use worldrunner_render_program::unity::synthetic::{
    PassSpec, VERTEX, VariantSpec, binding, constant_buffer, shader, slot,
};
use worldrunner_unity_serialized::shader_parameters::{
    BufferMember, ResourceKind, StructLayout, VariantParameters,
};

use super::buffers::{alpha, bare, frame, render};
use super::rig::{CLEAR, Rig, pixel, texture};

pub(super) const KEYWORD: &str = "DOTS_INSTANCING_ON";
const METADATA: &str = "unity_ObjectToWorld";
const MARKER: [u8; 4] = [0, 255, 255, 255];
/// The bytes in front of the first matrix, which the metadata word skips.
const HEADER: usize = 16;
const MATRIX_BYTES: usize = 48;

const PLAIN: &str = "
struct VOut { @builtin(position) position: vec4<f32> }
@vertex fn vs_main(@location(0) position: vec3<f32>) -> VOut {
    var out: VOut;
    out.position = vec4<f32>(position, 1.0);
    return out;
}
@fragment fn fs_main(in: VOut) -> @location(0) vec4<f32> { return vec4<f32>(0.0, 1.0, 1.0, 1.0); }";

/// The WGSL of the DOTS variant, whose visible block holds `entries` entries.
fn dots_wgsl(entries: usize) -> String {
    format!(
        "
struct Entry {{ data: vec4<u32> }}
struct Visible {{ entries: array<Entry, {entries}> }}
struct Words {{ to_world: i32, to_object: i32 }}
@group(1) @binding(2) var<uniform> visible: Visible;
@group(1) @binding(3) var<uniform> words: Words;
@group(0) @binding(5) var<storage, read> instances: array<f32>;
struct VOut {{
    @builtin(position) position: vec4<f32>,
    @location(0) @interpolate(flat) color: vec4<f32>,
}}
fn column(base: u32, k: u32) -> vec3<f32> {{
    let at = base + 3u * k;
    return vec3<f32>(instances[at], instances[at + 1u], instances[at + 2u]);
}}
@vertex fn vs_main(
    @location(0) position: vec3<f32>,
    @builtin(instance_index) i: u32,
) -> VOut {{
    let element = visible.entries[i].data.x;
    let base = (u32(words.to_world) & 0x7fffffffu) / 4u + element * 12u;
    let moved = column(base, 0u) * position.x + column(base, 1u) * position.y
        + column(base, 2u) * position.z + column(base, 3u);
    var out: VOut;
    out.position = vec4<f32>(moved, 1.0);
    out.color = vec4<f32>(f32(element) / 8.0, 0.3, 0.6, 1.0);
    return out;
}}
@fragment fn fs_main(in: VOut) -> @location(0) vec4<f32> {{ return in.color; }}"
    )
}

fn member(name: &str, kind: i32, columns: i32, offset: i32) -> BufferMember {
    BufferMember {
        name: name.to_owned(),
        kind,
        rows: 1,
        columns,
        is_matrix: false,
        array_size: 0,
        offset,
    }
}

/// The parameters of a DOTS variant: the metadata block, and the visible block of `entries`
/// entries of 16 bytes, which a player stores as a struct array.
fn dots_parameters(entries: i32) -> VariantParameters {
    let metadata = constant_buffer(
        "UnityDOTSInstancing_BuiltinPropertyMetadata",
        8,
        vec![
            member(
                "unity_DOTSInstancingF48_Metadataunity_ObjectToWorld",
                1,
                1,
                0,
            ),
            member(
                "unity_DOTSInstancingF48_Metadataunity_WorldToObject",
                1,
                1,
                4,
            ),
        ],
    );
    let mut visible = constant_buffer(
        "UnityInstancingDOTS_InstanceVisibility",
        entries * 16,
        Vec::new(),
    );
    visible.structs.push(StructLayout {
        name: "unity_DOTSVisibleInstances".to_owned(),
        index: 0,
        array_size: entries,
        size: 16,
        members: vec![member("VisibleData", 1, 4, 0)],
    });
    VariantParameters {
        constant_buffers: vec![metadata, visible],
        bindings: vec![
            binding(
                "UnityInstancingDOTS_InstanceVisibility",
                slot(1, 2, VERTEX),
                ResourceKind::ConstantBuffer,
            ),
            binding(
                "UnityDOTSInstancing_BuiltinPropertyMetadata",
                slot(1, 3, VERTEX),
                ResourceKind::ConstantBuffer,
            ),
            binding(
                "unity_DOTSInstanceData",
                slot(0, 5, VERTEX),
                ResourceKind::Buffer,
            ),
        ],
    }
}

fn spec(keywords: &[&'static str], wgsl: &str, parameters: VariantParameters) -> VariantSpec {
    VariantSpec {
        channels: vec![[0, 0]],
        ..variant(keywords, wgsl, "fs_main", parameters)
    }
}

/// A shader whose plain variant draws the whole target in `MARKER`, and whose DOTS variant has a
/// visible block of `entries` entries.
pub(super) fn dots_shader(entries: usize) -> ProgramShader {
    let plain = spec(
        &[],
        PLAIN,
        VariantParameters {
            constant_buffers: Vec::new(),
            bindings: Vec::new(),
        },
    );
    let dots = spec(
        &[KEYWORD],
        &dots_wgsl(entries),
        dots_parameters(i32::try_from(entries).unwrap()),
    );
    let synthetic = no_cull(shader(
        &[KEYWORD],
        &[PassSpec {
            tags: vec![("LIGHTMODE", "Alpha")],
            variants: vec![plain, dots],
        }],
    ));
    ProgramShader::new(synthetic.shader, synthetic.chunks).unwrap()
}

/// The matrix that moves the quad of clip space into cell `cell` of `cells`: scale `1 / cells`
/// in x and a translation to the cell's center, as four columns of three floats.
fn matrix(cell: usize, cells: usize) -> [f32; 12] {
    let cells = f32::from(u16::try_from(cells).unwrap());
    let center = (2.0 * f32::from(u16::try_from(cell).unwrap()) + 1.0) / cells - 1.0;
    [
        1.0 / cells,
        0.0,
        0.0,
        0.0,
        1.0,
        0.0,
        0.0,
        0.0,
        1.0,
        center,
        0.0,
        0.0,
    ]
}

/// A raw buffer of junk, then the matrix of each element: element `e` moves the quad into cell
/// `cell_of[e]`.
pub(super) fn instance_data(rig: &mut Rig, cell_of: &[usize], cells: usize) -> BufferHandle {
    let mut bytes = vec![0xAA; HEADER];
    for &cell in cell_of {
        bytes.extend(
            matrix(cell, cells)
                .iter()
                .flat_map(|value| value.to_le_bytes()),
        );
    }
    assert_eq!(bytes.len(), HEADER + cell_of.len() * MATRIX_BYTES);
    rig.runtime.create_buffer("instances", &bytes).unwrap()
}

/// The metadata word of the matrices: bit 31 set, then the byte offset of the first.
pub(super) fn word() -> u32 {
    0x8000_0000 | u32::try_from(HEADER).unwrap()
}

pub(super) fn dots_item(buffer: BufferHandle, visible: Vec<u32>) -> ListDraw {
    ListDraw {
        renderer: RendererId(1),
        slot: 0,
        material: Some(MaterialId(1)),
        pass: alpha(),
        instances: Instances::Dots(DotsBatch {
            buffer,
            metadata: vec![(METADATA.to_owned(), word())],
            visible,
        }),
        per_draw: PropertyLayer::default(),
    }
}

/// A frame of `cells` cells of two texels across and two rows that draws `items`.
pub(super) fn dots_frame(
    rig: &mut Rig,
    entries: usize,
    cells: usize,
    items: Vec<ListDraw>,
) -> OffscreenFrame {
    let mut frame = frame(rig, dots_shader(entries), vec![bare()], items);
    let width = u32::try_from(cells * 2).unwrap();
    frame.render_textures = vec![texture(10, [width, 2])];
    frame
}

/// The color the DOTS variant gives element `e`.
pub(super) fn element_color(element: u8) -> [u8; 4] {
    [element * 32, 77, 153, 255]
}

/// Checks the two texel columns of every cell against `expected`, the clear colour where `None`.
pub(super) fn assert_cells(image: &[u8], expected: &[Option<[u8; 4]>]) {
    let width = u32::try_from(expected.len() * 2).unwrap();
    for (cell, want) in expected.iter().enumerate() {
        let want = want.unwrap_or(CLEAR);
        for x in 0..2 {
            for y in 0..2 {
                let at = (u32::try_from(cell).unwrap() * 2 + x, y);
                let texel = pixel(image, width, at);
                assert!(
                    texel
                        .iter()
                        .zip(want)
                        .all(|(&got, want)| got.abs_diff(want) <= 1),
                    "{at:?} is {texel:?}, expected {want:?} in cell {cell}"
                );
            }
        }
    }
}

#[test]
fn three_instances_are_drawn_at_the_matrices_their_visible_indexes_name() {
    let mut rig = Rig::new();
    // Element 0 sits in cell 3, element 1 in cell 0 and element 2 in cell 2, so the visible
    // order [2, 0, 1] draws instance 0 at element 2.
    let buffer = instance_data(&mut rig, &[3, 0, 2], 4);
    let frame = dots_frame(&mut rig, 4, 4, vec![dots_item(buffer, vec![2, 0, 1])]);
    let operations = rig.build(&frame).unwrap();
    assert!(operations.missing_buffer_draws().is_empty());
    drop(operations);
    let image = render(&mut rig, &frame);
    assert_cells(
        &image,
        &[
            Some(element_color(1)),
            None,
            Some(element_color(2)),
            Some(element_color(0)),
        ],
    );
    assert!(
        !image.chunks(4).any(|texel| texel == MARKER),
        "the plain variant drew"
    );
}
