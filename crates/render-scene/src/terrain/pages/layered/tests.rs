#![expect(
    clippy::float_cmp,
    reason = "exact products and deterministic sample identity are part of the reference contract"
)]
use super::*;
use worldrunner_render_plan::{
    MaterialHandle, SceneTerrainAppearance, SceneTerrainParts, SceneTerrainSurface, SceneTexture,
    TerrainColorization, TerrainLayer, TerrainLodPolicy, TerrainTypeGrid, TexelFormat,
    TextureColorSpace, TextureHandle,
};

fn layer(color: [f32; 3], height: f32, coverage: f32) -> HeightLayerSample {
    HeightLayerSample {
        albedo: color,
        normal: [0.0, 0.0, 1.0],
        roughness: 0.4,
        metallic: 0.2,
        height,
        coverage,
    }
}

fn compose(samples: &[HeightLayerSample]) -> LayeredTexel {
    compose_height_layers(samples, TerrainSplatFilter::Spiral14, [1.0; 3], 0.0).unwrap()
}

#[test]
fn three_layer_crossover_uses_accumulated_height_strength() {
    let result = compose(&[
        layer([1.0, 0.0, 0.0], 0.25, 1.0),
        layer([0.0, 1.0, 0.0], 0.5, 1.0),
        layer([0.0, 0.0, 1.0], 0.75, 1.0),
    ]);
    // First accumulated strength is 5/4. The second share is 4/9,
    // crossover is 18/31, and accumulated strength becomes 89/36.
    // Third share is 36/125; crossover weights are 163/413 and 250/413.
    let expected = [3250.0 / 12803.0, 4500.0 / 12803.0, 163.0 / 413.0];
    for (actual, expected) in result.albedo.into_iter().zip(expected) {
        assert!((actual - expected).abs() < 1e-6);
    }
    assert_eq!(result.height, 0.75);
}

#[test]
fn quantized_xy_normal_remains_finite_at_positive_x() {
    let (terrain, resources) = fixture(TerrainSplatFilter::Spiral14);
    let mut textures = resources.textures().to_vec();
    textures[1] = SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Linear,
        width: 1,
        height: 1,
        mips: 1,
        bytes: vec![255, 128, 0, 100].into_boxed_slice(),
    };
    let resources = SceneResources::new(textures, vec![]).unwrap();
    let result = LayeredReference::new(&terrain, &resources)
        .unwrap()
        .sample([0.5; 2])
        .unwrap();
    // UNORM8 XY decodes to (1, 1/255). Z is zero at the unit-disk edge.
    let length = (1.0_f32 + 1.0 / 65025.0).sqrt();
    let expected = [1.0 / length, 1.0 / (255.0 * length), 0.0];
    assert!(result.normal.iter().all(|value| value.is_finite()));
    for (actual, expected) in result.normal.into_iter().zip(expected) {
        assert!((actual - expected).abs() < 1e-6);
    }
}

#[test]
fn asymmetric_spiral_coverage_matches_independent_tent_integrals() {
    let (terrain, resources) = fixture(TerrainSplatFilter::Spiral14);
    let terrain = terrain
        .with_layered_source(TerrainLayeredSource {
            type_grid: TerrainTypeGrid::new([3, 3], [0; 2], 1000, vec![0, 1, 0, 1, 0, 1, 1, 1, 0])
                .unwrap(),
            filter: TerrainSplatFilter::Spiral14,
            colorization: None,
        })
        .unwrap();
    let reference = LayeredReference::new(&terrain, &resources).unwrap();
    // Independently sum the nine cell-centered tensor-product tent functions
    // for layer zero, with constant edge extension, at each rotated tap.
    // Hashes: 1951865600, 3943213472, 4058885216; sectors: 13, 10, 3.
    // Rounded (sin, cos): (-.984808, -.173648), (-.342020, -.939693),
    // (.866025, .5). Modulated radii: .739764736, 1.449191405, 1.006316194.
    // Weighted tent sums: 3.741279383884493, 3.309228594931470,
    // 4.217124696883147. Each is divided by the authored 7.499999.
    for (world, radius, expected) in [
        ([1.0, 1.0], 700, 0.498_837_32),
        ([1.25, 0.75], 1300, 0.441_230_54),
        ([1.75, 1.25], 900, 0.562_283_4),
    ] {
        assert!((reference.coverage(0, world, radius) - expected).abs() < 2e-7);
    }
    // Sequential f32 weights sum to 7.499998569488525, whereas the
    // denominator rounds to 7.499999046325684. Zero radius still divides.
    assert_eq!(
        reference.coverage(0, [0.1; 2], 0),
        f32::from_bits(0x3f7f_ffff)
    );
}

