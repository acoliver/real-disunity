use super::*;
use crate::offscreen::capture_scene;

fn color_texture(bytes: [u8; 4]) -> SceneTexture {
    SceneTexture::Texels {
        format: worldrunner_render_plan::TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Linear,
        width: 1,
        height: 1,
        mips: 1,
        bytes: bytes.to_vec().into(),
    }
}

fn colored_terrain(reversed: bool, blur_radius_millimeters: u32) -> RenderScene {
    let materials = (0..3)
        .map(|index| {
            SceneMaterial::new(SceneMaterialParts {
                albedo: TextureHandle::from_raw(index),
                normal: Some(TextureHandle::from_raw(3)),
                roughness_metallic: Some(TextureHandle::from_raw(4)),
                ambient_occlusion: None,
                self_illumination: None,
                roughness_micro: 1_000_000,
                metalness_micro: 0,
                alpha_mode: SceneAlphaMode::Opaque,
                double_sided: false,
            })
            .unwrap()
        })
        .collect();
    let order = if reversed { [2, 1, 0] } else { [0, 1, 2] };
    RenderScene::new(SceneParts {
        resources: SceneResources::new(
            vec![
                color_texture([255, 0, 0, 255]),
                color_texture([0, 255, 0, 255]),
                color_texture([0, 0, 255, 255]),
                color_texture([128, 128, 255, 255]),
                color_texture([0, 255, 0, 255]),
            ],
            materials,
        )
        .unwrap(),
        geometry: SceneGeometry::new(Vec::new(), Vec::new()).unwrap(),
        terrain: Some(
            SceneTerrain::new(SceneTerrainParts {
                columns: 2,
                rows: 2,
                origin_east_millimeters: -2_000,
                origin_north_millimeters: -2_000,
                sample_spacing_millimeters: 4_000,
                heights_millimeters: vec![0; 4],
                surface_indices: [0, 1, 2, 0]
                    .map(|index| if reversed { 2 - index } else { index })
                    .to_vec(),
                surfaces: order
                    .map(|index| {
                        SceneTerrainSurface::new(
                            MaterialHandle::from_raw(index),
                            SceneTerrainAppearance {
                                blur_radius_millimeters,
                                ..SceneTerrainAppearance::NEUTRAL
                            },
                        )
                    })
                    .to_vec(),
                lod_policy: TerrainLodPolicy::FullResolution,
            })
            .unwrap(),
        ),
        effects: Some(effects()),
    })
    .unwrap()
}

#[test]
fn required_gpu_terrain_palette_permutation_preserves_pixels() {
    let context = pollster::block_on(GpuContext::request_with_features(wgpu::Features::empty()))
        .expect("terrain blend acceptance requires a GPU adapter");
    let view = SceneView::new(
        CameraPose {
            eye_millimeters: [0, -3_000, 5_000],
            target_millimeters: [0, 0, 0],
        },
        camera(),
        Viewport {
            width: 96,
            height: 96,
        },
    )
    .unwrap();
    let original = capture_scene(&context, &colored_terrain(false, 1_000), &view).unwrap();
    let permuted = capture_scene(&context, &colored_terrain(true, 1_000), &view).unwrap();
    assert!(
        original
            .pixels
            .chunks_exact(4)
            .any(|pixel| { pixel[0] > 40 && pixel[1] > 40 && pixel[2] > 40 }),
        "terrain must contain a mixed-color transition"
    );
    assert!(
        original
            .pixels
            .iter()
            .zip(&permuted.pixels)
            .all(|(a, b)| a.abs_diff(*b) <= 1),
        "dense palette ordering must not change the material mixture"
    );
    let sharp = capture_scene(&context, &colored_terrain(false, 0), &view).unwrap();
    assert_ne!(
        original.pixels, sharp.pixels,
        "authored blur radius must affect boundary pixels"
    );
}
