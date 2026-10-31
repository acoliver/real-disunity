//! The material mip bias in the frame block scales the terrain gradients, so the terrain albedo
//! moves to a finer mip level. The rig in `mip_rig` supplies a plain renderer (bias +0.0) and a
//! temporal half-scale renderer (bias forced to exactly -2). A terrain tile is 10 m of world and the albedo
//! has 64 texels with a constant colour per mip level, so a camera at height `h` looking down
//! spans one pixel over `2 h tan(30 degrees) / 32` meters and the implicit LOD is exactly the
//! integer `j` when that is `2^j / 6.4` meters.
use super::decals::texel;
use super::mip_bias::mipped_texture;
use super::mip_rig::{RENDER, assert_level, center, plain, temporal};
use super::*;

/// Camera height in millimeters for a pixel footprint of `2^level` texels:
/// `2^level * 10 / (4 tan 30 degrees)` meters, 17320.5 and 34641.0, rounded to a millimeter.
/// The camera is displaced 200 mm along the ground so the view direction is not parallel to
/// the world up axis. That tilts it by 0.66 degrees, which stretches the footprint by
/// `1 / cos^2` = 1.00013, an LOD change of 1.9e-4, under the 0.005 bound's margin of 0.0016.
pub(super) fn height_for(level: u32) -> i64 {
    match level {
        2 => 17_320,
        3 => 34_641,
        _ => unreachable!("only levels 2 and 3 have a fixture height"),
    }
}

fn scene() -> RenderScene {
    let material = SceneMaterial::new(SceneMaterialParts {
        albedo: TextureHandle::from_raw(0),
        normal: Some(TextureHandle::from_raw(1)),
        roughness_metallic: Some(TextureHandle::from_raw(2)),
        ambient_occlusion: None,
        self_illumination: None,
        roughness_micro: 1_000_000,
        metalness_micro: 0,
        alpha_mode: SceneAlphaMode::Opaque,
        double_sided: false,
    })
    .unwrap();
    let terrain = SceneTerrain::new(SceneTerrainParts {
        columns: 2,
        rows: 2,
        origin_east_millimeters: -20_000,
        origin_north_millimeters: -20_000,
        sample_spacing_millimeters: 40_000,
        heights_millimeters: vec![0; 4],
        surface_indices: vec![0; 4],
        surfaces: vec![SceneTerrainSurface::new(
            MaterialHandle::from_raw(0),
            SceneTerrainAppearance::NEUTRAL,
        )],
        lod_policy: TerrainLodPolicy::FullResolution,
    })
    .unwrap();
    RenderScene::new(SceneParts {
        resources: SceneResources::new(
            vec![
                mipped_texture(),
                texel([128, 128, 255, 255]),
                texel([0, 255, 0, 255]),
            ],
            vec![material],
        )
        .unwrap(),
        geometry: SceneGeometry::new(Vec::new(), Vec::new()).unwrap(),
        terrain: Some(terrain),
        effects: None,
    })
    .unwrap()
}

pub(super) fn frame_at(height: i64) -> FrameUniform {
    let view = SceneView::new(
        CameraPose {
            eye_millimeters: [0, -200, height],
            target_millimeters: [0, 0, 0],
        },
        camera(),
        Viewport {
            width: RENDER,
            height: RENDER,
        },
    )
    .unwrap();
    FrameUniform::unlit(&view)
}

#[test]
fn a_temporal_half_scale_renderer_selects_terrain_mips_two_levels_finer() {
    let context = context();
    let scene = scene();
    for level in [2_u32, 3] {
        let height = height_for(level);
        let reference = center(&context, &mut plain(&context), &scene, &frame_at(height));
        assert_level(reference, level as usize, "bias 0");
        let mut configured = temporal(&context);
        let biased = center(&context, &mut configured, &scene, &frame_at(height));
        assert_level(biased, level as usize - 2, "bias -2");
    }
}
