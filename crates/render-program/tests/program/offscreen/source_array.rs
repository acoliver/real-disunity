//! A 2D array source: the host uploads it once, a material names it for a texture array property
//! and a draw samples one of its layers through an array view, even when there is one layer.

use std::collections::BTreeMap;

use worldrunner_render_program::error::ProgramError;
use worldrunner_render_program::ids::{MaterialId, ShaderId, TextureId};
use worldrunner_render_program::layers::{PropertyLayer, TextureInput};
use worldrunner_render_program::unity::offscreen::OffscreenError;
use worldrunner_render_program::unity::offscreen::frame::{
    Material, OffscreenFrame, Record, RenderDimension, SourceDimension, SourceTexture,
};
use worldrunner_render_program::unity::shader::ProgramShader;
use worldrunner_render_program::unity::synthetic::shaders::{
    ARRAY_PATCH, array_patch_parameters, no_cull, variant,
};
use worldrunner_render_program::unity::synthetic::{PassSpec, shader};

use super::formats::{blit_frame, blit_setup};
use super::rig::{
    BLIT_QUAD, BLUE, CYAN, GREEN, RED, RGBA8, Rig, blit_quad, pixel, source, texture,
};

const SHADER: ShaderId = ShaderId(2);
const SOURCE: TextureId = TextureId(5);
const LAYERS: TextureId = TextureId(7);
const TARGET: TextureId = TextureId(11);

/// An array source of `size` texels a layer whose layer `n` is all of `layers[n]`.
fn array(id: TextureId, size: [u32; 2], layers: &[[u8; 4]]) -> SourceTexture {
    let bytes = layers
        .iter()
        .flat_map(|texel| texel.repeat((size[0] * size[1]) as usize))
        .collect();
    SourceTexture {
        id,
        dimension: SourceDimension::D2Array(u32::try_from(layers.len()).unwrap()),
        size,
        graphics_format: RGBA8,
        bytes,
        mips: 1,
        sampler: None,
    }
}

/// The vertex stage and bindings of the blit passes that read the array `_Layers`.
const ARRAY_HEAD: &str = "
@group(0) @binding(0) var layers: texture_2d_array<f32>;
@group(0) @binding(1) var layers_sampler: sampler;
struct VOut { @builtin(position) position: vec4<f32>, @location(0) uv: vec2<f32> }
@vertex fn vs_main(@location(0) position: vec3<f32>, @location(1) uv: vec2<f32>) -> VOut {
    var out: VOut;
    out.position = vec4<f32>(position, 1.0);
    out.uv = uv;
    return out;
}";

/// Reads level `column % 2` of layer `column / 2` of `_Layers`, where `column` is the column of a
/// four-column target.
const LEVELS: &str = "
@fragment fn fs_levels(in: VOut) -> @location(0) vec4<f32> {
    let column = i32(in.uv.x * 4.0);
    return textureSampleLevel(layers, layers_sampler, in.uv, column / 2, f32(column % 2));
}";

/// Reads the layer of `_Layers` that the half of the target names.
const HALVES: &str = "
@fragment fn fs_halves(in: VOut) -> @location(0) vec4<f32> {
    return textureSample(layers, layers_sampler, in.uv, i32(in.uv.x * 2.0));
}";

/// An array source of `size` texels a layer with a mip chain, whose layer `n` is all of
/// `levels[n][l]` on level `l`, one layer's levels after the other.
fn mipped(id: TextureId, size: [u32; 2], levels: &[&[[u8; 4]]]) -> SourceTexture {
    let bytes = levels
        .iter()
        .flat_map(|layer| {
            layer.iter().enumerate().flat_map(move |(level, texel)| {
                let texels = (size[0] >> level).max(1) * (size[1] >> level).max(1);
                texel.repeat(texels as usize)
            })
        })
        .collect();
    SourceTexture {
        mips: u32::try_from(levels[0].len()).unwrap(),
        ..array_of_layers(id, size, levels.len(), bytes)
    }
}

fn array_of_layers(id: TextureId, size: [u32; 2], layers: usize, bytes: Vec<u8>) -> SourceTexture {
    SourceTexture {
        id,
        dimension: SourceDimension::D2Array(u32::try_from(layers).unwrap()),
        size,
        graphics_format: RGBA8,
        mips: 1,
        bytes,
        sampler: None,
    }
}

fn setup(rig: &mut Rig) {
    setup_with(rig, ARRAY_PATCH, "fs_layer");
}

