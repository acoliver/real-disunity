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

/// A blit pass that reads the layer of `_Layers` that the column of the target names.
const ARRAY_SLICES: &str = "
@group(0) @binding(0) var layers: texture_2d_array<f32>;
@group(0) @binding(1) var layers_sampler: sampler;
struct VOut { @builtin(position) position: vec4<f32>, @location(0) uv: vec2<f32> }
@vertex fn vs_main(@location(0) position: vec3<f32>, @location(1) uv: vec2<f32>) -> VOut {
    var out: VOut;
    out.position = vec4<f32>(position.xy, 0.0, 1.0);
    out.uv = uv;
    return out;
}
@fragment fn fs_slices(in: VOut) -> @location(0) vec4<f32> {
    return textureSample(layers, layers_sampler, in.uv, i32(in.uv.x * 4.0));
}";

/// The array slot is declared with the code 10 a player variant gives a 2D array.
pub(super) fn array_slices_passes() -> SyntheticShader {
    no_cull(synthetic_shader(
        &[],
        &[PassSpec {
            tags: Vec::new(),
            variants: vec![variant(
                &[],
                ARRAY_SLICES,
                "fs_slices",
                array_patch_parameters(),
            )],
        }],
    ))
}

/// The quad's mesh and the four-layer array under `LAYERS`.
struct ArrayAssets;

impl Assets for ArrayAssets {
    fn mesh(&self, identity: u64, known: u64) -> Result<Option<MeshChannels>, AssetError> {
        Quad.mesh(identity, known)
    }

    fn texture(&self, identity: u64, known: u64) -> Result<Option<PlayerTexture>, AssetError> {
        if identity != LAYERS || known == 1 {
            return Ok(None);
        }
        Ok(Some(PlayerTexture {
            format: RGBA32,
            width: 1,
            height: 1,
            mips: 1,
            linear: false,
            generation: 1,
            distance_spread: 0,
            depth: 4,
            bytes: [RED, BLUE, CYAN, YELLOW].concat(),
        }))
    }

    fn sampler(&self, _: u64) -> Result<Option<TextureSampler>, AssetError> {
        Ok(None)
    }
}

#[test]
fn a_stored_array_is_uploaded_with_its_layers_and_a_draw_samples_each_one() {
    let directory = tempfile::tempdir().unwrap();
    let mut host = host(directory.path());
    let mut world = world();
    let array = shader(&mut world, "arrayslices");
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
    let outcome = host.flush(&snapshot, &ArrayAssets).unwrap();
    outcome.apply(&mut world).unwrap();

    assert!(
        outcome.report.failures.is_empty(),
        "{:?}",
        outcome.report.failures
    );
    // Column c shows layer c, in every row.
    let layer_colors = [RED, BLUE, CYAN, YELLOW];
    let expected: Vec<f32> = (0..SIDE)
        .flat_map(|_| {
            layer_colors
                .iter()
                .flat_map(|texel| texel.map(|byte| f32::from(byte) / 255.0))
        })
        .collect();
    assert_eq!(world.take_texture_pixels(request), Some(expected));
}
