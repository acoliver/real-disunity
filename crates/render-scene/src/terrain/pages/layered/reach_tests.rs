//! The page bake composes, for each workgroup, only the layers whose grid index occurs within
//! `layered_reach` cells of a texel's base cell (`layered_bake.wgsl`). These tests check that
//! bound against the reference coverage: an index one cell beyond it never contributes, and one
//! well inside it does, so the bound is neither wrong nor vacuous.
#![expect(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::float_cmp,
    reason = "mirrors the shader's f32 cell arithmetic; zero coverage is exact"
)]
use super::*;
use worldrunner_render_plan::{
    MaterialHandle, SceneTerrainAppearance, SceneTerrainParts, SceneTerrainSurface, SceneTexture,
    TerrainLayer, TerrainLodPolicy, TerrainTypeGrid, TexelFormat, TextureColorSpace, TextureHandle,
};

/// `layered_reach` of `layered_bake.wgsl` for a spiral source.
fn reach(radius_mm: u32, spacing_mm: u32, size: u32) -> i64 {
    let size = size as f32;
    let taps = radius_mm as f32 * 0.001 / (spacing_mm as f32 * 0.001) * 1.125;
    (taps + (size + taps) * 4.8e-7).ceil().min(size) as i64 + 3
}

/// `layered_base_cell` of `layered_bake.wgsl`.
fn base_cell(grid: f32, size: u32) -> i64 {
    ((grid - 0.5).clamp(-1.0, size as f32).floor() as i64).clamp(0, i64::from(size) - 1)
}

fn source(size: u32, spacing_mm: u32, cells: Vec<u8>) -> (SceneTerrain, SceneResources) {
    let appearance = SceneTerrainAppearance {
        layered: Some(TerrainLayer {
            albedo_metallic: TextureHandle::from_raw(0),
            normal_roughness: TextureHandle::from_raw(1),
            normal_encoding: worldrunner_render_plan::SceneNormalEncoding::TwoChannelXy,
            height: TextureHandle::from_raw(2),
            repeat_millimeters: 1000,
            offset_millimeters: [0; 2],
            roughness_metallic_offset_micro: [0; 2],
        }),
        ..SceneTerrainAppearance::NEUTRAL
    };
    let terrain = SceneTerrain::new(SceneTerrainParts {
        columns: 2,
        rows: 2,
        origin_east_millimeters: 0,
        origin_north_millimeters: 0,
        sample_spacing_millimeters: 1000,
        heights_millimeters: vec![0; 4],
        surface_indices: vec![0; 4],
        surfaces: vec![SceneTerrainSurface::new(MaterialHandle::from_raw(0), appearance); 2],
        lod_policy: TerrainLodPolicy::FullResolution,
    })
    .unwrap()
    .with_layered_source(TerrainLayeredSource {
        anisotropy: worldrunner_render_plan::TextureAnisotropy::default(),
        channel_map: None,
        type_grid: TerrainTypeGrid::new([size; 2], [0; 2], spacing_mm, cells).unwrap(),
        filter: TerrainSplatFilter::Spiral14,
        colorization: None,
    })
    .unwrap();
    let texture = |format, bytes: Vec<u8>| SceneTexture::Texels {
        format,
        color_space: TextureColorSpace::Linear,
        width: 1,
        height: 1,
        mips: 1,
        bytes: bytes.into_boxed_slice(),
    };
    let resources = SceneResources::new(
        vec![
            texture(TexelFormat::Rgba8Unorm, vec![80, 120, 160, 40]),
            texture(TexelFormat::Rgba8Unorm, vec![128, 128, 255, 100]),
            texture(TexelFormat::R8Unorm, vec![128]),
        ],
        vec![],
    )
    .unwrap();
    (terrain, resources)
}

/// The largest coverage of layer 1, held by the cells at Chebyshev distance `ring` from
/// `centre`, over texels whose base cell is `centre`.
fn ring_coverage(radius_mm: u32, spacing_mm: u32, corner: bool, ring: i64) -> f32 {
    let centre = if corner { 0 } else { ring + 1 };
    let size = u32::try_from(centre + ring + 2).unwrap();
    let cells = (0..i64::from(size))
        .flat_map(|y| (0..i64::from(size)).map(move |x| [x, y]))
        .map(|[x, y]| u8::from((x - centre).abs().max((y - centre).abs()) == ring))
        .collect();
    let (terrain, resources) = source(size, spacing_mm, cells);
    let reference = LayeredReference::new(&terrain, &resources).unwrap();
    let spacing = spacing_mm as f32 * 0.001;
    // Texels outside the grid clamp onto its edge cells, so corner texels start two cells out.
    let low = if corner { -2.0 } else { 0.0 };
    let mut state = 0x9e37_79b9_u32;
    let mut unit = || {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        state as f32 / u32::MAX as f32
    };
    let mut largest = 0.0_f32;
    let mut texels = 0;
    for _ in 0..4000 {
        let world = [0, 1].map(|_| (centre as f32 + 0.5 + low + (1.0 - low) * unit()) * spacing);
        if world.map(|x| base_cell(x / spacing, size)) != [centre; 2] {
            continue;
        }
        texels += 1;
        largest = largest.max(reference.coverage(1, world, radius_mm));
    }
    assert!(texels > 2000, "{texels} texels have the base cell");
    largest
}

#[test]
fn cells_beyond_the_bake_reach_never_contribute_coverage() {
    for spacing_mm in [700, 2000] {
        for radius_mm in [0, 600, 1300, 9000, 15_000] {
            for corner in [false, true] {
                // Every grid here is narrower than 128 cells, and the reach grows with the size.
                let beyond = reach(radius_mm, spacing_mm, 128) + 1;
                assert_eq!(
                    ring_coverage(radius_mm, spacing_mm, corner, beyond),
                    0.0,
                    "radius {radius_mm} mm, spacing {spacing_mm} mm, corner {corner}"
                );
                let taps = radius_mm as f32 / spacing_mm as f32;
                let inside = ((0.8 * taps) as i64).max(1);
                assert!(
                    ring_coverage(radius_mm, spacing_mm, corner, inside) > 0.0,
                    "radius {radius_mm} mm, spacing {spacing_mm} mm, corner {corner}: \
                     the ring {inside} cells out must contribute"
                );
            }
        }
    }
}
