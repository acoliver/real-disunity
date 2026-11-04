//! `Graphics.CopyTexture`: whole-mip copies between elements of texture arrays.

use worldrunner_render_program::ids::TextureId;
use worldrunner_render_program::offscreen::OffscreenError;
use worldrunner_render_program::offscreen::frame::{OffscreenFrame, Record};

use super::rig::{BLUE, GREEN, RED, Rig, ZERO, image, pixel, source, texture};

const SOURCE: TextureId = TextureId(5);
const ARRAY: TextureId = TextureId(12);

fn copy(source: TextureId, target: TextureId, to: (u32, u32), from: (u32, u32)) -> Record {
    Record::CopyTexture {
        source,
        source_element: from.0,
        source_mip: from.1,
        target,
        target_element: to.0,
        target_mip: to.1,
    }
}

fn array(size: [u32; 2], layers: u32, mips: u32) -> OffscreenFrame {
    let mut spec = texture(12, size);
    spec.layers = layers;
    spec.mips = mips;
    OffscreenFrame {
        render_textures: vec![spec],
        ..OffscreenFrame::default()
    }
}

fn rig_with_sources() -> Rig {
    let mut rig = Rig::new();
    let quadrants = image([4, 4], |x, y| match (x < 2, y < 2) {
        (true, true) => RED,
        (false, true) => GREEN,
        (true, false) => BLUE,
        (false, false) => [255, 255, 255, 255],
    });
    rig.state
        .set_texture(&mut rig.runtime, &source(5, [4, 4], quadrants))
        .unwrap();
    rig.state
        .set_texture(&mut rig.runtime, &source(6, [2, 2], vec![9; 16]))
        .unwrap();
    rig
}

#[test]
fn a_copy_fills_one_element_and_leaves_the_others_as_they_were() {
    let mut rig = rig_with_sources();
    let mut frame = array([4, 4], 3, 1);
    frame.records = vec![copy(SOURCE, ARRAY, (2, 0), (0, 0))];
    let result = rig
        .run(&frame, &[(ARRAY, 0, 0), (ARRAY, 0, 1), (ARRAY, 0, 2)])
        .unwrap();
    assert!(result[0].chunks(4).all(|texel| texel == ZERO));
    assert!(result[1].chunks(4).all(|texel| texel == ZERO));
    assert_eq!(pixel(&result[2], 4, (0, 0)), RED);
    assert_eq!(pixel(&result[2], 4, (3, 0)), GREEN);
    assert_eq!(pixel(&result[2], 4, (0, 3)), BLUE);
    assert_eq!(pixel(&result[2], 4, (3, 3)), [255, 255, 255, 255]);
}

#[test]
fn a_copy_names_the_mip_it_reads_and_the_mip_it_writes() {
    let mut rig = rig_with_sources();
    let mut frame = array([4, 4], 1, 2);
    // The 2x2 source goes to mip 1 of the 4x4 target, which is 2x2.
    frame.records = vec![copy(TextureId(6), ARRAY, (0, 1), (0, 0))];
    let result = rig.run(&frame, &[(ARRAY, 1, 0), (ARRAY, 0, 0)]).unwrap();
    assert!(result[0].iter().all(|byte| *byte == 9));
    assert!(result[1].chunks(4).all(|texel| texel == ZERO));
}

#[test]
fn copies_run_in_call_order_and_the_later_one_wins() {
    let mut rig = rig_with_sources();
    let mut frame = array([4, 4], 2, 1);
    frame.records = vec![
        copy(SOURCE, ARRAY, (0, 0), (0, 0)),
        copy(ARRAY, ARRAY, (1, 0), (0, 0)),
    ];
    let result = rig.run(&frame, &[(ARRAY, 0, 1)]).unwrap();
    assert_eq!(pixel(&result[0], 4, (0, 0)), RED);
    assert_eq!(pixel(&result[0], 4, (3, 3)), [255, 255, 255, 255]);
}

#[test]
fn a_copy_between_sizes_that_differ_is_a_named_error() {
    let mut rig = rig_with_sources();
    let mut frame = array([4, 4], 1, 1);
    frame.records = vec![copy(TextureId(6), ARRAY, (0, 0), (0, 0))];
    let error = rig.build(&frame).err().unwrap();
    assert!(
        matches!(&error, OffscreenError::CopyMismatch { reason, .. } if reason.contains("sizes")),
        "{error}"
    );
}

#[test]
fn a_copy_between_formats_that_differ_is_a_named_error() {
    let mut rig = rig_with_sources();
    let mut frame = array([4, 4], 1, 1);
    frame.render_textures[0].graphics_format = 48;
    frame.records = vec![copy(SOURCE, ARRAY, (0, 0), (0, 0))];
    let error = rig.build(&frame).err().unwrap();
    assert!(
        matches!(&error, OffscreenError::CopyMismatch { reason, .. } if reason.contains("formats")),
        "{error}"
    );
}

#[test]
fn a_copy_to_an_element_or_mip_the_target_lacks_is_a_named_error() {
    let mut rig = rig_with_sources();
    for to in [(2, 0), (0, 1)] {
        let mut frame = array([4, 4], 2, 1);
        frame.records = vec![copy(SOURCE, ARRAY, to, (0, 0))];
        let error = rig.build(&frame).err().unwrap();
        assert!(
            matches!(&error, OffscreenError::NoSuchSubresource { texture, .. } if *texture == ARRAY),
            "{to:?}: {error}"
        );
    }
}

#[test]
fn a_format_outside_the_table_is_a_named_error_with_its_code() {
    let mut rig = rig_with_sources();
    let mut frame = array([4, 4], 1, 1);
    frame.render_textures[0].graphics_format = 49;
    let error = rig.build(&frame).err().unwrap();
    assert!(
        matches!(error, OffscreenError::UnsupportedFormat { code: 49 }),
        "{error}"
    );
}

#[test]
fn an_srgb_flag_that_disagrees_with_the_format_is_a_named_error() {
    let mut rig = rig_with_sources();
    let mut frame = array([4, 4], 1, 1);
    frame.render_textures[0].srgb = true;
    let error = rig.build(&frame).err().unwrap();
    assert!(
        matches!(error, OffscreenError::SrgbMismatch { .. }),
        "{error}"
    );
}

#[test]
fn a_depth_that_is_not_16_24_or_32_bits_is_a_named_error() {
    let mut rig = rig_with_sources();
    let mut frame = array([4, 4], 1, 1);
    frame.render_textures[0].depth_bits = 8;
    let error = rig.build(&frame).err().unwrap();
    assert!(
        matches!(error, OffscreenError::UnsupportedDepth { bits: 8 }),
        "{error}"
    );
}
