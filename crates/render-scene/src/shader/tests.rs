//! Behavioural claims about the environment shader.
//!
//! These validate the WGSL on the CPU with the same front end `wgpu` uses, so a
//! shader fault is a failing test rather than a black window on a machine with
//! a GPU attached.

use super::{
    ENVIRONMENT_WGSL, FLAG_AMBIENT_OCCLUSION, FLAG_NORMAL, FLAG_ROUGH_METAL,
    FLAG_SELF_ILLUMINATION, MASKED_ENTRY_POINT, OPAQUE_ENTRY_POINT, TERRAIN_FRAGMENT_ENTRY_POINT,
    TERRAIN_VERTEX_ENTRY_POINT, TERRAIN_WGSL, VERTEX_ENTRY_POINT, fragment_entry_point,
};
use crate::terrain_uniform::TERRAIN_UNIFORM_BYTES;
use naga::valid::{Capabilities, ValidationFlags, Validator};

/// Parses and validates the shader, returning the module and its type info.
fn validate() -> naga::Module {
    let module = naga::front::wgsl::parse_str(ENVIRONMENT_WGSL)
        .unwrap_or_else(|error| panic!("the environment shader must parse: {error}"));
    Validator::new(ValidationFlags::all(), Capabilities::all())
        .validate(&module)
        .unwrap_or_else(|error| panic!("the environment shader must validate: {error}"));
    module
}

#[test]
fn the_environment_shader_compiles() {
    // Catching this on the CPU matters because the alternative is discovering
    // it when a device is already up and the failure looks like a blank frame.
    validate();
}

#[test]
fn every_entry_point_the_pipelines_bind_exists() {
    // The pipeline layer names these as strings, so a rename in the WGSL would
    // otherwise surface only as a run-time pipeline creation failure.
    let module = validate();
    let names: Vec<&str> = module
        .entry_points
        .iter()
        .map(|entry| entry.name.as_str())
        .collect();
    for expected in [VERTEX_ENTRY_POINT, OPAQUE_ENTRY_POINT, MASKED_ENTRY_POINT] {
        assert!(
            names.contains(&expected),
            "entry point {expected} is missing; the shader declares {names:?}"
        );
    }
}

#[test]
fn the_masked_variant_is_selected_only_for_masked_materials() {
    assert_eq!(fragment_entry_point(true), MASKED_ENTRY_POINT);
    assert_eq!(fragment_entry_point(false), OPAQUE_ENTRY_POINT);
}

#[test]
fn only_the_masked_entry_point_discards_fragments() {
    // The opaque pipeline writes depth for every fragment it covers. A stray
    // discard there would punch holes that read as missing geometry rather than
    // as a shading fault.
    let module = validate();
    let masked = module
        .entry_points
        .iter()
        .find(|entry| entry.name == MASKED_ENTRY_POINT)
        .expect("the masked entry point exists");
    let opaque = module
        .entry_points
        .iter()
        .find(|entry| entry.name == OPAQUE_ENTRY_POINT)
        .expect("the opaque entry point exists");
    assert!(
        contains_kill(&masked.function),
        "the masked entry point must discard cut-out fragments"
    );
    assert!(
        !contains_kill(&opaque.function),
        "the opaque entry point must not discard"
    );
}

/// Reports whether a function body contains a `discard`, at any nesting depth.
fn contains_kill(function: &naga::Function) -> bool {
    block_contains_kill(&function.body)
}

/// Walks a block and its nested blocks looking for a `discard`.
///
/// A cut-out discard is necessarily guarded by the cutoff comparison, so a
/// search that only looked at top-level statements would never find it.
fn block_contains_kill(block: &naga::Block) -> bool {
    block.iter().any(|statement| match statement {
        naga::Statement::Kill => true,
        naga::Statement::Block(inner) => block_contains_kill(inner),
        naga::Statement::If { accept, reject, .. } => {
            block_contains_kill(accept) || block_contains_kill(reject)
        }
        naga::Statement::Loop {
            body, continuing, ..
        } => block_contains_kill(body) || block_contains_kill(continuing),
        naga::Statement::Switch { cases, .. } => {
            cases.iter().any(|case| block_contains_kill(&case.body))
        }
        _ => false,
    })
}

#[test]
fn the_texture_flags_are_distinct_single_bits() {
    // These are combined with bitwise or, so a duplicated or multi-bit value
    // would silently enable one channel whenever another was requested.
    let flags = [
        FLAG_NORMAL,
        FLAG_ROUGH_METAL,
        FLAG_AMBIENT_OCCLUSION,
        FLAG_SELF_ILLUMINATION,
    ];
    for flag in flags {
        assert_eq!(flag.count_ones(), 1, "flag {flag} must be a single bit");
    }
    let combined = flags
        .iter()
        .fold(0u32, |accumulator, flag| accumulator | flag);
    assert_eq!(
        combined.count_ones(),
        u32::try_from(flags.len()).expect("four flags fit in a u32"),
        "the flags must not overlap"
    );
}

#[test]
fn the_shader_flags_match_the_wgsl_constants() {
    // The Rust constants are what fill the uniform; the WGSL constants are what
    // test it. If they drift, every material silently samples the wrong maps.
    for (name, value) in [
        ("FLAG_NORMAL", FLAG_NORMAL),
        ("FLAG_ROUGH_METAL", FLAG_ROUGH_METAL),
        ("FLAG_AMBIENT_OCCLUSION", FLAG_AMBIENT_OCCLUSION),
        ("FLAG_SELF_ILLUMINATION", FLAG_SELF_ILLUMINATION),
    ] {
        let declaration = format!("const {name}: u32 = {value}u;");
        assert!(
            ENVIRONMENT_WGSL.contains(&declaration),
            "the shader must declare `{declaration}`"
        );
    }
}