#[test]
fn single_layer_preserves_all_products() {
    let result = compose(&[layer([0.2, 0.4, 0.6], 0.8, 1.0)]);
    assert_eq!(result.albedo, [0.2, 0.4, 0.6]);
    assert_eq!(result.height, 0.8);
    assert_eq!(result.pack().0.color, [51, 102, 153, 51]);
    assert_eq!(result.pack().0.normal_roughness, [128, 128, 255, 102]);
    assert_eq!(result.pack().1, 204);
}

#[test]
fn height_crossover_and_maximum_are_distinct() {
    let result = compose(&[
        layer([1.0, 0.0, 0.0], 0.0, 1.0),
        layer([0.0, 0.0, 1.0], 0.25, 1.0),
    ]);
    assert_eq!(result.albedo, [1.0 / 3.0, 0.0, 2.0 / 3.0]);
    assert_eq!(result.height, 0.25);
    let result = compose(&[
        layer([1.0, 0.0, 0.0], 1.0, 0.8),
        layer([0.0, 0.0, 1.0], 0.0, 0.2),
    ]);
    assert_eq!(result.albedo, [1.0, 0.0, 0.0]);
    assert_eq!(result.height, 0.8);
}

#[test]
fn spiral_normals_accumulate_prior_directions() {
    let mut first = layer([1.0; 3], 0.0, 1.0);
    first.normal = [1.0, 0.0, 0.0];
    let result = compose(&[first, layer([0.0; 3], 0.0, 1.0)]);
    let expected = Vec3::new(1.0, 0.0, 0.5).normalize();
    assert!(Vec3::from_array(result.normal).abs_diff_eq(expected, 1e-6));
}

#[test]
fn bilinear_mode_uses_height_weight_but_zero_height_product() {
    let result = compose_height_layers(
        &[
            layer([1.0, 0.0, 0.0], 0.0, 1.0),
            layer([0.0, 0.0, 1.0], 1.0, 1.0),
        ],
        TerrainSplatFilter::Bilinear,
        [1.0; 3],
        0.0,
    )
    .unwrap();
    assert_eq!(result.albedo, [0.0, 0.0, 1.0]);
    assert_eq!(result.height, 0.0);
}

#[test]
fn tint_and_material_adjustment_happen_after_blending() {
    let result = compose_height_layers(
        &[layer([0.4, 0.2, 0.8], 0.3, 1.0)],
        TerrainSplatFilter::Spiral14,
        [0.5, 1.0, 0.25],
        -0.1,
    )
    .unwrap();
    assert_eq!(result.albedo, [0.2; 3]);
    assert!((result.roughness - 0.3).abs() < 1e-6);
}

#[test]
fn invalid_samples_fail_before_composition() {
    for sample in [
        layer([f32::NAN; 3], 0.0, 1.0),
        layer([1.0; 3], -0.1, 1.0),
        layer([1.0; 3], 0.0, 1.1),
    ] {
        assert_eq!(
            compose_height_layers(&[sample], TerrainSplatFilter::Spiral14, [1.0; 3], 0.0),
            Err(LayeredError::Sample)
        );
    }
    assert_eq!(
        compose_height_layers(
            &[layer([1.0; 3], 0.0, 0.0)],
            TerrainSplatFilter::Spiral14,
            [1.0; 3],
            0.0
        ),
        Err(LayeredError::Coverage)
    );
}

fn fixture(filter: TerrainSplatFilter) -> (SceneTerrain, SceneResources) {
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
        blur_radius_millimeters: 1000,
        ..SceneTerrainAppearance::NEUTRAL
    };
    let terrain = SceneTerrain::new(SceneTerrainParts {
        columns: 2,
        rows: 2,
        origin_east_millimeters: 0,
        origin_north_millimeters: 0,
        sample_spacing_millimeters: 1000,
        heights_millimeters: vec![0; 4],
        surface_indices: vec![0, 1, 0, 1],
        surfaces: vec![SceneTerrainSurface::new(MaterialHandle::from_raw(0), appearance); 2],
        lod_policy: TerrainLodPolicy::FullResolution,
    })
    .unwrap()
    .with_layered_source(TerrainLayeredSource {
        type_grid: TerrainTypeGrid::new([2, 2], [0; 2], 1000, vec![0, 1, 0, 1]).unwrap(),
        filter,
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
            texture(TexelFormat::Rgba8Unorm, vec![200, 100, 50, 128]),
        ],
        vec![],
    )
    .unwrap();
    (terrain, resources)
}

