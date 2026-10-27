use super::foliage::{geometry_sources, pass_depth};
use super::*;
use worldrunner_render_plan::{SceneImpostor, SceneImpostorLayout, TexelFormat};

fn texture(alphas: &[u8], width: u32) -> SceneTexture {
    SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Srgb,
        width,
        height: 1,
        mips: 1,
        bytes: alphas.iter().flat_map(|&a| [150, 100, 60, a]).collect(),
    }
}

fn source(texture: SceneTexture, factor: u32, cutoff: u32, atlas: bool) -> RenderScene {
    let mut material = material(
        SceneAlphaMode::Mask {
            cutoff_micro: cutoff,
        },
        true,
    )
    .with_alpha_factor(factor)
    .unwrap();
    if atlas {
        material = material.with_impostor(
            SceneImpostor::new(SceneImpostorLayout::AzimuthElevation, [2, 1], 0, true).unwrap(),
        );
    }
    RenderScene::new(SceneParts {
        resources: SceneResources::new(vec![texture], vec![material]).unwrap(),
        geometry: SceneGeometry::new(
            vec![if atlas {
                card(Some(BillboardPolicy::YawLocked))
            } else {
                quad()
            }],
            vec![SceneObject::new(
                identity(),
                MeshHandle::from_raw(0),
                MaterialHandle::from_raw(0),
            )],
        )
        .unwrap(),
        terrain: None,
        effects: Some(effects()),
    })
    .unwrap()
}

fn depths(context: &GpuContext, scene: &RenderScene) -> Vec<Vec<u8>> {
    let pipelines = ScenePipelines::new(context.device(), COLOR_FORMAT);
    let authority =
        worldrunner_render_gpu::submission::SubmissionAuthority::new(context.queue().clone());
    let gpu = SceneGpu::upload(context.device(), &authority, scene, &pipelines).unwrap();
    gpu.set_frame(context.queue(), frame_from(pose()));
    geometry_sources()
        .iter()
        .enumerate()
        .map(|(index, source)| {
            pass_depth(
                context,
                &gpu,
                &pipelines,
                source,
                if index == 3 {
                    "fs_depth_masked"
                } else {
                    "fs_masked"
                },
                index != 3,
            )
        })
        .collect()
}

#[test]
fn odd_alpha_cutoffs_match_rational_oracle_in_all_passes_and_impostors() {
    let context = context();
    for atlas in [false, true] {
        let full = depths(
            &context,
            &source(texture(&[255; 2], 2), 1_000_000, 1, atlas),
        );
        assert!(
            full[0]
                .chunks_exact(4)
                .any(|pixel| pixel != 1.0_f32.to_le_bytes())
        );
        for (alpha, cutoff) in [(1_u8, 3000_u32), (3, 5000), (127, 250_000), (129, 250_000)] {
            // Compare integers, independently of GPU UNORM conversion and multiplication.
            let covered = u64::from(alpha) * 500_000 >= u64::from(cutoff) * 255;
            let actual = depths(
                &context,
                &source(texture(&[alpha; 2], 2), 500_000, cutoff, atlas),
            );
            let expected = if covered {
                full[0].clone()
            } else {
                1.0_f32
                    .to_le_bytes()
                    .repeat(usize::try_from(TARGET * TARGET).unwrap())
            };
            for pass in actual {
                assert_eq!(pass, expected, "alpha={alpha}, atlas={atlas}");
            }
        }
    }
}

#[test]
fn bilinear_alpha_is_scaled_after_filtering_without_byte_baking() {
    let context = context();
    let factored = source(texture(&[1, 3], 2), 500_000, 4500, false);
    // Doubling the threshold is an independent oracle for a half-alpha factor.
    let oracle = source(texture(&[1, 3], 2), 1_000_000, 9000, false);
    let baked = source(texture(&[1, 2], 2), 1_000_000, 4500, false);
    let actual = depths(&context, &factored);
    assert_eq!(actual, depths(&context, &oracle));
    assert_ne!(actual[0], depths(&context, &baked)[0]);
    assert!(
        actual[0]
            .chunks_exact(4)
            .any(|pixel| pixel == 1.0_f32.to_le_bytes())
    );
    assert!(
        actual[0]
            .chunks_exact(4)
            .any(|pixel| pixel != 1.0_f32.to_le_bytes())
    );
}

#[test]
fn explicit_unit_alpha_factor_keeps_default_render_bytes() {
    let context = context();
    let stock = scene(
        SceneAlphaMode::Mask {
            cutoff_micro: 500_000,
        },
        true,
        1,
    );
    let mut materials = stock.resources().materials().to_vec();
    materials[0] = materials[0].with_alpha_factor(1_000_000).unwrap();
    let explicit = RenderScene::new(SceneParts {
        resources: SceneResources::new(stock.resources().textures().to_vec(), materials).unwrap(),
        geometry: stock.geometry().clone(),
        terrain: None,
        effects: stock.effects().cloned(),
    })
    .unwrap();
    assert_eq!(render(&context, &stock), render(&context, &explicit));
}

#[test]
fn unlit_output_scales_only_alpha_before_blending() {
    let context = context();
    let pipelines = ScenePipelines::new(context.device(), COLOR_FORMAT);
    let authority =
        worldrunner_render_gpu::submission::SubmissionAuthority::new(context.queue().clone());
    let forward = &geometry_sources()[0];
    let mut reference = None;
    for (factor, alpha) in [(1_000_000, 201), (300_000, 60), (0, 0)] {
        let scene = source(texture(&[201], 1), factor, 1, false);
        let gpu = SceneGpu::upload(context.device(), &authority, &scene, &pipelines).unwrap();
        gpu.set_frame(context.queue(), frame_from(pose()));
        let pixels = super::foliage::pass_color(&context, &gpu, &pipelines, forward, "fs_unlit");
        let interior: Vec<_> = pixels
            .chunks_exact(4)
            .filter(|pixel| pixel[0] != 0)
            .collect();
        assert!(!interior.is_empty());
        // 201 * 0.3 = 60.3, separated from the UNORM rounding boundary by 0.2.
        assert!(interior.iter().all(|pixel| pixel[3] == alpha));
        let rgb: Vec<_> = interior
            .iter()
            .flat_map(|pixel| &pixel[..3])
            .copied()
            .collect();
        if let Some(reference) = &reference {
            assert_eq!(&rgb, reference);
        } else {
            reference = Some(rgb);
        }
    }
}