/// The head and `fragment` as the blit shader, whose fragment entry point is `entry`.
fn setup_blit(rig: &mut Rig, fragment: &str, entry: &'static str) {
    setup_with(rig, &format!("{ARRAY_HEAD}{fragment}"), entry);
}

fn setup_with(rig: &mut Rig, wgsl: &str, entry: &'static str) {
    let synthetic = no_cull(shader(
        &[],
        &[PassSpec {
            tags: Vec::new(),
            variants: vec![variant(&[], wgsl, entry, array_patch_parameters())],
        }],
    ));
    rig.state.add_shader(
        SHADER,
        ProgramShader::new(synthetic.shader, synthetic.chunks).unwrap(),
    );
    rig.state
        .set_mesh(&mut rig.runtime, BLIT_QUAD, &blit_quad())
        .unwrap();
    rig.state
        .set_texture(&mut rig.runtime, &source(5, [1, 1], vec![255; 4]))
        .unwrap();
}

/// A blit of the shader whose material sets the `_Layers` array property to `LAYERS`.
fn frame() -> OffscreenFrame {
    let mut properties = PropertyLayer::default();
    properties.textures.insert(
        "_Layers".to_owned(),
        TextureInput {
            texture: LAYERS,
            sampler: None,
        },
    );
    OffscreenFrame {
        blit_mesh: Some(BLIT_QUAD),
        render_textures: vec![texture(11, [4, 4])],
        materials: BTreeMap::from([(
            MaterialId(1),
            Material {
                shader: Some(SHADER),
                keywords: Vec::new(),
                properties,
                fallback: PropertyLayer::default(),
                below_globals: PropertyLayer::default(),
                render_queue: 2000,
            },
        )]),
        records: vec![Record::Blit {
            source: SOURCE,
            target: TARGET,
            material: Some(MaterialId(1)),
            pass: 0,
        }],
        ..OffscreenFrame::default()
    }
}

#[test]
fn a_draw_samples_the_layer_it_asks_for_of_an_array_source() {
    let mut rig = Rig::new();
    setup(&mut rig);
    // The shader reads element 2.
    let layers = [
        [10, 0, 0, 255],
        [0, 20, 0, 255],
        [0, 0, 30, 255],
        [40, 40, 40, 255],
    ];
    rig.state
        .set_texture(&mut rig.runtime, &array(LAYERS, [2, 2], &layers))
        .unwrap();
    let result = rig.run(&frame(), &[(TARGET, 0, 0)]).unwrap().remove(0);
    for (x, y) in [(0, 0), (3, 0), (1, 2), (3, 3)] {
        assert_eq!(pixel(&result, 4, (x, y)), [0, 0, 30, 255], "({x}, {y})");
    }
}

/// The shape a default of one layer has: the state holds it, and a draw binds it, as an array.
#[test]
fn a_one_layer_array_is_held_as_an_array_of_one_layer() {
    let mut rig = Rig::new();
    rig.state
        .set_texture(&mut rig.runtime, &array(LAYERS, [1, 1], &[[1, 2, 3, 4]]))
        .unwrap();
    assert_eq!(
        rig.state.texture_dimension(LAYERS),
        Some(RenderDimension::D2Array(1))
    );
    assert_eq!(
        rig.state.texture_view_dimension(LAYERS),
        Some(wgpu::TextureViewDimension::D2Array)
    );
}

#[test]
fn an_array_source_whose_bytes_are_not_its_layers_is_refused() {
    let mut rig = Rig::new();
    let mut short = array(LAYERS, [2, 2], &[[1, 2, 3, 4]; 3]);
    short.bytes.truncate(2 * 2 * 4 * 2);
    let error = rig
        .state
        .set_texture(&mut rig.runtime, &short)
        .expect_err("two layers of bytes are not three");
    assert!(
        matches!(
            error,
            OffscreenError::Program(ProgramError::TextureData { .. })
        ),
        "{error}"
    );
}

#[test]
fn an_array_source_of_no_layers_is_refused() {
    let mut rig = Rig::new();
    let empty = SourceTexture {
        id: LAYERS,
        dimension: SourceDimension::D2Array(0),
        size: [1, 1],
        graphics_format: RGBA8,
        bytes: Vec::new(),
        mips: 1,
        sampler: None,
    };
    assert!(rig.state.set_texture(&mut rig.runtime, &empty).is_err());
}

