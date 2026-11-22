//! The raster state a script sets for a record's draws: a scissor rectangle clips them to its
//! pixels of the attachments, and a depth bias moves the depth they write by the constant and
//! slope-scaled terms.
//!
//! The vertex stage reads no vertex input. It makes two triangles that cover the whole target
//! with a depth that rises from 0.25 at the left edge to 0.75 at the right edge, so the depth
//! slope is 0.5 over the target's 8 texels.

use std::collections::BTreeMap;
use std::sync::Arc;

use worldrunner_render_program::ids::{MaterialId, ShaderId, TextureId};
use worldrunner_render_program::layers::PropertyLayer;
use worldrunner_render_program::unity::offscreen::frame::{
    DrawRaster, OffscreenFrame, ProceduralDraw, Record,
};
use worldrunner_render_program::unity::shader::PassSelector;
use worldrunner_render_program::unity::synthetic::fixed;
use worldrunner_unity_serialized::shader_state::ShaderState;

use super::procedural::{clear, generated_from, material, whole};
use super::rig::{BLUE, CLEAR, Rig, pixel, texture};

const TARGET: TextureId = TextureId(10);
const SHADER: ShaderId = ShaderId(1);
const SIZE: u32 = 8;
const SIDE: f32 = 8.0;
const NO_COLOR: u32 = 0;
/// The depth the slanted triangles write at column `x`, whose centre is at `(x + 0.5) / SIZE`.
fn slanted_depth(x: u32) -> f32 {
    0.25 + 0.5 * (f32::from(u16::try_from(x).unwrap()) + 0.5) / SIDE
}
/// The depth slope per texel the slanted triangles have.
const SLOPE: f32 = 0.5 / SIDE;

const WGSL: &str = "
struct Material { tint: vec4<f32> }
@group(0) @binding(0) var<uniform> material: Material;
@vertex fn vs_main(@builtin(vertex_index) v: u32) -> @builtin(position) vec4<f32> {
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, -1.0), vec2<f32>(-1.0, 1.0),
        vec2<f32>(-1.0, 1.0), vec2<f32>(1.0, -1.0), vec2<f32>(1.0, 1.0));
    let corner = corners[v];
    return vec4<f32>(corner, 0.25 + 0.25 * (corner.x + 1.0), 1.0);
}
@fragment fn fs_main() -> @location(0) vec4<f32> { return material.tint; }";

/// `ZTest Always` and `ZWrite On`, so the depth every draw writes is the depth it makes.
fn always_writes(state: &mut ShaderState) {
    state.depth.test = fixed(8.0);
    state.depth.write = fixed(1.0);
}

fn draw(raster: DrawRaster, topology: wgpu::PrimitiveTopology) -> Record {
    Record::Procedural(Arc::new(ProceduralDraw {
        target: whole(TARGET),
        raster,
        values: PropertyLayer::default(),
        per_draw: PropertyLayer::default(),
        front_face: wgpu::FrontFace::Cw,
        material: MaterialId(1),
        passes: vec![PassSelector::Index(0)],
        topology,
        vertices: 6,
        instances: 1,
    }))
}

/// A frame of one clear and one draw into a target of `format` with 32 depth bits.
fn frame(format: u32, raster: DrawRaster) -> OffscreenFrame {
    let mut target = texture(10, [SIZE, SIZE]);
    target.graphics_format = format;
    target.depth_bits = 32;
    OffscreenFrame {
        render_textures: vec![target],
        materials: BTreeMap::from([(
            MaterialId(1),
            material(SHADER, BLUE.map(|c| f32::from(c) / 255.0), 0.0),
        )]),
        records: vec![
            clear(CLEAR.map(|c| f64::from(c) / 255.0)),
            draw(raster, wgpu::PrimitiveTopology::TriangleList),
        ],
        ..OffscreenFrame::default()
    }
}

fn rig() -> Rig {
    let mut rig = Rig::new();
    rig.state
        .add_shader(SHADER, generated_from(WGSL, false, always_writes));
    rig
}

fn colors(raster: DrawRaster) -> Vec<u8> {
    let frame = frame(super::rig::RGBA8, raster);
    rig().run(&frame, &[(TARGET, 0, 0)]).unwrap().remove(0)
}

