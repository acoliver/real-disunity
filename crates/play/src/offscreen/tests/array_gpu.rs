//! A stored 2D array texture drawn through the synchronous flush on a real adapter (lavapipe in
//! CI): the managed engine stores the layers of an array in one byte string under one identity,
//! the host uploads them as one layered texture, and a draw reads each layer by its index.
//!
//! The shader is a synthetic blit pass that reads layer `i32(uv.x * 4.0)` of the array property
//! `_Layers`, so column `c` of a four-column target shows layer `c`. Every expected texel is the
//! color stored for that layer, not an output of the renderer.

use std::collections::BTreeMap;

use worldrunner_disunity_runtime::{MeshChannels, PlayerTexture, TextureDimension, TextureSampler};
use worldrunner_render_program::unity::synthetic::shaders::{
    array_patch_parameters, no_cull, variant,
};
use worldrunner_render_program::unity::synthetic::{
    PassSpec, SyntheticShader, shader as synthetic_shader,
};
use worldrunner_world_simulation::graphics::{OffscreenWork, PropertyValue, TargetRef, TextureRef};

use super::super::assets::{AssetError, Assets};
use super::super::flush::OffscreenHost;
use super::flush::{host, host_without_blocks};
use super::gpu::{Quad, SIDE, shader};
use super::support::{material, render_texture, set, snapshot, texture_slot, world};

/// The identity the managed engine stored the array under.
pub(super) const LAYERS: u64 = 1 << 40;
/// `TextureFormat` RGBA32 and its four layers, each a 1x1 texel that is exact in sRGB and linear.
const RGBA32: u32 = 4;
const RED: [u8; 4] = [255, 0, 0, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];
const CYAN: [u8; 4] = [0, 255, 255, 255];
const YELLOW: [u8; 4] = [255, 255, 0, 255];
const GREEN: [u8; 4] = [0, 255, 0, 255];

/// The vertex stage and bindings every array pass shares.
const ARRAY_HEAD: &str = "
@group(0) @binding(0) var layers: texture_2d_array<f32>;
@group(0) @binding(1) var layers_sampler: sampler;
struct VOut { @builtin(position) position: vec4<f32>, @location(0) uv: vec2<f32> }
@vertex fn vs_main(@location(0) position: vec3<f32>, @location(1) uv: vec2<f32>) -> VOut {
    var out: VOut;
    out.position = vec4<f32>(position.xy, 0.0, 1.0);
    out.uv = uv;
    return out;
}";

/// A blit pass that reads the layer of `_Layers` that the column of the target names.
const ARRAY_SLICES: &str = "
@fragment fn fs_slices(in: VOut) -> @location(0) vec4<f32> {
    return textureSample(layers, layers_sampler, in.uv, i32(in.uv.x * 4.0));
}";

/// A blit pass that reads level `column % 2` of layer `column / 2` of `_Layers`.
const ARRAY_LEVELS: &str = "
@fragment fn fs_levels(in: VOut) -> @location(0) vec4<f32> {
    let column = i32(in.uv.x * 4.0);
    return textureSampleLevel(layers, layers_sampler, in.uv, column / 2, f32(column % 2));
}";

fn array_passes(fragment: &str, entry: &'static str) -> SyntheticShader {
    no_cull(synthetic_shader(
        &[],
        &[PassSpec {
            tags: Vec::new(),
            variants: vec![variant(
                &[],
                &format!("{ARRAY_HEAD}{fragment}"),
                entry,
                array_patch_parameters(),
            )],
        }],
    ))
}

/// The array slot is declared with the code 10 a player variant gives a 2D array.
pub(super) fn array_slices_passes() -> SyntheticShader {
    array_passes(ARRAY_SLICES, "fs_slices")
}

pub(super) fn array_levels_passes() -> SyntheticShader {
    array_passes(ARRAY_LEVELS, "fs_levels")
}

