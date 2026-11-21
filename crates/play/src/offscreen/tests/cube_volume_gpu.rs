//! A stored cube and a stored 3D texture drawn through the synchronous flush on a real adapter
//! (lavapipe in CI): the managed engine stores the faces of a cube or the slices of a volume in
//! one byte string under one identity, the host uploads them as one cube or one volume, and a
//! draw whose shader declares a cube or a 3D slot samples it.
//!
//! The cube pass reads the direction its column of a six-column target names (+X, -X, +Y, -Y, +Z,
//! -Z) and the volume pass reads the slice, or the level, its column of a four-column target
//! names. Every expected texel is the colour stored for that face or slice, not an output of the
//! renderer.

use std::collections::BTreeMap;

use worldrunner_disunity_runtime::{PlayerTexture, TextureDimension};
use worldrunner_render_program::unity::synthetic::shaders::{
    cube_blit_parameters, no_cull, variant, volume_blit_parameters,
};
use worldrunner_render_program::unity::synthetic::{
    PassSpec, SyntheticShader, shader as synthetic_shader,
};

use super::array_gpu::blit_property;
use super::flush::host;
use super::gpu::SIDE;

/// `TextureFormat` RGBA32, and colours whose channels are 0 or 255, which are exact in sRGB and in
/// linear.
const RGBA32: u32 = 4;
pub(super) const RED: [u8; 4] = [255, 0, 0, 255];
pub(super) const GREEN: [u8; 4] = [0, 255, 0, 255];
pub(super) const BLUE: [u8; 4] = [0, 0, 255, 255];
pub(super) const CYAN: [u8; 4] = [0, 255, 255, 255];
pub(super) const MAGENTA: [u8; 4] = [255, 0, 255, 255];
pub(super) const YELLOW: [u8; 4] = [255, 255, 0, 255];
const WHITE: [u8; 4] = [255, 255, 255, 255];

/// The vertex stage every pass here shares: a blit over the whole target.
const VERTEX: &str = "
struct VOut { @builtin(position) position: vec4<f32>, @location(0) uv: vec2<f32> }
@vertex fn vs_main(@location(0) position: vec3<f32>, @location(1) uv: vec2<f32>) -> VOut {
    var out: VOut;
    out.position = vec4<f32>(position.xy, 0.0, 1.0);
    out.uv = uv;
    return out;
}";

const CUBE_HEAD: &str = "
@group(0) @binding(0) var cube_tex: texture_cube<f32>;
@group(0) @binding(1) var cube_sampler: sampler;
fn direction(face: i32) -> vec3<f32> {
    switch face {
        case 0: { return vec3<f32>(1.0, 0.0, 0.0); }
        case 1: { return vec3<f32>(-1.0, 0.0, 0.0); }
        case 2: { return vec3<f32>(0.0, 1.0, 0.0); }
        case 3: { return vec3<f32>(0.0, -1.0, 0.0); }
        case 4: { return vec3<f32>(0.0, 0.0, 1.0); }
        default: { return vec3<f32>(0.0, 0.0, -1.0); }
    }
}";

/// Column `c` of six reads level 0 of the face in the direction `c`.
const CUBE_BASE: &str = "
@fragment fn fs_base(in: VOut) -> @location(0) vec4<f32> {
    let face = i32(in.uv.x * 6.0);
    return textureSampleLevel(cube_tex, cube_sampler, direction(face), 0.0);
}";

/// Column `c` of six reads level 1 of the face in the direction `c`.
const CUBE_TOP: &str = "
@fragment fn fs_top(in: VOut) -> @location(0) vec4<f32> {
    let face = i32(in.uv.x * 6.0);
    return textureSampleLevel(cube_tex, cube_sampler, direction(face), 1.0);
}";

const VOLUME_HEAD: &str = "
@group(0) @binding(0) var volume_tex: texture_3d<f32>;
@group(0) @binding(1) var volume_sampler: sampler;";

/// Column `c` of four reads level 0 at the centre of slice `c` of four.
const VOLUME_SLICES: &str = "
@fragment fn fs_slices(in: VOut) -> @location(0) vec4<f32> {
    let w = (floor(in.uv.x * 4.0) + 0.5) / 4.0;
    return textureSampleLevel(volume_tex, volume_sampler, vec3<f32>(0.5, 0.5, w), 0.0);
}";

/// Columns 0 and 1 read slices 0 and 1 of level 1, column 2 reads level 2 and column 3 reads
/// slice 3 of level 0.
const VOLUME_LEVELS: &str = "
@fragment fn fs_levels(in: VOut) -> @location(0) vec4<f32> {
    let column = i32(in.uv.x * 4.0);
    var level = 1.0;
    var w = 0.25 + 0.5 * f32(column);
    if column == 2 { level = 2.0; w = 0.5; }
    if column == 3 { level = 0.0; w = 0.875; }
    return textureSampleLevel(volume_tex, volume_sampler, vec3<f32>(0.5, 0.5, w), level);
}";