#[test]
fn the_normal_map_reconstructs_its_third_channel() {
    // All 105 authored normal maps are BC5, which stores two channels. Sampling
    // the blue channel would return zero and lay every normal flat into the
    // surface, so the shader must rebuild z from the unit-length constraint.
    let module = validate();
    let uses_blue = ENVIRONMENT_WGSL.contains("normal_texture, material_sampler, uv).rgb");
    assert!(!uses_blue, "a BC5 normal map has no blue channel to sample");
    assert!(
        ENVIRONMENT_WGSL.contains("sqrt(max(0.0, 1.0 - dot(xy, xy)))"),
        "the shader must reconstruct the normal's third channel"
    );
    assert!(
        !module.entry_points.is_empty(),
        "the shader must expose entry points"
    );
}

#[test]
fn ambient_light_blends_about_the_scenes_own_up_axis() {
    // Scene space is right-handed Z-up. A hemisphere ambient term that blended
    // on y would run the horizon vertically and light the world sideways - a
    // fault that looks like a broken sky rather than a wrong axis.
    assert!(
        ENVIRONMENT_WGSL.contains("rd_hemisphere(normal, vec3<f32>(0.0, 0.0, 1.0),"),
        "the ambient hemisphere must blend on the scene's up axis"
    );
}

/// Parses and validates the terrain shader, returning the module.
fn validate_terrain() -> naga::Module {
    let module = naga::front::wgsl::parse_str(TERRAIN_WGSL)
        .unwrap_or_else(|error| panic!("the terrain shader must parse: {error}"));
    Validator::new(ValidationFlags::all(), Capabilities::all())
        .validate(&module)
        .unwrap_or_else(|error| panic!("the terrain shader must validate: {error}"));
    module
}

#[test]
fn the_terrain_shader_compiles() {
    validate_terrain();
}

#[test]
fn the_terrain_entry_points_the_pipeline_binds_exist() {
    let module = validate_terrain();
    let names: Vec<&str> = module
        .entry_points
        .iter()
        .map(|entry| entry.name.as_str())
        .collect();
    for expected in [TERRAIN_VERTEX_ENTRY_POINT, TERRAIN_FRAGMENT_ENTRY_POINT] {
        assert!(
            names.contains(&expected),
            "entry point {expected} is missing; the shader declares {names:?}"
        );
    }
}

#[test]
fn the_terrain_block_is_the_size_the_encoder_writes() {
    // The encoder packs this block byte by byte and the shader reads it as a
    // struct. Nothing else connects the two, so a field added on one side alone
    // would not fail to compile - it would shift every field after it and
    // render the world in whichever materials the misread bytes happened to
    // name. Measuring the shader's own layout is the only check that catches it.
    let module = validate_terrain();
    let mut layouter = naga::proc::Layouter::default();
    layouter
        .update(module.to_ctx())
        .expect("the terrain shader's types must lay out");

    let (handle, _) = module
        .types
        .iter()
        .find(|(_, ty)| ty.name.as_deref() == Some("Terrain"))
        .expect("the terrain shader must declare a `Terrain` uniform block");

    assert_eq!(
        u64::from(layouter[handle].size),
        TERRAIN_UNIFORM_BYTES as u64,
        "the WGSL `Terrain` block and the encoded block must agree in size"
    );
}

#[test]
fn the_terrain_type_grid_is_never_filtered() {
    // The grid holds surface identifiers. Filtering between 7 and 84 yields 45,
    // naming an unrelated surface that may not even be bound - so the grid is
    // declared as an unfilterable integer texture and read with textureLoad.
    // A `textureSample` against it would not be a quality regression; it would
    // be a correctness one, and this is what forbids it.
    assert!(
        TERRAIN_WGSL.contains("var type_grid: texture_2d<u32>;"),
        "the type grid must be an integer texture"
    );
    assert!(
        !TERRAIN_WGSL.contains("textureSample(type_grid"),
        "the type grid must never be filtered"
    );
}

#[test]
fn the_terrain_blend_takes_no_derivatives_in_non_uniform_flow() {
    // The blend loop's trip count depends on which surfaces meet at a fragment,
    // so it is non-uniform, and a derivative taken inside it reads lanes that
    // may have taken a different branch. Naga's uniformity analysis rejects
    // that outright, which means this claim is really enforced by the shader
    // validating at all; the assertion below pins the mechanism that keeps it
    // true, so that a later edit reaching for `textureSample` gets a failing
    // test rather than a validation error with no explanation attached.
    validate_terrain();
    assert!(
        TERRAIN_WGSL.contains("textureSampleGrad(albedo_array"),
        "surface sampling must supply explicit gradients"
    );
    assert!(
        !TERRAIN_WGSL.contains("textureSample(albedo_array"),
        "an implicit-derivative sample cannot be taken inside the blend loop"
    );
}

#[test]
fn terrain_and_environment_share_one_reflectance_model() {
    // Both sources are composed from the same lighting prelude. If terrain
    // shaded with its own copy, terrain and the props standing on it would
    // drift apart under the same sun, and that reads as terrain being the wrong
    // material rather than as two shaders disagreeing.
    for source in [ENVIRONMENT_WGSL, TERRAIN_WGSL] {
        assert!(
            source.contains("fn surface_lighting("),
            "every shader must be composed with the shared lighting model"
        );
    }
    assert_eq!(
        TERRAIN_WGSL.matches("fn surface_lighting(").count(),
        1,
        "the reflectance model must be defined exactly once per module"
    );
}
