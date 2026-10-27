//! Rendered equivalence between mapped factors and effective scalar materials.

use super::*;
use glam::Vec3;
use worldrunner_render_plan::TexelFormat;

const ROUGHNESS_CHANNEL: u32 = 128;
const METALNESS_CHANNEL: u32 = 64;
const UNIT: u32 = 1_000_000;

fn constant_texture(rgba: [u8; 4], color_space: TextureColorSpace) -> SceneTexture {
    SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space,
        width: 1,
        height: 1,
        mips: 1,
        bytes: Box::new(rgba),
    }
}

fn factor_scene(mapped: bool, roughness_micro: u32, metalness_micro: u32) -> RenderScene {
    let material = SceneMaterial::new(SceneMaterialParts {
        albedo: TextureHandle::from_raw(0),
        normal: None,
        roughness_metallic: mapped.then_some(TextureHandle::from_raw(1)),
        ambient_occlusion: None,
        self_illumination: None,
        roughness_micro,
        metalness_micro,
        alpha_mode: SceneAlphaMode::Opaque,
        double_sided: false,
    })
    .expect("factor fixture scalars are in range");
    RenderScene::new(SceneParts {
        resources: SceneResources::new(
            vec![
                constant_texture([150, 100, 60, 255], TextureColorSpace::Srgb),
                // Deliberately distinct R/G/B catches channel swaps as well as
                // accidentally decoding these measurements as sRGB.
                constant_texture([231, 128, 64, 255], TextureColorSpace::Linear),
            ],
            vec![material],
        )
        .expect("factor fixture resources validate"),
        geometry: SceneGeometry::new(
            vec![quad()],
            vec![SceneObject::new(
                identity(),
                MeshHandle::from_raw(0),
                MaterialHandle::from_raw(0),
            )],
        )
        .expect("factor fixture geometry validates"),
        terrain: None,
        effects: Some(effects()),
    })
    .expect("factor fixture scene validates")
}

fn effective_scalar(channel: u32, factor: u32) -> u32 {
    // Nearest micro-unit; this oracle only describes material inputs, not BRDFs.
    (channel * factor + 127) / 255
}

fn specular_frame() -> FrameUniform {
    FrameUniform {
        background_far_plane_meters: 0.0,
        sun_direction: Vec3::new(0.0, 0.6, 0.8),
        sun_irradiance: Vec3::splat(0.2),
        ambient_sky: Vec3::ZERO,
        ambient_ground: Vec3::ZERO,
        ..frame_from(pose())
    }
}

fn interior_rgb(pixels: &[u8]) -> Vec<u8> {
    // Central 8x8 patch is well inside both triangles' silhouette, away from
    // rasterization edges. Keep spatial samples: averaging can hide errors.
    (28..36)
        .flat_map(|y| {
            (28..36).flat_map(move |x| {
                let offset = (y * TARGET as usize + x) * 4;
                assert_eq!(pixels[offset + 3], 255);
                pixels[offset..offset + 3].iter().copied()
            })
        })
        .collect()
}

fn assert_rendered_factors(cases: &[(u32, u32)]) {
    let context = pollster::block_on(GpuContext::request())
        .expect("material pixel regression requires a GPU adapter; absence is a failure");
    let info = context.adapter().get_info();
    println!("material factors: {} via {:?}", info.name, info.backend);
    let mut results = Vec::new();
    for &(roughness, metalness) in cases {
        let mapped = interior_rgb(&render_frame(
            &context,
            &factor_scene(true, roughness, metalness),
            &specular_frame(),
        ));
        let scalar = interior_rgb(&render_frame(
            &context,
            &factor_scene(
                false,
                effective_scalar(ROUGHNESS_CHANNEL, roughness),
                effective_scalar(METALNESS_CHANNEL, metalness),
            ),
            &specular_frame(),
        ));
        // The oracle rounds effective factors by <= 0.5 micro-unit. Allow one
        // sRGB8 code for that rounding crossing a render-target quantization
        // boundary, not a percentage tolerance on the lighting result.
        let maximum_error = mapped
            .iter()
            .zip(&scalar)
            .map(|(actual, expected)| actual.abs_diff(*expected))
            .max()
            .expect("the interior patch is nonempty");
        println!(
            "factors ({roughness}, {metalness}): center RGB {:?}, scalar {:?}, max error {maximum_error}",
            &mapped[108..111],
            &scalar[108..111]
        );
        assert!(
            mapped
                .iter()
                .chain(&scalar)
                .all(|value| (1..255).contains(value)),
            "factor ({roughness}, {metalness}) must stay lit and nonsaturating"
        );
        assert!(
            maximum_error <= 1,
            "factor ({roughness}, {metalness}) differs from channel * scalar by {maximum_error} sRGB8 codes"
        );
        results.push(mapped);
    }
    for (index, left) in results.iter().enumerate() {
        for (other, right) in results.iter().enumerate().skip(index + 1) {
            let changed = left
                .iter()
                .zip(right)
                .filter(|(left, right)| left.abs_diff(**right) >= 3)
                .count();
            assert!(
                changed >= 64,
                "factor cases {:?} and {:?} must visibly differ across the interior (only {changed} channels differ by >= 3 codes)",
                cases[index],
                cases[other]
            );
        }
    }
}

#[test]
fn rendered_roughness_factors_match_effective_scalars() {
    assert_rendered_factors(&[(UNIT, UNIT), (0, UNIT), (600_000, UNIT)]);
}

#[test]
fn rendered_metalness_factors_match_effective_scalars() {
    assert_rendered_factors(&[(UNIT, UNIT), (UNIT, 0), (UNIT, 400_000)]);
}
