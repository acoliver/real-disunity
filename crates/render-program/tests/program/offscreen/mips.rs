//! A render texture with a mip chain that a camera wrote at its base level and a later draw
//! samples: the engine writes no other level, so the draw reads the levels written at the same
//! point as the base and the frame counts the read. A `GenerateMips` record writes every level
//! from the base, after which the draw reads them all.

use std::collections::BTreeMap;
use worldrunner_render_program::layers::PropertyLayer;

use worldrunner_render_program::ids::{MaterialId, ShaderId, TextureId};
use worldrunner_render_program::unity::offscreen::frame::{
    Attachment, Attachments, Clear, Material, OffscreenFrame, Record, RenderDimension,
};
use worldrunner_render_program::unity::offscreen::{OffscreenError, PartialMipReads};
use worldrunner_render_program::unity::shader::ProgramShader;
use worldrunner_render_program::unity::synthetic::shaders::blit_passes;

use super::camera::fixture;
use super::rig::{
    BLIT_QUAD, BLUE, CLEAR, RED, Rig, blit_quad, image, layer, pixel, source, texture,
};

const BLIT: ShaderId = ShaderId(2);
const CAMERA_TARGET: TextureId = TextureId(10);
const COPY: TextureId = TextureId(11);

/// The camera fixture (red left half over the clear color on an 8x8 target of `mips` levels),
/// then a blit of that target through the identity tint into an 8x8 target of its own.
fn camera_then_blit(rig: &mut Rig, mips: u32) -> OffscreenFrame {
    let mut frame = fixture(rig, 1 << 1);
    frame.render_textures[0].mips = mips;
    frame.render_textures.push(texture(11, [8, 8]));
    let synthetic = blit_passes("_MainTex");
    rig.state.add_shader(
        BLIT,
        ProgramShader::new(synthetic.shader, synthetic.chunks).unwrap(),
    );
    rig.state
        .set_mesh(&mut rig.runtime, BLIT_QUAD, &blit_quad())
        .unwrap();
    frame.blit_mesh = Some(BLIT_QUAD);
    frame.materials.insert(
        MaterialId(3),
        Material {
            shader: Some(BLIT),
            keywords: Vec::new(),
            properties: layer(&[("_Tint", [1.0; 4])]),
            fallback: PropertyLayer::default(),
            below_globals: PropertyLayer::default(),
            render_queue: 2000,
        },
    );
    frame.records.push(Record::Blit {
        source: CAMERA_TARGET,
        target: COPY,
        material: Some(MaterialId(3)),
        pass: 0,
    });
    frame
}

fn halves() -> BTreeMap<&'static str, [u8; 4]> {
    BTreeMap::from([("left", RED), ("right", CLEAR)])
}

#[test]
fn a_draw_reads_the_base_level_of_a_render_texture_whose_other_levels_were_never_written() {
    let mut rig = Rig::new();
    let frame = camera_then_blit(&mut rig, 3);
    let operations = rig.build(&frame).unwrap();
    assert_eq!(operations.partial_mip_reads(), 1);
    drop(operations);
    let image = rig.run(&frame, &[(COPY, 0, 0)]).unwrap().remove(0);
    let expected = halves();
    for y in 0..8 {
        assert_eq!(pixel(&image, 8, (1, y)), expected["left"], "left ({y})");
        assert_eq!(pixel(&image, 8, (6, y)), expected["right"], "right ({y})");
    }
    assert_ne!(expected["left"], BLUE);
}

#[test]
fn a_render_texture_with_one_level_is_read_whole_and_counts_nothing() {
    let mut rig = Rig::new();
    let frame = camera_then_blit(&mut rig, 1);
    assert_eq!(rig.build(&frame).unwrap().partial_mip_reads(), 0);
}

#[test]
fn a_render_texture_nothing_has_written_is_read_whole_and_counts_nothing() {
    let mut rig = Rig::new();
    let mut frame = camera_then_blit(&mut rig, 3);
    // Only the blit remains, so every level of its source is still at its first version.
    frame.records.remove(0);
    assert_eq!(rig.build(&frame).unwrap().partial_mip_reads(), 0);
}