/// The depths of the target of a depth-only frame, row by row from row 0.
fn depths(raster: DrawRaster) -> Vec<f32> {
    let mut rig = rig();
    let frame = frame(NO_COLOR, raster);
    let bytes = rig.run(&frame, &[(TARGET, 0, 0)]).unwrap().remove(0);
    bytes
        .chunks(4)
        .map(|float| f32::from_le_bytes(float.try_into().unwrap()))
        .collect()
}

fn biased(bias: [f32; 2]) -> DrawRaster {
    DrawRaster {
        depth_bias: bias,
        ..DrawRaster::default()
    }
}

/// Whether the texel at column `x`, row `y` is drawn blue rather than cleared.
fn drawn(image: &[u8], x: u32, y: u32) -> bool {
    match pixel(image, SIZE, (x, y)) {
        BLUE => true,
        CLEAR => false,
        other => panic!("({x}, {y}) is {other:?}"),
    }
}

#[test]
fn without_a_scissor_the_draw_covers_the_whole_target() {
    let image = colors(DrawRaster::default());
    assert!((0..SIZE).all(|x| (0..SIZE).all(|y| drawn(&image, x, y))));
}

#[test]
fn a_scissor_clips_the_draw_to_its_texels_counted_from_row_and_column_zero() {
    let image = colors(DrawRaster {
        scissor: Some([2.0, 1.0, 4.0, 3.0]),
        ..DrawRaster::default()
    });
    for x in 0..SIZE {
        for y in 0..SIZE {
            let inside = (2..6).contains(&x) && (1..4).contains(&y);
            assert_eq!(drawn(&image, x, y), inside, "({x}, {y})");
        }
    }
}

#[test]
fn the_part_of_a_scissor_outside_the_target_is_dropped() {
    let image = colors(DrawRaster {
        scissor: Some([-4.0, -4.0, 8.0, 8.0]),
        ..DrawRaster::default()
    });
    for x in 0..SIZE {
        for y in 0..SIZE {
            assert_eq!(drawn(&image, x, y), x < 4 && y < 4, "({x}, {y})");
        }
    }
    let image = colors(DrawRaster {
        scissor: Some([6.0, 6.0, 100.0, 100.0]),
        ..DrawRaster::default()
    });
    for x in 0..SIZE {
        for y in 0..SIZE {
            assert_eq!(drawn(&image, x, y), x >= 6 && y >= 6, "({x}, {y})");
        }
    }
}

#[test]
fn a_scissor_with_no_pixel_left_draws_nothing() {
    for rect in [
        [3.0, 3.0, 0.0, 4.0],
        [3.0, 3.0, 4.0, 0.0],
        [8.0, 0.0, 4.0, 4.0],
        [-9.0, 0.0, 4.0, 4.0],
    ] {
        let image = colors(DrawRaster {
            scissor: Some(rect),
            ..DrawRaster::default()
        });
        assert!(image.chunks(4).all(|texel| texel == CLEAR), "{rect:?}");
    }
}

#[test]
fn a_slope_bias_moves_the_depth_a_draw_writes_by_the_slope_times_the_factor() {
    let plain = depths(DrawRaster::default());
    for (index, depth) in plain.iter().enumerate() {
        let expected = slanted_depth(u32::try_from(index).unwrap() % SIZE);
        assert!(
            (depth - expected).abs() < 0.002,
            "texel {index}: {depth} vs {expected}"
        );
    }
    let moved = depths(biased([0.0, 2.0]));
    for (index, (depth, base)) in moved.iter().zip(&plain).enumerate() {
        let shift = depth - base;
        assert!(
            (shift + 2.0 * SLOPE).abs() < 0.01,
            "texel {index}: shifted by {shift}, expected {}",
            -2.0 * SLOPE
        );
    }
}

#[test]
fn a_constant_bias_moves_the_stored_depth_away_from_the_viewer_which_is_toward_zero() {
    let plain = depths(DrawRaster::default());
    let moved = depths(biased([1_048_576.0, 0.0]));
    assert!(
        moved.iter().zip(&plain).all(|(m, p)| p - m > 0.01),
        "{moved:?}"
    );
}

#[test]
fn a_line_list_is_drawn_with_a_bias_set_and_without_it() {
    let mut frame = frame(super::rig::RGBA8, DrawRaster::default());
    frame.records[1] = draw(biased([1.0, 1.0]), wgpu::PrimitiveTopology::LineList);
    let mut rig = rig();
    rig.run(&frame, &[(TARGET, 0, 0)]).unwrap();
}