#[test]
fn type_grid_membership_and_boundary_clamping() {
    let (terrain, resources) = fixture(TerrainSplatFilter::Bilinear);
    let reference = LayeredReference::new(&terrain, &resources).unwrap();
    assert_eq!(reference.membership(0, [0.5, 0.5]), 1.0);
    assert_eq!(reference.membership(0, [1.0, 0.5]), 0.5);
    assert_eq!(reference.membership(1, [-10.0, 0.0]), 0.0);
    assert_eq!(reference.membership(1, [10.0, 0.0]), 1.0);
}

#[test]
fn spiral_uses_fractional_outer_taps_and_preserves_partition() {
    let (terrain, resources) = fixture(TerrainSplatFilter::Spiral14);
    let reference = LayeredReference::new(&terrain, &resources).unwrap();
    let a = reference.coverage(0, [1.0, 0.5], 1000);
    let b = reference.coverage(1, [1.0, 0.5], 1000);
    assert!(a > 0.0 && a < 1.0);
    assert!((a + b - 1.0).abs() < 1e-6);
    assert_eq!(a, reference.coverage(0, [1.0, 0.5], 1000));
}

#[test]
fn texture_colorization_uses_declared_channels() {
    let (terrain, resources) = fixture(TerrainSplatFilter::Spiral14);
    let terrain = terrain
        .with_layered_source(TerrainLayeredSource {
            type_grid: TerrainTypeGrid::new([2, 2], [0; 2], 1000, vec![0, 1, 0, 1]).unwrap(),
            filter: TerrainSplatFilter::Spiral14,
            colorization: Some(TerrainColorization {
                texture: TextureHandle::from_raw(3),
                channels: TerrainColorizationChannels::BgrRoughness,
                origin_millimeters: [0; 2],
                extent_millimeters: [1000; 2],
            }),
        })
        .unwrap();
    let result = LayeredReference::new(&terrain, &resources)
        .unwrap()
        .sample([0.5; 2])
        .unwrap();
    assert!((result.albedo[0] - 80.0 / 255.0 * 50.0 / 200.0).abs() < 1e-6);
    assert!((result.albedo[2] - 160.0 / 255.0).abs() < 1e-6);
    assert!((result.roughness - (100.0 / 255.0 + 1.0 / 255.0)).abs() < 1e-6);
}

#[test]
fn layered_descriptors_reject_missing_layers_and_zero_mapping_extent() {
    let (terrain, _) = fixture(TerrainSplatFilter::Spiral14);
    let source = TerrainLayeredSource {
        type_grid: TerrainTypeGrid::new([2, 2], [0; 2], 1000, vec![0, 1, 0, 1]).unwrap(),
        filter: TerrainSplatFilter::Spiral14,
        colorization: Some(TerrainColorization {
            texture: TextureHandle::from_raw(0),
            channels: TerrainColorizationChannels::Rgb,
            origin_millimeters: [0; 2],
            extent_millimeters: [0, 1000],
        }),
    };
    assert!(terrain.with_layered_source(source).is_err());
    let mut layer = SceneTerrainAppearance::NEUTRAL;
    assert!(layer.layered.is_none());
    layer.layered = Some(TerrainLayer {
        albedo_metallic: TextureHandle::from_raw(0),
        normal_roughness: TextureHandle::from_raw(1),
        normal_encoding: worldrunner_render_plan::SceneNormalEncoding::TwoChannelXy,
        height: TextureHandle::from_raw(2),
        repeat_millimeters: 0,
        offset_millimeters: [0; 2],
        roughness_metallic_offset_micro: [0; 2],
    });
    assert!(layer.layered.unwrap().validate().is_err());
}