#[test]
fn partial_reads_are_listed_by_the_texture_property_the_levels_and_the_pass_that_read_them() {
    let mut rig = Rig::new();
    let frame = camera_then_blit(&mut rig, 3);
    assert_eq!(
        rig.build(&frame).unwrap().partial_mip_reads_by_pass(),
        vec![PartialMipReads {
            texture: "_MainTex".to_owned(),
            written: 1,
            total: 3,
            key: "index".to_owned(),
            tag: Some("0".to_owned()),
            reads: 1,
        }]
    );
}

/// Clears level `mip` of the camera's target to blue just before the blit, as a pass that builds
/// a mip chain one level at a time writes each level after the base.
fn write_level(frame: &mut OffscreenFrame, mip: u32) {
    let at = frame.records.len() - 1;
    frame.records.insert(
        at,
        Record::Clear {
            target: Attachments::single(
                Some(Attachment {
                    texture: CAMERA_TARGET,
                    mip,
                }),
                None,
            ),
            clear: Clear {
                color: Some([0.0, 0.0, 1.0, 1.0]),
                ..Clear::default()
            },
        },
    );
}

#[test]
fn a_mip_chain_written_one_level_at_a_time_is_read_whole() {
    let mut rig = Rig::new();
    let mut frame = camera_then_blit(&mut rig, 3);
    write_level(&mut frame, 1);
    write_level(&mut frame, 2);
    assert_eq!(rig.build(&frame).unwrap().partial_mip_reads(), 0);
    let image = rig.run(&frame, &[(COPY, 0, 0)]).unwrap().remove(0);
    let expected = halves();
    for y in 0..8 {
        assert_eq!(pixel(&image, 8, (1, y)), expected["left"], "left ({y})");
        assert_eq!(pixel(&image, 8, (6, y)), expected["right"], "right ({y})");
    }
}

#[test]
fn a_mip_chain_read_stops_at_the_first_level_the_frame_never_wrote() {
    let mut rig = Rig::new();
    let mut frame = camera_then_blit(&mut rig, 3);
    write_level(&mut frame, 1);
    let operations = rig.build(&frame).unwrap();
    assert_eq!(operations.partial_mip_reads(), 1);
    assert_eq!(operations.partial_mip_reads_by_pass()[0].written, 2);
}

/// Generates the mips of the camera's target just before the blit reads it.
fn generate_before_blit(frame: &mut OffscreenFrame) {
    let at = frame.records.len() - 1;
    frame.records.insert(
        at,
        Record::GenerateMips {
            texture: CAMERA_TARGET,
        },
    );
}

#[test]
fn a_draw_reads_every_level_of_a_render_texture_whose_mips_were_generated() {
    let mut rig = Rig::new();
    let mut frame = camera_then_blit(&mut rig, 4);
    generate_before_blit(&mut frame);
    let operations = rig.build(&frame).unwrap();
    assert_eq!(operations.partial_mip_reads(), 0);
    assert!(operations.partial_mip_reads_by_pass().is_empty());
    drop(operations);
    let image = rig.run(&frame, &[(COPY, 0, 0)]).unwrap().remove(0);
    let expected = halves();
    for y in 0..8 {
        assert_eq!(pixel(&image, 8, (1, y)), expected["left"], "left ({y})");
        assert_eq!(pixel(&image, 8, (6, y)), expected["right"], "right ({y})");
    }
}

#[test]
fn generated_mips_of_a_texture_written_in_an_earlier_frame_are_read_whole() {
    let mut rig = Rig::new();
    let first = camera_then_blit(&mut rig, 4);
    rig.run(&first, &[]).unwrap();
    // This frame writes no base level: the texture holds what the camera drew last frame.
    let mut frame = first.clone();
    frame.records.remove(0);
    generate_before_blit(&mut frame);
    assert_eq!(rig.build(&frame).unwrap().partial_mip_reads(), 0);
    let image = rig.run(&frame, &[(COPY, 0, 0)]).unwrap().remove(0);
    let expected = halves();
    assert_eq!(pixel(&image, 8, (1, 4)), expected["left"]);
    assert_eq!(pixel(&image, 8, (6, 4)), expected["right"]);
}