/// Which slot a pass declares: the cube `_Cube` or the volume `_Volume`.
#[derive(Clone, Copy)]
enum Slot {
    Cube,
    Volume,
}

fn passes(slot: Slot, wgsl: [&str; 2], entry: &'static str) -> SyntheticShader {
    let parameters = match slot {
        Slot::Cube => cube_blit_parameters("_Cube"),
        Slot::Volume => volume_blit_parameters("_Volume"),
    };
    no_cull(synthetic_shader(
        &[],
        &[PassSpec {
            tags: Vec::new(),
            variants: vec![variant(
                &[],
                &format!("{}{VERTEX}{}", wgsl[0], wgsl[1]),
                entry,
                parameters,
            )],
        }],
    ))
}

pub(super) fn cube_base_passes() -> SyntheticShader {
    passes(Slot::Cube, [CUBE_HEAD, CUBE_BASE], "fs_base")
}

/// The cube pass of `cube_base_passes` reading the property the host's environment binds.
pub(super) fn spec_cube_passes() -> SyntheticShader {
    no_cull(synthetic_shader(
        &[],
        &[PassSpec {
            tags: Vec::new(),
            variants: vec![variant(
                &[],
                &format!("{CUBE_HEAD}{VERTEX}{CUBE_BASE}"),
                "fs_base",
                cube_blit_parameters("unity_SpecCube0"),
            )],
        }],
    ))
}

pub(super) fn cube_top_passes() -> SyntheticShader {
    passes(Slot::Cube, [CUBE_HEAD, CUBE_TOP], "fs_top")
}

pub(super) fn volume_slices_passes() -> SyntheticShader {
    passes(Slot::Volume, [VOLUME_HEAD, VOLUME_SLICES], "fs_slices")
}

pub(super) fn volume_levels_passes() -> SyntheticShader {
    passes(Slot::Volume, [VOLUME_HEAD, VOLUME_LEVELS], "fs_levels")
}

/// A stored cube of `side` by `side` faces with `mips` levels in RGBA32, whose bytes are `bytes`:
/// the six faces one after the other, each followed by the rest of its levels.
pub(super) fn cube(side: u32, mips: u32, bytes: Vec<u8>) -> PlayerTexture {
    PlayerTexture {
        format: RGBA32,
        width: side,
        height: side,
        mips,
        linear: false,
        generation: 1,
        distance_spread: 0,
        depth: 6,
        dimension: TextureDimension::Cube,
        graphics_format: 0,
        bytes,
    }
}

/// A stored volume of `side` by `side` texels and `depth` slices with `mips` levels in RGBA32, the
/// levels one after the other from the base, each holding its slices one after the other.
fn volume(side: u32, depth: u32, mips: u32, bytes: Vec<u8>) -> PlayerTexture {
    PlayerTexture {
        depth,
        dimension: TextureDimension::D3,
        ..cube(side, mips, bytes)
    }
}

/// The texels of a `size` target after a blit of `pass` over the property `property` set to
/// `stored`, with the failures the flush counted.
fn blit(
    pass: &str,
    property: &str,
    size: [u32; 2],
    stored: PlayerTexture,
) -> (Option<Vec<f32>>, BTreeMap<String, usize>) {
    let directory = tempfile::tempdir().unwrap();
    blit_property(host(directory.path()), (pass, property), size, stored)
}

/// What a target of `colors.len()` columns shows when column `c` is `colors[c]`, in every row.
pub(super) fn columns(rows: u32, colors: &[[u8; 4]]) -> Vec<f32> {
    (0..rows)
        .flat_map(|_| {
            colors
                .iter()
                .flat_map(|texel| texel.map(|byte| f32::from(byte) / 255.0))
        })
        .collect()
}

/// The target's texels after a blit that counts no failure.
fn blitted(pass: &str, property: &str, size: [u32; 2], stored: PlayerTexture) -> Vec<f32> {
    let (pixels, failures) = blit(pass, property, size, stored);
    assert!(failures.is_empty(), "{failures:?}");
    pixels.unwrap()
}

/// A cube of 2x2 faces with a second 1x1 level: face `f` is `BASE[f]` and its second level is
/// `TOP[f]`.
pub(super) const BASE: [[u8; 4]; 6] = [RED, GREEN, BLUE, CYAN, MAGENTA, YELLOW];
pub(super) const TOP: [[u8; 4]; 6] = [YELLOW, MAGENTA, CYAN, BLUE, GREEN, RED];

fn two_level_cube() -> PlayerTexture {
    let bytes = (0..6)
        .flat_map(|face| [BASE[face].repeat(4), TOP[face].to_vec()].concat())
        .collect();
    cube(2, 2, bytes)
}

#[test]
fn a_stored_cube_is_uploaded_as_six_faces_and_a_draw_samples_each_direction() {
    let stored = cube(1, 1, BASE.concat());
    assert_eq!(
        blitted("cubebase", "_Cube", [6, SIDE], stored),
        columns(SIDE, &BASE)
    );
}

