//! A stored 2D array texture drawn through the synchronous flush on a real adapter (lavapipe in
//! CI): the managed engine stores the layers of an array in one byte string under one identity,
//! the host uploads them as one layered texture, and a draw reads each layer by its index.
//!
//! The shader is a synthetic blit pass that reads layer `i32(uv.x * 4.0)` of the array property
//! `_Layers`, so column `c` of a four-column target shows layer `c`. Every expected texel is the
//! color stored for that layer, not an output of the renderer.

use worldrunner_disunity_runtime::{MeshChannels, PlayerTexture, TextureSampler};
use worldrunner_render_program::unity::synthetic::shaders::{
    array_patch_parameters, no_cull, variant,
};
use worldrunner_render_program::unity::synthetic::{
    PassSpec, SyntheticShader, shader as synthetic_shader,
};
use worldrunner_world_simulation::graphics::{OffscreenWork, PropertyValue, TargetRef, TextureRef};

use super::super::assets::{AssetError, Assets};
use super::flush::host;
use super::gpu::{Quad, SIDE, shader};
use super::support::{material, render_texture, set, snapshot, texture_slot, world};

/// The identity the managed engine stored the array under.
const LAYERS: u64 = 1 << 40;
/// `TextureFormat` RGBA32 and its four layers, each a 1x1 texel that is exact in sRGB and linear.
const RGBA32: u32 = 4;
const RED: [u8; 4] = [255, 0, 0, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];
const CYAN: [u8; 4] = [0, 255, 255, 255];
const YELLOW: [u8; 4] = [255, 255, 0, 255];

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

/// The quad's mesh and one array under `LAYERS`.
struct ArrayAssets(PlayerTexture);

impl Assets for ArrayAssets {
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

/// An RGBA32 array of `depth` layers of `width` by `width` texels holding `bytes`.
fn array(width: u32, (depth, mips): (u32, u32), bytes: Vec<u8>) -> PlayerTexture {
    PlayerTexture {
        format: RGBA32,
        width,
        height: width,
        mips,
        linear: false,
        generation: 1,
        distance_spread: 0,
        depth,
        bytes,
    }
}

/// The target's texels, as unit floats, after a blit of `pass` with `_Layers` set to `stored`.
fn blitted(pass: &str, stored: PlayerTexture) -> Vec<f32> {
    let directory = tempfile::tempdir().unwrap();
    let mut host = host(directory.path());
    let mut world = world();
    let array = shader(&mut world, pass);
    let blit = material(&mut world, array);
    set(
        &mut world,
        blit,
        PropertyValue::Texture("_Layers".into(), texture_slot(TextureRef::Asset(LAYERS))),
    );
    let (source, out) = (
        render_texture(&mut world, [SIDE, SIDE]),
        render_texture(&mut world, [SIDE, SIDE]),
    );
    world
        .record_offscreen(OffscreenWork::Blit {
            source: TextureRef::Render(source),
            target: TargetRef::Render(out),
            material: Some(blit),
            pass: 0,
        })
        .unwrap();
    let request = world.request_texture_pixels(out, 0, 0, SIDE, SIDE).unwrap();

    let snapshot = snapshot(&mut world);
    let outcome = host.flush(&snapshot, &ArrayAssets(stored)).unwrap();
    outcome.apply(&mut world).unwrap();

    assert!(
        outcome.report.failures.is_empty(),
        "{:?}",
        outcome.report.failures
    );
    world.take_texture_pixels(request).unwrap()
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
    let stored = array(1, (4, 1), layers.concat());
    // Column c shows layer c, in every row.
    assert_eq!(blitted("arrayslices", stored), columns(layers));
}

#[test]
fn a_stored_array_with_a_mip_chain_uploads_every_level_of_every_layer() {
    // Two layers of 2x2 texels, each followed by its 1x1 second level, in four colours.
    let layer = |base: [u8; 4], next: [u8; 4]| [base.repeat(4), next.to_vec()].concat();
    let bytes = [layer(RED, BLUE), layer(CYAN, YELLOW)].concat();
    let stored = array(2, (2, 2), bytes);
    // Column c shows level c % 2 of layer c / 2.
    assert_eq!(
        blitted("arraylevels", stored),
        columns([RED, BLUE, CYAN, YELLOW])
    );
}