const MIPPED: TextureId = TextureId(12);
const FIRST_SOURCE: u64 = 20;

/// The color of texel (x, y) of `layer`'s base level. Every channel is affine in x and y with
/// even slopes, so every 2x2 average of every level of the sides used here is a whole number
/// and exact in 8 bits.
fn pattern(layer: u32) -> impl Fn(u32, u32) -> [u8; 4] {
    let bias = 20 * layer;
    move |x, y| {
        [
            4 * x + 16 * y + bias,
            150 + 8 * y - 4 * x - bias,
            2 * x + 2 * y,
            255,
        ]
        .map(|value| u8::try_from(value).unwrap())
    }
}

/// The 2x2 box average of a square RGBA8 image `side` across, which must divide exactly.
fn box_average(level: &[u8], side: u32) -> Vec<u8> {
    let half = (side / 2).max(1);
    let texel =
        |x: u32, y: u32, channel: usize| u32::from(level[((y * side + x) * 4) as usize + channel]);
    let mut next = Vec::new();
    for y in 0..half {
        for x in 0..half {
            for channel in 0..4 {
                let sum: u32 = (0..2)
                    .flat_map(|dy| (0..2).map(move |dx| (dx, dy)))
                    .map(|(dx, dy)| texel(2 * x + dx, 2 * y + dy, channel))
                    .sum();
                assert_eq!(sum % 4, 0, "the pattern averages exactly at ({x}, {y})");
                next.push(u8::try_from(sum / 4).unwrap());
            }
        }
    }
    next
}

/// Every level of `layer`'s pattern on a texture `side` across, from the base.
fn chain(layer: u32, side: u32, mips: u32) -> Vec<Vec<u8>> {
    let mut levels = vec![image([side, side], pattern(layer))];
    for mip in 1..mips {
        let above = &levels[mip as usize - 1];
        let next = box_average(above, (side >> (mip - 1)).max(1));
        levels.push(next);
    }
    levels
}

/// A frame that uploads one pattern for each element of a render texture of `dimension`, copies
/// them into its base level and generates the other levels.
fn generated(rig: &mut Rig, dimension: RenderDimension, side: u32, mips: u32) -> OffscreenFrame {
    let mut spec = texture(12, [side, side]);
    spec.dimension = dimension;
    spec.mips = mips;
    let mut records = Vec::new();
    for layer in 0..dimension.layers() {
        let id = FIRST_SOURCE + u64::from(layer);
        let bytes = image([side, side], pattern(layer));
        rig.state
            .set_texture(&mut rig.runtime, &source(id, [side, side], bytes))
            .unwrap();
        records.push(Record::CopyTexture {
            source: TextureId(id),
            source_element: 0,
            source_mip: 0,
            target: MIPPED,
            target_element: layer,
            target_mip: 0,
        });
    }
    records.push(Record::GenerateMips { texture: MIPPED });
    OffscreenFrame {
        render_textures: vec![spec],
        records,
        ..OffscreenFrame::default()
    }
}

/// Reads back every level of every element of the generated texture and compares each with the
/// box averages of that element's own pattern.
fn assert_chains(rig: &mut Rig, frame: &OffscreenFrame, (side, mips): (u32, u32), layers: u32) {
    let reads: Vec<_> = (0..mips)
        .flat_map(|mip| (0..layers).map(move |layer| (MIPPED, mip, layer)))
        .collect();
    let images = rig.run(frame, &reads).unwrap();
    for (&(_, mip, layer), bytes) in reads.iter().zip(&images) {
        let expected = &chain(layer, side, mips)[mip as usize];
        let level = (side >> mip).max(1);
        assert_eq!(bytes.len(), (level * level * 4) as usize, "mip {mip} size");
        assert_eq!(bytes, expected, "mip {mip} of element {layer}");
    }
}

