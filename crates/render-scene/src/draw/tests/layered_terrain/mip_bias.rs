//! The material mip bias in the frame block scales the gradients of the direct layered terrain
//! draw, so the layer albedo moves to a finer mip level, in the isotropic and the anisotropic
//! filter alike. The rig is the one of the conventional terrain test: a plain renderer (bias
//! +0.0) against a temporal half-scale renderer (bias forced to exactly -2), a 64 texel albedo with a
//! constant colour per mip level and a repeat of 10 m, so the camera heights of that test give
//! the implicit LOD exactly 2 and 3. The albedo has zero alpha, which is the layer's metallic.
//! The camera looks almost straight down, so the anisotropic footprint is a ratio of 1.0003 to
//! 1.0006 and takes two taps of the same level, which still runs the anisotropic code path.
//!
//! The isotropic draw leaves the LOD to the hardware, which resolves the exact integer of
//! `height_for`. The anisotropic draw computes the LOD in the shader and passes it to
//! `textureSampleLevel`, and there the camera tilt and the rounding of `height_for` leave it a
//! hair below the integer (1.9989 for level 2). A sampler that keeps six fractional LOD bits and
//! truncates, as the Apple M4 Max Metal failure suggests, then blends 1/64 of the finer level in: a colour that
//! misses the bound. The anisotropic fixture therefore uses `aniso_height_for`, which sits 2^-9
//! of an LOD above each integer, so exact, rounding and truncating samplers all select the
//! asserted level; `aniso_footprint` recomputes the shader LOD on the CPU and bounds it.
use super::super::mip_bias::LEVELS;
use super::super::mip_rig::{RENDER, assert_level, center, plain, temporal};
use super::super::terrain_mip_bias::{frame_at, height_for};
use super::*;

fn layer_albedo() -> SceneTexture {
    let mut bytes = Vec::new();
    for (level, color) in LEVELS.iter().enumerate() {
        for _ in 0..(64_usize >> level).pow(2) {
            bytes.extend_from_slice(&[color[0], color[1], color[2], 0]);
        }
    }
    SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Linear,
        width: 64,
        height: 64,
        mips: 7,
        bytes: bytes.into(),
    }
}

fn ten_meter_surface() -> SceneTerrainSurface {
    let mut appearance = layered_surface(0).appearance();
    appearance.layered.as_mut().unwrap().repeat_millimeters = 10_000;
    SceneTerrainSurface::new(MaterialHandle::from_raw(0), appearance)
}

fn mipped_layer_scene(anisotropy: u16) -> RenderScene {
    let resources = SceneResources::new(
        vec![
            layer_albedo(),
            texels([1, 1], &[128, 128, 255, 255]),
            SceneTexture::Texels {
                format: TexelFormat::R8Unorm,
                color_space: TextureColorSpace::Linear,
                width: 1,
                height: 1,
                mips: 1,
                bytes: vec![255].into(),
            },
        ],
        vec![material(SceneAlphaMode::Opaque, false)],
    )
    .unwrap();
    let terrain = SceneTerrain::new(SceneTerrainParts {
        columns: 2,
        rows: 2,
        origin_east_millimeters: -20_000,
        origin_north_millimeters: -20_000,
        sample_spacing_millimeters: 40_000,
        heights_millimeters: vec![0; 4],
        surface_indices: vec![0; 4],
        surfaces: vec![ten_meter_surface()],
        lod_policy: TerrainLodPolicy::FullResolution,
    })
    .unwrap()
    .with_layered_source(TerrainLayeredSource {
        anisotropy: worldrunner_render_plan::TextureAnisotropy::new(anisotropy).unwrap(),
        channel_map: None,
        type_grid: TerrainTypeGrid::new([2, 2], [0; 2], 1000, vec![0; 4]).unwrap(),
        filter: TerrainSplatFilter::Bilinear,
        colorization: None,
    })
    .unwrap();
    RenderScene::new(SceneParts {
        resources,
        geometry: SceneGeometry::new(vec![], vec![]).unwrap(),
        terrain: Some(terrain),
        effects: None,
    })
    .unwrap()
}

/// Camera height in millimeters for the anisotropic fixture. The shader's LOD at the centre
/// pixel is `log2` of the footprint of the 200 mm tilted camera: 1.99892 and 2.99943 at the
/// heights of `height_for`, just below the integer. These heights are the first whole
/// millimeters that put it 2^-9 above, 2.00200 and 2.00197 (see `aniso_footprint`).
pub(super) fn aniso_height_for(level: u32) -> i64 {
    match level {
        2 => 17_357,
        3 => 34_702,
        _ => unreachable!("only levels 2 and 3 have a fixture height"),
    }
}

fn assert_bias_selects_finer_levels(anisotropy: u16, height_of: fn(u32) -> i64) {
    let context = context();
    let scene = mipped_layer_scene(anisotropy);
    for level in [2_u32, 3] {
        let frame = frame_at(height_of(level));
        let reference = center(&context, &mut plain(&context), &scene, &frame);
        assert_level(reference, level as usize, "bias 0");
        let biased = center(&context, &mut temporal(&context), &scene, &frame);
        assert_level(biased, level as usize - 2, "bias -2");
    }
    assert_eq!(RENDER, 32);
}

#[test]
fn a_temporal_half_scale_renderer_selects_isotropic_layered_mips_two_levels_finer() {
    assert_bias_selects_finer_levels(1, height_for);
}

#[test]
fn a_temporal_half_scale_renderer_selects_anisotropic_layered_mips_two_levels_finer() {
    assert_bias_selects_finer_levels(4, aniso_height_for);
}

/// The page bake runs without a frame block and its footprints are world-space page footprints,
/// so a frame carrying a nonzero bias must leave the baked pages, and the cached draw that reads
/// them, untouched. Both draws run the same code on the same device, so the bytes are identical.
#[test]
fn a_nonzero_material_mip_bias_leaves_the_baked_pages_unchanged() {
    let context = context();
    for filter in [TerrainSplatFilter::Bilinear, TerrainSplatFilter::Spiral14] {
        let scene = layered_scene(filter);
        let unbiased = render_with_frame(&context, &scene, true, &matching_frame());
        let mut frame = matching_frame();
        frame.material_mip_bias = -2.0;
        let biased = render_with_frame(&context, &scene, true, &frame);
        assert!(lit_pixels(&unbiased) > 4000);
        assert_eq!(unbiased, biased, "{filter:?}");
    }
}