#[test]
#[expect(
    clippy::cast_possible_truncation,
    reason = "same f64 page center to f32 conversion as the composition entry point"
)]
fn odd_page_extents_share_gutters_and_direct_sample_centers() {
    use worldrunner_render_plan::{TerrainPageSettings, TerrainPageSettingsParts};
    let (terrain, resources) = fixture(TerrainSplatFilter::Spiral14);
    let reference = LayeredReference::new(&terrain, &resources).unwrap();
    let settings = TerrainPageSettings::new(TerrainPageSettingsParts {
        interior: [3, 5],
        border: 1,
        filter_radius: 1,
        maximum_depth: 1,
        split_threshold_millipixels: 2000,
        merge_threshold_millipixels: 1000,
        maximum_selected_pages: 4,
        resident_bytes: 1_000_000,
        upload_bytes_per_frame: 1_000_000,
    })
    .unwrap();
    let left_key = PageKey::new(1, [0, 0]).unwrap();
    let left = reference.compose_page(settings, left_key).unwrap();
    let right = reference
        .compose_page(settings, PageKey::new(1, [1, 0]).unwrap())
        .unwrap();
    assert_eq!(left.len(), 35);
    for y in 0..7 {
        assert_eq!(left[y * 5 + 4], right[y * 5 + 1]);
        assert_eq!(left[y * 5 + 3], right[y * 5]);
    }
    let uv = super::super::composition::source_uv(settings, left_key, [2, 3]);
    assert_eq!(left[17], reference.sample(uv.map(|x| x as f32)).unwrap());
}

#[test]
fn unfinished_gpu_sampling_rejects_layered_sources() {
    let (terrain, _) = fixture(TerrainSplatFilter::Spiral14);
    assert_eq!(
        crate::terrain_uniform::TerrainUniformState::encode(&terrain),
        Err(crate::terrain_uniform::TerrainUniformError::LayeredSource)
    );
}

#[test]
fn roughness_clamps_only_after_composition() {
    let mut first = layer([0.5; 3], 0.0, 1.0);
    first.roughness = -1.0;
    let mut second = first;
    second.roughness = 1.0;
    let result = compose(&[first, second]);
    assert_eq!(result.roughness, 0.0);
    assert_eq!(result.pack().0.normal_roughness[3], 0);
    first.roughness = 0.0;
    assert_eq!(compose(&[first, second]).roughness, 0.5);
}

#[test]
fn srgb_rgb_decodes_before_filtering_and_metallic_stays_linear() {
    let (terrain, resources) = fixture(TerrainSplatFilter::Spiral14);
    let mut textures = resources.textures().to_vec();
    textures[0] = SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Srgb,
        width: 2,
        height: 1,
        mips: 1,
        bytes: vec![0, 0, 0, 64, 255, 255, 255, 192].into_boxed_slice(),
    };
    let resources = SceneResources::new(textures, vec![]).unwrap();
    let reference = LayeredReference::new(&terrain, &resources).unwrap();
    assert_eq!(
        reference
            .texture(TextureHandle::from_raw(0), [0.5; 2], false)
            .unwrap(),
        [
            0.5,
            0.5,
            0.5,
            64.0 / 255.0 + (192.0 / 255.0 - 64.0 / 255.0) * 0.5
        ]
    );
}

#[test]
fn type_grid_resolution_and_world_mapping_are_independent_of_geometry() {
    let (terrain, resources) = fixture(TerrainSplatFilter::Bilinear);
    let terrain = terrain
        .with_layered_source(TerrainLayeredSource {
            type_grid: TerrainTypeGrid::new([3, 1], [2000, 3000], 500, vec![1, 0, 1]).unwrap(),
            filter: TerrainSplatFilter::Bilinear,
            colorization: None,
        })
        .unwrap();
    let reference = LayeredReference::new(&terrain, &resources).unwrap();
    assert_eq!(reference.coverage(1, [2.25, 3.25], 0), 1.0);
    assert_eq!(reference.coverage(0, [2.75, 3.25], 0), 1.0);
}

#[test]
fn two_channel_normal_reconstruction_happens_after_filtering() {
    let direction = decode_normal(
        [0.8, 0.5, 0.0, 1.0],
        worldrunner_render_plan::SceneNormalEncoding::TwoChannelXy,
    );
    assert!(Vec3::from_array(direction).abs_diff_eq(Vec3::new(0.6, 0.0, 0.8), 1e-6));
    assert_eq!(
        decode_normal(
            [0.5, 0.5, 1.0, 1.0],
            worldrunner_render_plan::SceneNormalEncoding::Rgb
        ),
        [0.0, 0.0, 1.0]
    );
}