#[test]
fn each_level_is_the_exact_2x2_average_of_the_level_above_it() {
    let mut rig = Rig::new();
    let frame = generated(&mut rig, RenderDimension::D2, 8, 4);
    assert_chains(&mut rig, &frame, (8, 4), 1);
    // The 1x1 level is the average of the whole base.
    let base = chain(0, 8, 1).remove(0);
    let sum: u32 = base.chunks(4).map(|texel| u32::from(texel[0])).sum();
    let last = rig.run(&frame, &[(MIPPED, 3, 0)]).unwrap().remove(0);
    assert_eq!(u32::from(last[0]), sum / 64);
}

#[test]
fn each_layer_of_an_array_is_generated_from_its_own_base() {
    let mut rig = Rig::new();
    let frame = generated(&mut rig, RenderDimension::D2Array(3), 8, 4);
    assert_chains(&mut rig, &frame, (8, 4), 3);
    assert_ne!(chain(0, 8, 2)[1], chain(1, 8, 2)[1], "the layers differ");
}

#[test]
fn each_face_of_a_cube_is_generated_from_its_own_base() {
    let mut rig = Rig::new();
    let frame = generated(&mut rig, RenderDimension::Cube, 4, 3);
    assert_chains(&mut rig, &frame, (4, 3), 6);
}

fn refused(rig: &mut Rig, frame: &OffscreenFrame) -> OffscreenError {
    rig.build(frame).err().expect("the frame is refused")
}

#[test]
fn a_format_that_is_not_filterable_float_or_unorm_color_is_refused_by_name() {
    // The test device does not filter 32-bit float, which is 49 (one channel) and 52 (four).
    for code in [49, 52] {
        let mut rig = Rig::new();
        let mut frame = generated(&mut rig, RenderDimension::D2, 4, 3);
        frame.render_textures[0].graphics_format = code;
        frame.records.remove(0);
        let error = refused(&mut rig, &frame);
        assert!(
            matches!(&error, OffscreenError::MipsOfFormat { texture, format }
                if *texture == MIPPED
                    && matches!(format, wgpu::TextureFormat::R32Float | wgpu::TextureFormat::Rgba32Float)),
            "{code}: {error}"
        );
    }
}

#[test]
fn a_depth_only_render_texture_is_refused_by_name() {
    let mut rig = Rig::new();
    let mut frame = generated(&mut rig, RenderDimension::D2, 4, 3);
    frame.render_textures[0].graphics_format = 0;
    frame.render_textures[0].depth_bits = 24;
    frame.records = vec![Record::GenerateMips { texture: MIPPED }];
    let error = refused(&mut rig, &frame);
    assert!(
        matches!(&error, OffscreenError::MipsOfFormat { texture, format }
            if *texture == MIPPED && *format == wgpu::TextureFormat::Depth24Plus),
        "{error}"
    );
}

#[test]
fn a_3d_render_texture_is_refused_by_name() {
    let mut rig = Rig::new();
    let mut frame = generated(&mut rig, RenderDimension::D2, 4, 1);
    frame.render_textures[0].dimension = RenderDimension::D3(4);
    frame.records = vec![Record::GenerateMips { texture: MIPPED }];
    let error = refused(&mut rig, &frame);
    assert!(
        matches!(&error, OffscreenError::MipsOfVolume(texture) if *texture == MIPPED),
        "{error}"
    );
}

#[test]
fn an_uploaded_texture_is_not_a_render_texture_whose_mips_can_be_generated() {
    let mut rig = Rig::new();
    let mut frame = generated(&mut rig, RenderDimension::D2, 4, 3);
    frame.records = vec![Record::GenerateMips {
        texture: TextureId(FIRST_SOURCE),
    }];
    let error = refused(&mut rig, &frame);
    assert!(
        matches!(&error, OffscreenError::NotRenderTexture(texture) if texture.0 == FIRST_SOURCE),
        "{error}"
    );
}

#[test]
fn a_texture_of_one_level_has_none_to_generate() {
    let mut rig = Rig::new();
    let frame = generated(&mut rig, RenderDimension::D2, 4, 1);
    assert_chains(&mut rig, &frame, (4, 1), 1);
}
