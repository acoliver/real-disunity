//! Encoding tests for the terrain splat uniform block.

use worldrunner_render_plan::{
    MaterialHandle, SceneTerrain, SceneTerrainAppearance, SceneTerrainParts, SceneTerrainSurface,
    TerrainLodPolicy,
};

use super::*;

const SPACING_MILLIMETERS: u32 = 600;

/// Reads one float back out of the encoded block.
fn read_f32(state: &TerrainUniformState, offset: usize) -> f32 {
    let mut raw = [0u8; 4];
    raw.copy_from_slice(&state.bytes()[offset..offset + 4]);
    f32::from_le_bytes(raw)
}

/// Reads one unsigned integer back out of the encoded block.
fn read_u32(state: &TerrainUniformState, offset: usize) -> u32 {
    let mut raw = [0u8; 4];
    raw.copy_from_slice(&state.bytes()[offset..offset + 4]);
    u32::from_le_bytes(raw)
}

/// Byte offset of one field within one surface record.
fn surface_offset(surface: usize, field: usize) -> usize {
    TERRAIN_HEADER_BYTES + surface * TERRAIN_SURFACE_BYTES + field * 4
}

/// Builds one terrain over the given surfaces with a two-by-two grid.
fn terrain(surfaces: Vec<SceneTerrainSurface>) -> SceneTerrain {
    let surface_indices = (0..4)
        .map(|index| u8::try_from(index % surfaces.len()).expect("small fixture"))
        .collect();
    SceneTerrain::new(SceneTerrainParts {
        columns: 2,
        rows: 2,
        origin_east_millimeters: -3000,
        origin_north_millimeters: 5000,
        sample_spacing_millimeters: SPACING_MILLIMETERS,
        heights_millimeters: vec![0; 4],
        surface_indices,
        surfaces,
        lod_policy: TerrainLodPolicy::FullResolution,
    })
    .expect("fixture terrain is valid")
}

/// Builds one surface with an explicit appearance.
fn surface(material: u32, appearance: SceneTerrainAppearance) -> SceneTerrainSurface {
    SceneTerrainSurface::new(MaterialHandle::from_raw(material), appearance)
}

#[test]
fn grid_geometry_encodes_into_the_header() {
    let state =
        TerrainUniformState::encode(&terrain(vec![surface(0, SceneTerrainAppearance::NEUTRAL)]))
            .expect("fixture encodes");

    assert!((read_f32(&state, 0) - -3.0).abs() < f32::EPSILON);
    assert!((read_f32(&state, 4) - 5.0).abs() < f32::EPSILON);
    assert!((read_f32(&state, 8) - 0.6).abs() < 1e-6);
    assert!((read_f32(&state, 12) - 1.0 / crate::terrain::TERRAIN_METERS_PER_TILE).abs() < 1e-6);
    assert_eq!(read_u32(&state, 16), 2);
    assert_eq!(read_u32(&state, 20), 2);
    assert_eq!(read_u32(&state, 24), 1);
}

#[test]
fn the_block_is_written_in_full() {
    let state =
        TerrainUniformState::encode(&terrain(vec![surface(0, SceneTerrainAppearance::NEUTRAL)]))
            .expect("fixture encodes");
    assert_eq!(state.bytes().len(), TERRAIN_UNIFORM_BYTES);
    assert_eq!(TERRAIN_UNIFORM_BYTES, 32 + 32 * 256);
}

#[test]
fn a_neutral_appearance_encodes_as_an_identity() {
    let state =
        TerrainUniformState::encode(&terrain(vec![surface(0, SceneTerrainAppearance::NEUTRAL)]))
            .expect("fixture encodes");

    // A neutral tint must reach the shader as one, not as zero: the tint is a
    // multiplier, so a zero would render the terrain black.
    for channel in 0..3 {
        assert!((read_f32(&state, surface_offset(0, channel)) - 1.0).abs() < f32::EPSILON);
    }
    assert!(read_f32(&state, surface_offset(0, 3)).abs() < f32::EPSILON);
    // An unrotated surface is cosine one, sine zero.
    assert!((read_f32(&state, surface_offset(0, 4)) - 1.0).abs() < f32::EPSILON);
    assert!(read_f32(&state, surface_offset(0, 5)).abs() < f32::EPSILON);
}