#[test]
fn a_stored_cube_with_a_mip_chain_uploads_every_level_of_every_face() {
    assert_eq!(
        blitted("cubebase", "_Cube", [6, SIDE], two_level_cube()),
        columns(SIDE, &BASE)
    );
    assert_eq!(
        blitted("cubetop", "_Cube", [6, SIDE], two_level_cube()),
        columns(SIDE, &TOP)
    );
}

#[test]
fn a_stored_volume_is_uploaded_as_one_texture_and_a_draw_samples_each_slice() {
    let slices = [RED, GREEN, BLUE, YELLOW];
    // Four slices of 2x2 texels, one colour each.
    let stored = volume(2, 4, 1, slices.iter().flat_map(|c| c.repeat(4)).collect());
    assert_eq!(
        blitted("volumeslices", "_Volume", [SIDE, SIDE], stored),
        columns(SIDE, &slices)
    );
}

#[test]
fn a_stored_volume_with_a_mip_chain_uploads_each_level_with_its_own_depth() {
    // Level 0 is 2x2x4, level 1 is 1x1x2 and level 2 is 1x1x1.
    let bytes = [
        [RED, GREEN, BLUE, YELLOW]
            .iter()
            .flat_map(|c| c.repeat(4))
            .collect::<Vec<u8>>(),
        [CYAN, MAGENTA].concat(),
        WHITE.to_vec(),
    ]
    .concat();
    let stored = volume(2, 4, 3, bytes);
    // Slices 0 and 1 of level 1, level 2, and slice 3 of level 0.
    assert_eq!(
        blitted("volumelevels", "_Volume", [SIDE, SIDE], stored),
        columns(SIDE, &[CYAN, MAGENTA, WHITE, YELLOW])
    );
}

#[test]
fn a_cube_that_is_not_made_of_six_equal_faces_is_refused_and_counted_by_format() {
    // Five faces of RGBA32 texels in a record that says six.
    let stored = cube(1, 1, BASE.concat()[..20].to_vec());
    let (_, failures) = blit("cubebase", "_Cube", [6, SIDE], stored);
    assert_eq!(failures.get("texture:Layers"), Some(&1), "{failures:?}");
}

#[test]
fn a_volume_in_a_format_the_host_does_not_read_is_refused_and_counted_by_format() {
    // A 4x4x1 volume in DXT1, whose blocks a 3D texture does not take.
    let stored = PlayerTexture {
        format: 10,
        ..volume(4, 1, 1, vec![0; 8])
    };
    let (_, failures) = blit("volumeslices", "_Volume", [SIDE, SIDE], stored);
    assert_eq!(
        failures.get("texture:Unsupported"),
        Some(&1),
        "{failures:?}"
    );
    assert_eq!(
        failures.get("textureformat:DXT1 4x4, 8 bytes"),
        Some(&1),
        "{failures:?}"
    );
}

/// A stored plain 2D texture of one 1x1 RGBA32 texel.
fn flat() -> PlayerTexture {
    PlayerTexture {
        depth: 1,
        dimension: TextureDimension::D2,
        ..cube(1, 1, RED.to_vec())
    }
}

/// A stored cube of six 1x1 faces, and a stored volume of one 1x1 slice.
fn one_cube() -> PlayerTexture {
    cube(1, 1, BASE.concat())
}

fn one_volume() -> PlayerTexture {
    volume(1, 1, 1, RED.to_vec())
}

/// The draw of `pass` over a property that holds a texture of another dimension than the shader
/// declares is left out and counted under the one name that says so.
fn assert_mismatch_is_named(pass: &str, property: &str, stored: PlayerTexture) {
    let (pixels, failures) = blit(pass, property, [SIDE, SIDE], stored);
    assert_eq!(
        failures,
        BTreeMap::from([("record:SlotDimensionMismatch".to_owned(), 1)]),
        "{failures:?}"
    );
    assert!(
        pixels.unwrap().iter().all(|&texel| texel == 0.0),
        "the draw is left out"
    );
}

#[test]
fn a_cube_slot_over_a_stored_2d_texture_is_refused_by_name() {
    assert_mismatch_is_named("cubebase", "_Cube", flat());
}

#[test]
fn a_volume_slot_over_a_stored_2d_texture_is_refused_by_name() {
    assert_mismatch_is_named("volumeslices", "_Volume", flat());
}

#[test]
fn a_cube_slot_over_a_stored_volume_is_refused_by_name() {
    assert_mismatch_is_named("cubebase", "_Cube", one_volume());
}

#[test]
fn a_volume_slot_over_a_stored_cube_is_refused_by_name() {
    assert_mismatch_is_named("volumeslices", "_Volume", one_cube());
}

#[test]
fn a_2d_slot_over_a_stored_cube_is_refused_by_name() {
    assert_mismatch_is_named("flatslot", "_FlatTex", one_cube());
}

#[test]
fn a_2d_slot_over_a_stored_volume_is_refused_by_name() {
    assert_mismatch_is_named("flatslot", "_FlatTex", one_volume());
}
