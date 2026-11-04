//! A render texture with no color format and depth bits is a depth-only target: a camera draws
//! into it in a pass with no color attachment, and its depth is copied and sampled.

use worldrunner_render_post::contracts::TypedWork;
use worldrunner_render_program::ids::{MaterialId, RendererId, TextureId};
use worldrunner_render_program::offscreen::OffscreenError;
use worldrunner_render_program::offscreen::frame::{OffscreenFrame, Record};

use super::camera::fixture;
use super::formats::{blit_frame, blit_setup};
use super::rig::{BLIT_QUAD, NEAR_QUAD, Rig, layer, pixel, quad, texture};

const NO_COLOR: u32 = 0;
const TARGET: TextureId = TextureId(10);
const COPY: TextureId = TextureId(12);
const SAMPLED: TextureId = TextureId(11);
const LEFT_ONLY: u32 = 1 << 1;
const BOTH: u32 = (1 << 1) | (1 << 2);

/// A depth-only 8x8 target of `bits` depth bits. With `BOTH` layers the left half is drawn at
/// depth 0.5 and the right half at depth 0.25.
fn depth_frame(rig: &mut Rig, mask: u32, bits: u8) -> OffscreenFrame {
    let mut frame = fixture(rig, mask);
    frame.render_textures[0].graphics_format = NO_COLOR;
    frame.render_textures[0].depth_bits = bits;
    rig.state
        .set_mesh(&mut rig.runtime, NEAR_QUAD, &quad(0.25))
        .unwrap();
    frame.renderers.get_mut(&RendererId(2)).unwrap().mesh = NEAR_QUAD;
    frame
}

fn depths(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks(4)
        .map(|float| f32::from_le_bytes(float.try_into().unwrap()))
        .collect()
}

#[test]
fn a_camera_writes_only_depth_into_a_depth_only_render_texture() {
    let mut rig = Rig::new();
    let frame = depth_frame(&mut rig, BOTH, 32);
    let operations = rig.build(&frame).unwrap();
    let passes: Vec<_> = operations
        .operations()
        .iter()
        .filter_map(|operation| match &operation.work {
            TypedWork::Draws { pass, .. } => Some(pass),
            _ => None,
        })
        .collect();
    assert_eq!(passes.len(), 1);
    assert!(passes[0].colors.is_empty(), "no color attachment");
    assert!(passes[0].depth.is_some());
    drop(operations);
    let image = rig.run(&frame, &[(TARGET, 0, 0)]).unwrap().remove(0);
    let texels = depths(&image);
    assert_eq!(texels.len(), 64);
    for y in 0..8 {
        for x in 0..4 {
            assert_eq!(
                texels[y * 8 + x].to_bits(),
                0.5_f32.to_bits(),
                "left ({x}, {y})"
            );
            assert_eq!(
                texels[y * 8 + x + 4].to_bits(),
                0.25_f32.to_bits(),
                "right ({}, {y})",
                x + 4
            );
        }
    }
}

#[test]
fn a_depth_only_render_texture_is_cleared_to_far_where_nothing_draws() {
    let mut rig = Rig::new();
    let frame = depth_frame(&mut rig, LEFT_ONLY, 32);
    let image = rig.run(&frame, &[(TARGET, 0, 0)]).unwrap().remove(0);
    let texels = depths(&image);
    assert_eq!(texels[3].to_bits(), 0.5_f32.to_bits());
    assert_eq!(texels[4].to_bits(), 1.0_f32.to_bits());
}

/// The depth of `target` read as a float texture by a blit pass into an 8x8 RGBA8 texture, with the
/// blit's red tint at 1.5: depth 0.5 gives red 0.75 (191) and depth 1.0 saturates to 255.
fn sample_depth(frame: &mut OffscreenFrame, rig: &mut Rig, source: TextureId) -> Vec<u8> {
    blit_setup(rig, [4, 4]);
    frame.blit_mesh = Some(BLIT_QUAD);
    frame.render_textures.push(texture(11, [8, 8]));
    frame.materials.insert(
        MaterialId(3),
        blit_frame(1.5, 8, [8, 8])
            .materials
            .remove(&MaterialId(1))
            .unwrap(),
    );
    frame.records.push(Record::Blit {
        source,
        target: SAMPLED,
        material: Some(MaterialId(3)),
        pass: 0,
    });
    rig.run(frame, &[(SAMPLED, 0, 0)]).unwrap().remove(0)
}

