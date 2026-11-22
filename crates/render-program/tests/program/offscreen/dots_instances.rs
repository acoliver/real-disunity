//! Instances a game shader reads from a raw buffer, drawn through the shader's `DOTS_INSTANCING_ON`
//! variant.
//!
//! The synthetic shader has the shape of a game's DOTS variant: a read-only raw buffer
//! `unity_DOTSInstanceData` at set 0, binding 5, a metadata block whose words are byte offsets
//! into it, and a block of visible-instance entries indexed by the instance index. The vertex
//! stage reads one matrix of four packed float3 columns, 48 bytes, at the metadata offset plus 48
//! times the visible entry. The target is `CELLS` cells across, two texels each, and the matrix of
//! element `e` moves the quad into the cell the test chose for it.

use std::ops::Range;

use worldrunner_render_program::ids::{MaterialId, RendererId};
use worldrunner_render_program::layers::PropertyLayer;
use worldrunner_render_program::unity::offscreen::OffscreenOperations;
use worldrunner_render_program::unity::offscreen::frame::{
    DotsBatch, Instances, ListDraw, OffscreenFrame,
};
use worldrunner_render_program::unity::runtime::BufferHandle;
use worldrunner_render_program::unity::shader::ProgramShader;
use worldrunner_render_program::unity::synthetic::dots::{
    HEADER, MARKER, METADATA, dots_passes, matrix, word,
};

use super::buffers::{alpha, bare, frame, render};
use super::rig::{CLEAR, Rig, pixel, texture};

/// [`dots_passes`] as a program shader.
pub(super) fn dots_shader(entries: usize, reads_indirect: bool) -> ProgramShader {
    let synthetic = dots_passes(entries, reads_indirect);
    ProgramShader::new(synthetic.shader, synthetic.chunks).unwrap()
}

/// A raw buffer and the part of it a batch reads.
pub(super) struct Data {
    buffer: BufferHandle,
    window: Range<u64>,
}

/// A raw buffer of `lead` bytes of junk, then the window a batch reads: junk of `HEADER` bytes
/// and the matrix of each element, where element `e` moves the quad into cell `cell_of[e]`.
pub(super) fn instance_data(rig: &mut Rig, cell_of: &[usize], cells: usize, lead: usize) -> Data {
    let mut bytes = vec![0xBB; lead];
    bytes.extend([0xAA; HEADER]);
    for &cell in cell_of {
        bytes.extend(
            matrix(cell, cells)
                .iter()
                .flat_map(|value| value.to_le_bytes()),
        );
    }
    let window = lead as u64..bytes.len() as u64;
    let buffer = rig.runtime.create_buffer("instances", &bytes).unwrap();
    Data { buffer, window }
}

pub(super) fn dots_item(data: &Data, visible: Vec<u32>) -> ListDraw {
    ListDraw {
        renderer: RendererId(1),
        slot: 0,
        material: Some(MaterialId(1)),
        pass: alpha(),
        instances: Instances::Dots(DotsBatch {
            buffer: data.buffer,
            window: data.window.clone(),
            metadata: vec![(METADATA.to_owned(), word())],
            visible,
        }),
        per_draw: PropertyLayer::default(),
    }
}

/// A frame of `cells` cells of two texels across and two rows that draws `items`.
pub(super) fn dots_frame(
    rig: &mut Rig,
    shader: ProgramShader,
    cells: usize,
    items: Vec<ListDraw>,
) -> OffscreenFrame {
    let mut frame = frame(rig, shader, vec![bare()], items);
    let width = u32::try_from(cells * 2).unwrap();
    frame.render_textures = vec![texture(10, [width, 2])];
    frame
}

