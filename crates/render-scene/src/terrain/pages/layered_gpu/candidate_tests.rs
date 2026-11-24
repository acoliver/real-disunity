//! The bake composes each workgroup's candidate layers only. Its products must equal, byte for
//! byte, those of the same kernel composing every layer, over layer boundaries, blurred and
//! unblurred layers, indices in both halves of a 64-bit span, absent layers, texels beyond the
//! grid, scattered workgroups and a partial last workgroup.
use super::*;
use crate::terrain::pages::residency_tests::context;
use worldrunner_render_plan::*;

const LAYERS: u32 = 40;

fn fixture(filter: TerrainSplatFilter) -> (SceneTerrain, SceneResources) {
    let surfaces = (0..LAYERS)
        .map(|i| {
            let images = i % 3 * 3;
            SceneTerrainSurface::new(
                MaterialHandle::from_raw(0),
                SceneTerrainAppearance {
                    layered: Some(TerrainLayer {
                        albedo_metallic: TextureHandle::from_raw(images),
                        normal_roughness: TextureHandle::from_raw(images + 1),
                        height: TextureHandle::from_raw(images + 2),
                        normal_encoding: SceneNormalEncoding::TwoChannelXy,
                        repeat_millimeters: 1700 + i * 37,
                        offset_millimeters: [130, -70],
                        roughness_metallic_offset_micro: [-100_000, 70_000],
                    }),
                    rotation_arcminutes: i * 530 % 21_600,
                    blur_radius_millimeters: [0, 1300, 0, 4200, 700][i as usize % 5],
                    tint_percent: [80, 120, 100],
                },
            )
        })
        .collect();
    // Patches of five cells, each holding one of 31 indices across both mask words; indices
    // 7 and 23 to 30 never occur, so their layers compose nowhere.
    let [columns, rows] = [48_u32, 40];
    let cells = (0..rows)
        .flat_map(|y| (0..columns).map(move |x| [x / 5, y / 5]))
        .map(|[x, y]| {
            let index = (x * 7 + y * 11) % 31;
            u8::try_from(match index {
                7 => 39,
                23..=30 => index + 8,
                _ => index,
            })
            .unwrap()
        })
        .collect();
    let terrain = SceneTerrain::new(SceneTerrainParts {
        columns: 2,
        rows: 2,
        origin_east_millimeters: 0,
        origin_north_millimeters: 0,
        sample_spacing_millimeters: 2000,
        heights_millimeters: vec![0; 4],
        surface_indices: vec![0; 4],
        surfaces,
        lod_policy: TerrainLodPolicy::FullResolution,
    })
    .unwrap()
    .with_layered_source(TerrainLayeredSource {
        anisotropy: TextureAnisotropy::default(),
        channel_map: None,
        type_grid: TerrainTypeGrid::new([columns, rows], [-1500, -800], 700, cells).unwrap(),
        filter,
        colorization: Some(TerrainColorization {
            texture: TextureHandle::from_raw(9),
            channels: TerrainColorizationChannels::Rgb,
            origin_millimeters: [0; 2],
            extent_millimeters: [20_000; 2],
        }),
    })
    .unwrap();
    let (_, resources) = super::tests::fixture(
        TerrainSplatFilter::Spiral14,
        TerrainColorizationChannels::Rgb,
    );
    (terrain, resources)
}

/// A 100 x 70 raster from beyond one corner of the grid to beyond the other, then 64 texels
/// scattered over the whole grid, then a partial workgroup.
#[expect(clippy::cast_precision_loss, reason = "small test coordinates")]
fn positions() -> Vec<[f32; 2]> {
    let mut points: Vec<_> = (0..70)
        .flat_map(|y| (0..100).map(move |x| [-3.0 + x as f32 * 0.37, -2.5 + y as f32 * 0.43]))
        .collect();
    let mut state = 0x2545_f491_u32;
    let mut unit = || {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        state as f32 / u32::MAX as f32
    };
    points.extend((0..64 + 37).map(|_| [-2.0 + unit() * 36.0, -1.5 + unit() * 30.0]));
    points
}

#[test]
fn candidate_layers_bake_the_bytes_of_every_layer() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let every = LAYERED_WGSL.replace("fn layered_next(", "fn candidate_layered_next(")
        + "fn layered_next(index: u32) -> u32 { return index; }\n";
    let points = positions();
    for filter in [TerrainSplatFilter::Spiral14, TerrainSplatFilter::Bilinear] {
        let (terrain, resources) = fixture(filter);
        let source = LayeredGpuSource::new(&terrain, &resources).unwrap();
        for footprint in [[0.0; 2], [0.05, 0.05], [0.4, 0.4]] {
            let baked =
                super::source_mip_tests::execute_footprint(&context, &source, &points, footprint);
            let expected = super::source_mip_tests::execute_shader(
                &context,
                &source,
                &points,
                footprint,
                Some(&every),
            );
            let distinct: std::collections::HashSet<_> = expected.chunks_exact(16).collect();
            assert!(
                distinct.len() > 100,
                "{filter:?}: {} products",
                distinct.len()
            );
            let differing = baked
                .chunks_exact(16)
                .zip(expected.chunks_exact(16))
                .position(|(a, b)| a != b);
            assert_eq!(
                differing,
                None,
                "{filter:?} footprint {footprint:?}: first differing texel {:?}",
                differing.map(|i| points[i])
            );
        }
    }
}