#[test]
fn a_draw_samples_each_level_of_each_layer_of_a_mipped_array() {
    let mut rig = Rig::new();
    setup_blit(&mut rig, LEVELS, "fs_levels");
    // Layer 0 holds red then green, layer 1 blue then cyan: a wrong layer or level stride shows
    // as another one of the four.
    let levels = mipped(LAYERS, [2, 2], &[&[RED, GREEN], &[BLUE, CYAN]]);
    rig.state.set_texture(&mut rig.runtime, &levels).unwrap();
    let result = rig.run(&frame(), &[(TARGET, 0, 0)]).unwrap().remove(0);
    for (column, expected) in [RED, GREEN, BLUE, CYAN].into_iter().enumerate() {
        for y in 0..4 {
            let at = (u32::try_from(column).unwrap(), y);
            assert_eq!(pixel(&result, 4, at), expected, "{at:?}");
        }
    }
}

#[test]
fn a_mipped_array_holds_and_gives_back_the_bytes_of_every_level() {
    let mut rig = Rig::new();
    let before = rig.runtime.live_bytes();
    let levels = mipped(LAYERS, [4, 4], &[&[RED, GREEN, BLUE], &[CYAN, RED, GREEN]]);
    // 4 x 4, 2 x 2 and 1 x 1 texels of 4 bytes, in each of two layers.
    let bytes = 2 * (16 + 4 + 1) * 4;
    assert_eq!(levels.bytes.len(), bytes);
    rig.state.set_texture(&mut rig.runtime, &levels).unwrap();
    assert_eq!(rig.runtime.live_bytes(), before + bytes as u64);
    assert_eq!(
        rig.state.release_source_texture(&mut rig.runtime, LAYERS),
        Some(bytes as u64)
    );
    rig.runtime.collect();
    assert_eq!(rig.runtime.live_bytes(), before);
}

#[test]
fn a_mipped_array_whose_bytes_or_levels_do_not_fit_its_chain_is_refused() {
    let mut rig = Rig::new();
    let whole = mipped(LAYERS, [2, 2], &[&[RED, GREEN], &[BLUE, CYAN]]);
    let mut short = whole.clone();
    short.bytes.truncate(20 + 16);
    for (name, source) in [
        ("the second layer without its second level", short),
        (
            "the chain read as the base level alone",
            SourceTexture {
                mips: 1,
                ..whole.clone()
            },
        ),
        (
            "three levels of a 2x2 texture",
            SourceTexture {
                mips: 3,
                ..whole.clone()
            },
        ),
        ("no levels", SourceTexture { mips: 0, ..whole }),
    ] {
        assert!(
            rig.state.set_texture(&mut rig.runtime, &source).is_err(),
            "{name}"
        );
    }
}

/// `GraphicsFormat`s of DXT1 and DXT5 in sRGB and linear.
const BC1: [u32; 2] = [96, 97];
const BC3: [u32; 2] = [100, 101];
/// RGB565 colours whose channels are all zero or full, so that they sample to the same texels in
/// sRGB and linear and decode to the same bytes on any device.
const RED_565: u16 = 0xF800;
const GREEN_565: u16 = 0x07E0;
const BLUE_565: u16 = 0x001F;
const CYAN_565: u16 = 0x07FF;
const YELLOW_565: u16 = 0xFFE0;
const MAGENTA_565: u16 = 0xF81F;

/// Makes a block of one RGB565 colour.
type Block = fn(u16) -> Vec<u8>;

/// A DXT1 block whose sixteen texels are all `color`: both endpoints are it and every index is 0.
fn bc1_block(color: u16) -> Vec<u8> {
    let [low, high] = color.to_le_bytes();
    vec![low, high, low, high, 0, 0, 0, 0]
}

/// A DXT5 block of `color` and the alpha 255 in both alpha endpoints.
fn bc3_block(color: u16) -> Vec<u8> {
    [vec![255, 255, 0, 0, 0, 0, 0, 0], bc1_block(color)].concat()
}

/// A block-compressed array of 4x4 layers with `mips` levels, whose bytes are `layers`, the blocks
/// of each layer's levels one after the other.
fn blocks(id: TextureId, graphics_format: u32, mips: u32, layers: &[Vec<u8>]) -> SourceTexture {
    SourceTexture {
        graphics_format,
        mips,
        ..array_of_layers(id, [4, 4], layers.len(), layers.concat())
    }
}

fn require_blocks(rig: &Rig) {
    assert!(
        rig.gpu
            .device
            .features()
            .contains(wgpu::Features::TEXTURE_COMPRESSION_BC),
        "the adapter must offer block compression for this test"
    );
}