#[test]
fn a_viewport_maps_clip_space_to_its_rectangle_of_the_target() {
    let image = colors(DrawRaster {
        viewport: Some([2.0, 1.0, 4.0, 3.0]),
        ..DrawRaster::default()
    });
    for x in 0..SIZE {
        for y in 0..SIZE {
            let inside = (2..6).contains(&x) && (1..4).contains(&y);
            assert_eq!(drawn(&image, x, y), inside, "({x}, {y})");
        }
    }
}

#[test]
fn a_scissor_clips_the_draw_inside_a_viewport() {
    let image = colors(DrawRaster {
        viewport: Some([2.0, 2.0, 4.0, 4.0]),
        scissor: Some([0.0, 0.0, 4.0, 4.0]),
        ..DrawRaster::default()
    });
    for x in 0..SIZE {
        for y in 0..SIZE {
            let inside = (2..4).contains(&x) && (2..4).contains(&y);
            assert_eq!(drawn(&image, x, y), inside, "({x}, {y})");
        }
    }
}

#[test]
fn a_viewport_that_is_not_inside_the_target_or_has_no_area_draws_nothing() {
    for rect in [
        [-1.0, 0.0, 4.0, 4.0],
        [6.0, 6.0, 4.0, 4.0],
        [0.0, 0.0, 0.0, 4.0],
        [2.0, 2.0, 4.0, 0.0],
        [0.0, 0.0, 9.0, 4.0],
    ] {
        let image = colors(DrawRaster {
            viewport: Some(rect),
            ..DrawRaster::default()
        });
        assert!(image.chunks(4).all(|texel| texel == CLEAR), "{rect:?}");
    }
}

/// `Blend SrcAlpha OneMinusSrcAlpha` over the always-writing depth state. One `Blend` line sets
/// the colour and the alpha factors alike.
fn alpha_blended(state: &mut ShaderState) {
    always_writes(state);
    state.blend[0].src = fixed(5.0);
    state.blend[0].dst = fixed(10.0);
    state.blend[0].src_alpha = fixed(5.0);
    state.blend[0].dst_alpha = fixed(10.0);
}

/// The texel at the middle of a target cleared to transparent after one draw of `tint` whose pass
/// has the state `pass`.
fn over_transparent(tint: [f32; 4], pass: fn(&mut ShaderState), coverage_alpha: bool) -> [u8; 4] {
    let mut rig = Rig::new();
    rig.state
        .add_shader(SHADER, generated_from(WGSL, false, pass));
    let mut frame = frame(
        super::rig::RGBA8,
        DrawRaster {
            coverage_alpha,
            ..DrawRaster::default()
        },
    );
    frame
        .materials
        .insert(MaterialId(1), material(SHADER, tint, 0.0));
    frame.records[0] = clear([0.0; 4]);
    let image = rig.run(&frame, &[(TARGET, 0, 0)]).unwrap().remove(0);
    pixel(&image, SIZE, (3, 3))
}

fn near(found: [u8; 4], expected: [u8; 4]) {
    for (channel, (found, expected)) in found.iter().zip(expected).enumerate() {
        assert!(
            found.abs_diff(expected) <= 1,
            "channel {channel}: {found} against {expected}"
        );
    }
}

#[test]
fn a_source_alpha_blend_leaves_the_target_alpha_as_the_alpha_squared_without_coverage() {
    // The pass's own blend scales the alpha channel by the alpha as it does the colour.
    near(
        over_transparent([1.0, 0.0, 0.0, 0.5], alpha_blended, false),
        [128, 0, 0, 64],
    );
}

#[test]
fn a_source_alpha_blend_with_coverage_alpha_leaves_the_alpha_as_the_coverage() {
    near(
        over_transparent([1.0, 0.0, 0.0, 0.5], alpha_blended, true),
        [128, 0, 0, 128],
    );
}

#[test]
fn coverage_alpha_keeps_the_alpha_blend_of_a_pass_that_is_not_a_source_alpha_blend() {
    // An opaque pass writes its colour and its alpha as they are, whatever the record asks.
    for coverage_alpha in [false, true] {
        near(
            over_transparent([1.0, 0.0, 0.0, 0.5], always_writes, coverage_alpha),
            [255, 0, 0, 128],
        );
    }
}