/// The color the DOTS variant gives element `e`.
pub(super) fn element_color(element: u8) -> [u8; 4] {
    [element * 32, 102, 153, 255]
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
    let data = instance_data(&mut rig, &[3, 0, 2], 4, 0);
    let frame = dots_frame(
        &mut rig,
        dots_shader(4, false),
        4,
        vec![dots_item(&data, vec![2, 0, 1])],
    );
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

fn drawn(operations: &OffscreenOperations) -> usize {
    operations.list_draws()[0].drawn
}

#[test]
fn a_batch_longer_than_the_visible_block_is_drawn_in_chunks_of_the_blocks_length() {
    let mut rig = Rig::new();
    let cells = 300;
    let data = instance_data(&mut rig, &(0..cells).collect::<Vec<_>>(), cells, 0);
    let visible = (0..u32::try_from(cells).unwrap()).collect();
    let frame = dots_frame(
        &mut rig,
        dots_shader(256, false),
        cells,
        vec![dots_item(&data, visible)],
    );
    let operations = rig.build(&frame).unwrap();
    assert_eq!(drawn(&operations), 2, "256 instances, then 44");
    drop(operations);
    let image = render(&mut rig, &frame);
    let width = u32::try_from(cells * 2).unwrap();
    for cell in 0..u32::try_from(cells).unwrap() {
        let texel = pixel(&image, width, (cell * 2, 1));
        assert!(
            texel != CLEAR && texel != MARKER,
            "cell {cell} is {texel:?}, not drawn by the DOTS variant"
        );
    }
}

#[test]
fn chunks_of_a_batch_draw_the_instances_in_the_order_the_batch_lists_them() {
    let mut rig = Rig::new();
    let data = instance_data(&mut rig, &[0, 1, 2, 3, 4], 5, 0);
    let frame = dots_frame(
        &mut rig,
        dots_shader(2, false),
        5,
        vec![dots_item(&data, vec![4, 3, 2, 1, 0])],
    );
    let operations = rig.build(&frame).unwrap();
    assert_eq!(drawn(&operations), 3, "chunks of 2, 2 and 1");
    drop(operations);
    let image = render(&mut rig, &frame);
    assert_cells(&image, &[0, 1, 2, 3, 4].map(|e| Some(element_color(e))));
}

#[test]
fn a_batch_with_no_visible_instance_draws_nothing_and_its_neighbours_still_draw() {
    let mut rig = Rig::new();
    let data = instance_data(&mut rig, &[3, 1], 4, 0);
    let items = vec![dots_item(&data, Vec::new()), dots_item(&data, vec![1, 0])];
    let frame = dots_frame(&mut rig, dots_shader(4, false), 4, items);
    let operations = rig.build(&frame).unwrap();
    assert_eq!(drawn(&operations), 1);
    drop(operations);
    let image = render(&mut rig, &frame);
    assert_cells(
        &image,
        &[None, Some(element_color(1)), None, Some(element_color(0))],
    );
}

#[test]
fn a_window_that_starts_inside_the_buffer_is_what_the_metadata_offsets_count_from() {
    let mut rig = Rig::new();
    // 256 bytes of junk in front of the window: the alignment every device binds storage at.
    let data = instance_data(&mut rig, &[3, 0, 2], 4, 256);
    let items = vec![dots_item(&data, vec![2, 0, 1])];
    let frame = dots_frame(&mut rig, dots_shader(4, false), 4, items);
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
}

#[test]
fn a_shader_that_declares_the_indirect_visibility_buffer_is_still_drawn() {
    let mut rig = Rig::new();
    let data = instance_data(&mut rig, &[3, 0, 2], 4, 0);
    let items = vec![dots_item(&data, vec![2, 0, 1])];
    let frame = dots_frame(&mut rig, dots_shader(4, true), 4, items);
    let operations = rig.build(&frame).unwrap();
    assert!(operations.missing_buffer_draws().is_empty());
    assert_eq!(drawn(&operations), 1);
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
}

#[test]
fn a_window_the_device_cannot_bind_is_refused_by_name() {
    let mut rig = Rig::new();
    let data = instance_data(&mut rig, &[3, 0, 2], 4, 4);
    let items = vec![dots_item(&data, vec![2, 0, 1])];
    let frame = dots_frame(&mut rig, dots_shader(4, false), 4, items);
    let message = rig
        .build(&frame)
        .err()
        .expect("the frame is refused")
        .to_string();
    assert!(
        message.contains("unity_DOTSInstanceData") && message.contains("multiples of"),
        "{message}"
    );
}