fn assert_sampled_depth(image: &[u8]) {
    for y in 0..8 {
        for x in 0..4 {
            assert_eq!(
                pixel(image, 8, (x, y)),
                [191, 0, 0, 255],
                "drawn ({x}, {y})"
            );
            assert_eq!(
                pixel(image, 8, (x + 4, y)),
                [255, 0, 0, 255],
                "cleared ({}, {y})",
                x + 4
            );
        }
    }
}

#[test]
fn a_depth_only_render_texture_of_24_bits_is_drawn_and_sampled_by_a_later_blit() {
    let mut rig = Rig::new();
    let mut frame = depth_frame(&mut rig, LEFT_ONLY, 24);
    let image = sample_depth(&mut frame, &mut rig, TARGET);
    assert_sampled_depth(&image);
}

#[test]
fn a_depth_copy_between_depth_only_render_textures_carries_the_depth() {
    let mut rig = Rig::new();
    let mut frame = depth_frame(&mut rig, LEFT_ONLY, 24);
    let mut copy = texture(12, [8, 8]);
    copy.graphics_format = NO_COLOR;
    copy.depth_bits = 24;
    frame.render_textures.push(copy);
    frame.records.push(Record::CopyTexture {
        source: TARGET,
        source_element: 0,
        source_mip: 0,
        target: COPY,
        target_element: 0,
        target_mip: 0,
    });
    let image = sample_depth(&mut frame, &mut rig, COPY);
    assert_sampled_depth(&image);
}

#[test]
fn a_depth_that_cannot_be_copied_into_a_buffer_is_a_named_error_at_the_readback() {
    let mut rig = Rig::new();
    let frame = depth_frame(&mut rig, LEFT_ONLY, 24);
    let mut operations = rig.build(&frame).unwrap();
    let error = operations.read_back(TARGET, 0, 0).err().unwrap();
    assert!(
        matches!(
            error,
            OffscreenError::UnreadableFormat { texture, format }
                if texture == TARGET && format == wgpu::TextureFormat::Depth24Plus
        ),
        "{error}"
    );
}

#[test]
fn a_blit_into_a_depth_only_render_texture_is_a_named_error() {
    let mut rig = Rig::new();
    let mut frame = depth_frame(&mut rig, LEFT_ONLY, 32);
    blit_setup(&mut rig, [4, 4]);
    frame.blit_mesh = Some(BLIT_QUAD);
    frame.materials.insert(
        MaterialId(3),
        blit_frame(1.0, 8, [8, 8])
            .materials
            .remove(&MaterialId(1))
            .unwrap(),
    );
    frame.records = vec![Record::Blit {
        source: TextureId(5),
        target: TARGET,
        material: Some(MaterialId(3)),
        pass: 0,
    }];
    let error = rig.build(&frame).err().unwrap();
    assert!(
        matches!(error, OffscreenError::NoColorAttachment(id) if id == TARGET),
        "{error}"
    );
}

#[test]
fn no_color_format_and_no_depth_is_the_named_unsupported_format() {
    let mut rig = Rig::new();
    let frame = depth_frame(&mut rig, LEFT_ONLY, 0);
    let error = rig.build(&frame).err().unwrap();
    assert!(
        matches!(error, OffscreenError::UnsupportedFormat { code: 0 }),
        "{error}"
    );
}

#[test]
fn the_scene_samples_a_depth_only_render_texture_through_its_depth_storage() {
    let mut rig = Rig::new();
    let frame = depth_frame(&mut rig, LEFT_ONLY, 32);
    rig.run(&frame, &[]).unwrap();
    assert!(rig.state.texture_handle(TARGET).is_some());
}

#[test]
fn a_color_texture_with_a_camera_that_has_both_still_writes_its_color() {
    let mut rig = Rig::new();
    let mut frame = fixture(&mut rig, LEFT_ONLY);
    frame.render_textures[0].depth_bits = 24;
    frame.materials.get_mut(&MaterialId(1)).unwrap().properties =
        layer(&[("_Tint", [1.0, 0.0, 0.0, 1.0])]);
    let image = rig.run(&frame, &[(TARGET, 0, 0)]).unwrap().remove(0);
    assert_eq!(pixel(&image, 8, (0, 0)), [255, 0, 0, 255]);
}