fn assert_columns(image: &[u8], columns: [[u8; 4]; 4], what: &str) {
    for (column, expected) in columns.into_iter().enumerate() {
        for y in 0..4 {
            let at = (u32::try_from(column).unwrap(), y);
            assert_eq!(pixel(image, 4, at), expected, "{what} {at:?}");
        }
    }
}

#[test]
fn a_draw_samples_each_layer_of_a_block_compressed_array() {
    let mut rig = Rig::new();
    require_blocks(&rig);
    setup_blit(&mut rig, HALVES, "fs_halves");
    let kinds: [(u32, Block); 4] = [
        (BC1[0], bc1_block),
        (BC1[1], bc1_block),
        (BC3[0], bc3_block),
        (BC3[1], bc3_block),
    ];
    for (format, block) in kinds {
        let array = blocks(LAYERS, format, 1, &[block(RED_565), block(BLUE_565)]);
        rig.state.set_texture(&mut rig.runtime, &array).unwrap();
        let image = rig.run(&frame(), &[(TARGET, 0, 0)]).unwrap().remove(0);
        assert_columns(&image, [RED, RED, BLUE, BLUE], &format!("format {format}"));
    }
}

#[test]
fn a_block_compressed_array_with_a_full_chain_samples_each_level_of_each_layer() {
    let mut rig = Rig::new();
    require_blocks(&rig);
    setup_blit(&mut rig, LEVELS, "fs_levels");
    // 4x4, 2x2 and 1x1 texels each take one block, so a layer is three blocks and the second
    // layer starts after the third.
    let kinds: [(u32, Block); 2] = [(BC1[0], bc1_block), (BC3[0], bc3_block)];
    for (format, block) in kinds {
        let chain = |colors: [u16; 3]| colors.iter().flat_map(|color| block(*color)).collect();
        let layers = [
            chain([RED_565, GREEN_565, YELLOW_565]),
            chain([BLUE_565, CYAN_565, MAGENTA_565]),
        ];
        let array = blocks(LAYERS, format, 3, &layers);
        rig.state.set_texture(&mut rig.runtime, &array).unwrap();
        let image = rig.run(&frame(), &[(TARGET, 0, 0)]).unwrap().remove(0);
        assert_columns(
            &image,
            [RED, GREEN, BLUE, CYAN],
            &format!("format {format}"),
        );
    }
}

#[test]
fn a_block_array_whose_bytes_are_not_its_blocks_is_refused() {
    let mut rig = Rig::new();
    require_blocks(&rig);
    // Two layers of one block each hold 16 bytes of DXT1, and 17 are not.
    let mut array = blocks(
        LAYERS,
        BC1[0],
        1,
        &[bc1_block(RED_565), bc1_block(BLUE_565)],
    );
    array.bytes.push(0);
    assert!(rig.state.set_texture(&mut rig.runtime, &array).is_err());
}

/// A blit of the 2D source 5, which the draw reads through the 2D slot `_MainTex`, whose bytes are
/// `layers` layers of 4x4 red texels.
fn red_array_source(layers: usize) -> SourceTexture {
    array_of_layers(TextureId(5), [4, 4], layers, RED.repeat(16 * layers))
}

#[test]
fn a_2d_slot_over_an_array_of_several_layers_is_refused_by_name() {
    let mut rig = Rig::new();
    blit_setup(&mut rig, [4, 4]);
    rig.state
        .set_texture(&mut rig.runtime, &red_array_source(2))
        .unwrap();
    let error = rig
        .run(&blit_frame(1.0, RGBA8, [4, 4]), &[(TextureId(11), 0, 0)])
        .unwrap_err();
    assert!(
        matches!(error, OffscreenError::FlatSlotOverArray { layers: 2, .. }),
        "{error}"
    );
    let message = error.to_string();
    assert!(
        message.contains("_MainTex") && message.contains("2D array of 2 layers"),
        "{message}"
    );
}

#[test]
fn a_2d_slot_over_an_array_of_one_layer_samples_it() {
    let mut rig = Rig::new();
    blit_setup(&mut rig, [4, 4]);
    rig.state
        .set_texture(&mut rig.runtime, &red_array_source(1))
        .unwrap();
    let image = rig
        .run(&blit_frame(1.0, RGBA8, [4, 4]), &[(TextureId(11), 0, 0)])
        .unwrap()
        .remove(0);
    assert_columns(&image, [RED; 4], "one layer");
}