#[test]
fn authored_appearance_encodes_per_surface() {
    let state = TerrainUniformState::encode(&terrain(vec![surface(
        0,
        SceneTerrainAppearance {
            blur_radius_millimeters: 9000,
            tint_percent: [83, 108, 54],
            rotation_arcminutes: 2700,
        },
    )]))
    .expect("fixture encodes");

    // Tint is authored in percent of the engine's neutral modifier.
    assert!((read_f32(&state, surface_offset(0, 0)) - 0.83).abs() < 1e-6);
    assert!((read_f32(&state, surface_offset(0, 1)) - 1.08).abs() < 1e-6);
    assert!((read_f32(&state, surface_offset(0, 2)) - 0.54).abs() < 1e-6);
    // Nine metres of blur over a six-hundred-millimetre grid is fifteen texels.
    assert!((read_f32(&state, surface_offset(0, 3)) - 15.0).abs() < 1e-6);
    // Two thousand seven hundred arcminutes is forty-five degrees.
    let root_half = std::f32::consts::FRAC_1_SQRT_2;
    assert!((read_f32(&state, surface_offset(0, 4)) - root_half).abs() < 1e-6);
    assert!((read_f32(&state, surface_offset(0, 5)) - root_half).abs() < 1e-6);
}

#[test]
fn surfaces_sharing_a_material_share_one_layer() {
    // This is the counterpart of the scene domain's refusal to collapse them:
    // the two surfaces stay distinct, but they sample identical texels, so
    // uploading the material twice would waste a layer for no visible gain.
    let tinted = SceneTerrainAppearance {
        tint_percent: [83, 108, 54],
        ..SceneTerrainAppearance::NEUTRAL
    };
    let state = TerrainUniformState::encode(&terrain(vec![
        surface(7, SceneTerrainAppearance::NEUTRAL),
        surface(7, tinted),
        surface(4, SceneTerrainAppearance::NEUTRAL),
    ]))
    .expect("fixture encodes");

    assert_eq!(
        state.layer_materials(),
        [MaterialHandle::from_raw(7), MaterialHandle::from_raw(4)]
    );
    assert!(read_f32(&state, surface_offset(0, 6)).abs() < f32::EPSILON);
    assert!(read_f32(&state, surface_offset(1, 6)).abs() < f32::EPSILON);
    assert!((read_f32(&state, surface_offset(2, 6)) - 1.0).abs() < f32::EPSILON);
    // The shared layer must not have flattened the appearances themselves.
    assert!((read_f32(&state, surface_offset(0, 0)) - 1.0).abs() < f32::EPSILON);
    assert!((read_f32(&state, surface_offset(1, 0)) - 0.83).abs() < 1e-6);
}

#[test]
fn unaddressed_slots_encode_as_a_deterministic_identity() {
    let state =
        TerrainUniformState::encode(&terrain(vec![surface(0, SceneTerrainAppearance::NEUTRAL)]))
            .expect("fixture encodes");
    assert_eq!(state.surface_count(), 1);

    for slot in [1, 128, MAX_TERRAIN_SURFACES - 1] {
        for channel in 0..3 {
            assert!((read_f32(&state, surface_offset(slot, channel)) - 1.0).abs() < f32::EPSILON);
        }
        assert!((read_f32(&state, surface_offset(slot, 4)) - 1.0).abs() < f32::EPSILON);
        assert!(read_f32(&state, surface_offset(slot, 5)).abs() < f32::EPSILON);
    }
}