/// The quad's mesh and one stored texture under `LAYERS`.
pub(super) struct StoredAssets(pub(super) PlayerTexture);

impl Assets for StoredAssets {
    fn mesh(&self, identity: u64, known: u64) -> Result<Option<MeshChannels>, AssetError> {
        Quad.mesh(identity, known)
    }

    fn texture(&self, identity: u64, known: u64) -> Result<Option<PlayerTexture>, AssetError> {
        Ok((identity == LAYERS && known != self.0.generation).then(|| self.0.clone()))
    }

    fn sampler(&self, _: u64) -> Result<Option<TextureSampler>, AssetError> {
        Ok(None)
    }
}

/// An array of `depth` layers of `side` by `side` texels in the engine's texture `format`, each
/// layer holding `mips` levels, whose bytes are `bytes`.
fn array(format: u32, side: u32, (depth, mips): (u32, u32), bytes: Vec<u8>) -> PlayerTexture {
    PlayerTexture {
        format,
        width: side,
        height: side,
        mips,
        linear: false,
        generation: 1,
        distance_spread: 0,
        depth,
        dimension: TextureDimension::D2Array,
        graphics_format: 0,
        bytes,
    }
}

/// The target's texels, as unit floats, after a blit of `pass` with `_Layers` set to `stored`, and
/// the failures the flush counted.
fn blit_with(
    host: OffscreenHost,
    pass: &str,
    stored: PlayerTexture,
) -> (Option<Vec<f32>>, BTreeMap<String, usize>) {
    blit_property(host, (pass, "_Layers"), [SIDE, SIDE], stored)
}

/// As `blit_with` for the texture property `property` and a target of `size` texels.
pub(super) fn blit_property(
    mut host: OffscreenHost,
    (pass, property): (&str, &str),
    size: [u32; 2],
    stored: PlayerTexture,
) -> (Option<Vec<f32>>, BTreeMap<String, usize>) {
    let mut world = world();
    let array = shader(&mut world, pass);
    let blit = material(&mut world, array);
    set(
        &mut world,
        blit,
        PropertyValue::Texture(property.into(), texture_slot(TextureRef::Asset(LAYERS))),
    );
    let (source, out) = (
        render_texture(&mut world, size),
        render_texture(&mut world, size),
    );
    world
        .record_offscreen(OffscreenWork::Blit {
            source: TextureRef::Render(source),
            target: TargetRef::Render(out),
            material: Some(blit),
            pass: 0,
        })
        .unwrap();
    let request = world
        .request_texture_pixels(out, 0, 0, size[0], size[1])
        .unwrap();

    let snapshot = snapshot(&mut world);
    let outcome = host.flush(&snapshot, &StoredAssets(stored)).unwrap();
    outcome.apply(&mut world).unwrap();
    (
        world.take_texture_pixels(request),
        outcome.report.failures.clone(),
    )
}

/// The target's texels after a blit that counts no failure.
fn blitted(pass: &str, stored: PlayerTexture) -> Vec<f32> {
    let directory = tempfile::tempdir().unwrap();
    let (pixels, failures) = blit_with(host(directory.path()), pass, stored);
    assert!(failures.is_empty(), "{failures:?}");
    pixels.unwrap()
}

/// What a four-column target shows when column `c` is `colors[c]`.
fn columns(colors: [[u8; 4]; 4]) -> Vec<f32> {
    (0..SIDE)
        .flat_map(|_| {
            colors
                .iter()
                .flat_map(|texel| texel.map(|byte| f32::from(byte) / 255.0))
        })
        .collect()
}

#[test]
fn a_stored_array_is_uploaded_with_its_layers_and_a_draw_samples_each_one() {
    let layers = [RED, BLUE, CYAN, YELLOW];
    let stored = array(RGBA32, 1, (4, 1), layers.concat());
    // Column c shows layer c, in every row.
    assert_eq!(blitted("arrayslices", stored), columns(layers));
}

#[test]
fn a_stored_array_with_a_mip_chain_uploads_every_level_of_every_layer() {
    // Two layers of 2x2 texels, each followed by its 1x1 second level, in four colours.
    let layer = |base: [u8; 4], next: [u8; 4]| [base.repeat(4), next.to_vec()].concat();
    let bytes = [layer(RED, BLUE), layer(CYAN, YELLOW)].concat();
    let stored = array(RGBA32, 2, (2, 2), bytes);
    // Column c shows level c % 2 of layer c / 2.
    assert_eq!(
        blitted("arraylevels", stored),
        columns([RED, BLUE, CYAN, YELLOW])
    );
}

#[test]
fn an_array_the_engine_has_no_texture_format_for_reaches_the_device_by_its_graphics_format() {
    // As the managed engine stores GraphicsFormat 87: `format` zero, four bytes a texel, and two
    // layers of 2x2 texels each followed by its 1x1 second level.
    let layer = |base: [u8; 4], next: [u8; 4]| [base.repeat(4), next.to_vec()].concat();
    let stored = PlayerTexture {
        graphics_format: 87,
        ..array(
            0,
            2,
            (2, 2),
            [layer(RED, BLUE), layer(CYAN, YELLOW)].concat(),
        )
    };
    assert_eq!(
        blitted("arraylevels", stored),
        columns([RED, BLUE, CYAN, YELLOW])
    );
}

/// The `TextureFormat`s DXT1 and DXT5, and the blocks of one colour (RGB565) in each: both
/// endpoints are the colour and every index is 0, so all sixteen texels are it.
const DXT1: u32 = 10;
const DXT5: u32 = 12;
const RED_565: u16 = 0xF800;
const GREEN_565: u16 = 0x07E0;
const BLUE_565: u16 = 0x001F;
const CYAN_565: u16 = 0x07FF;
const YELLOW_565: u16 = 0xFFE0;
const MAGENTA_565: u16 = 0xF81F;

/// Makes a block of one RGB565 colour.
type Block = fn(u16) -> Vec<u8>;

fn bc1_block(color: u16) -> Vec<u8> {
    let [low, high] = color.to_le_bytes();
    vec![low, high, low, high, 0, 0, 0, 0]
}

fn bc3_block(color: u16) -> Vec<u8> {
    [vec![255, 255, 0, 0, 0, 0, 0, 0], bc1_block(color)].concat()
}

/// Two layers of 4x4 texels with three levels each (4x4, 2x2 and 1x1 take a block apiece), in
/// blocks made by `block`.
fn block_layers(block: Block) -> Vec<u8> {
    let chain = |colors: [u16; 3]| -> Vec<u8> { colors.iter().flat_map(|c| block(*c)).collect() };
    [
        chain([RED_565, GREEN_565, YELLOW_565]),
        chain([BLUE_565, CYAN_565, MAGENTA_565]),
    ]
    .concat()
}

#[test]
fn a_block_compressed_array_goes_up_as_blocks_and_a_draw_samples_each_level_of_each_layer() {
    for (format, block) in [(DXT1, bc1_block as Block), (DXT5, bc3_block)] {
        let stored = array(format, 4, (2, 3), block_layers(block));
        assert_eq!(
            blitted("arraylevels", stored),
            columns([RED, GREEN, BLUE, CYAN]),
            "format {format}"
        );
    }
}

#[test]
fn a_block_array_on_a_host_without_block_compression_is_refused_and_counted_by_format() {
    let directory = tempfile::tempdir().unwrap();
    let stored = array(DXT1, 4, (2, 3), block_layers(bc1_block));
    let bytes = stored.bytes.len();
    let (_, failures) = blit_with(host_without_blocks(directory.path()), "arraylevels", stored);
    assert_eq!(failures.get("texture:ArrayNeedsBlocks"), Some(&1));
    assert_eq!(
        failures.get(&format!("textureformat:DXT1 4x4, {bytes} bytes")),
        Some(&1),
        "{failures:?}"
    );
}
