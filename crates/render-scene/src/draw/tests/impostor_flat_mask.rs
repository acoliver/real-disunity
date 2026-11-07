use super::*;
use worldrunner_render_plan::{
    SceneAtlasSampling, SceneImpostor, SceneImpostorLayout, TexelFormat,
};

#[test]
fn octahedral_flat_mask_matches_forward_prepass_metadata_and_shadow() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let layouts = ScenePipelines::new(context.device(), COLOR_FORMAT);
    for sampling in [SceneAtlasSampling::MipZero, SceneAtlasSampling::Implicit] {
        let gpu = upload(&context, &layouts, &source(false, false, sampling));
        let mut reference = None;
        for (index, shader) in super::foliage::geometry_sources().iter().enumerate() {
            let shadow = index == 3;
            let entry = if shadow {
                "fs_depth_masked"
            } else {
                "fs_masked"
            };
            let depth =
                super::foliage::pass_depth(&context, &gpu, &layouts, shader, entry, !shadow);
            println!(
                "{sampling:?} geometry pass {index} covered pixels: {}",
                count(&depth)
            );
            assert!(count(&depth) > 0);
            if let Some(reference) = &reference {
                assert_eq!(&depth, reference, "geometry pass {index}");
            } else {
                reference = Some(depth);
            }
        }
    }
}

fn linear_atlas(texel: [u8; 4]) -> SceneTexture {
    SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Linear,
        width: 48,
        height: 48,
        mips: 1,
        bytes: texel.repeat(48 * 48).into(),
    }
}

fn surface() -> SceneMaterial {
    SceneMaterial::new(SceneMaterialParts {
        albedo: TextureHandle::from_raw(0),
        normal: Some(TextureHandle::from_raw(2)),
        roughness_metallic: None,
        ambient_occlusion: None,
        self_illumination: None,
        roughness_micro: 1_000_000,
        metalness_micro: 0,
        alpha_mode: SceneAlphaMode::Mask {
            cutoff_micro: 500_000,
        },
        double_sided: true,
    })
    .unwrap()
}

fn centered_card() -> SceneMesh {
    let mesh = card(Some(BillboardPolicy::YawLocked));
    let mut vertices = mesh.lods()[0].vertices().to_vec();
    for vertex in &mut vertices {
        vertex.position_millimeters[2] -= 1000;
    }
    SceneMesh::new(vec![
        SceneMeshLod::new(
            0,
            vertices,
            vec![0, 1, 2, 0, 2, 3],
            Some(BillboardPolicy::YawLocked),
        )
        .unwrap(),
    ])
    .unwrap()
}

fn atlas_texture(constant: bool) -> SceneTexture {
    let mut bytes = Vec::new();
    for y in 0..48 {
        for x in 0..48 {
            let alpha = if constant || (4..12).contains(&(x % 16)) {
                254
            } else {
                0
            };
            let alpha = if !constant && (x / 16, y / 16) == (0, 1) {
                0
            } else {
                alpha
            };
            bytes.extend_from_slice(&[255, 255, 255, alpha]);
        }
    }
    // The coarse mip moves the covered half to the left edge of each cell.
    // A different silhouette proves that the implicit policy used a nonzero mip.
    for y in 0..24 {
        for x in 0..24 {
            let covered = x % 8 < 4 && (x / 8, y / 8) != (0, 1);
            bytes.extend_from_slice(&[255, 255, 255, if constant || covered { 254 } else { 0 }]);
        }
    }
    SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Srgb,
        width: 48,
        height: 48,
        mips: 2,
        bytes: bytes.into(),
    }
}

fn source(constant: bool, relief: bool, sampling: SceneAtlasSampling) -> RenderScene {
    let mut atlas = SceneImpostor::new(
        SceneImpostorLayout::HemisphericalOctahedral,
        [3, 3],
        0,
        true,
    )
    .unwrap()
    .with_plane_size([2000; 2])
    .unwrap()
    .with_sampling(sampling);
    if relief {
        atlas = atlas.with_relief(TextureHandle::from_raw(1), 100).unwrap();
    }
    RenderScene::new(SceneParts {
        resources: SceneResources::new(
            vec![
                atlas_texture(constant),
                linear_atlas([0; 4]),
                linear_atlas([128, 128, 0, 255]),
            ],
            vec![surface().with_impostor(atlas)],
        )
        .unwrap(),
        geometry: SceneGeometry::new(
            vec![centered_card()],
            vec![SceneObject::new(
                SceneTransform {
                    position_millimeters: [0, 0, 1000],
                    ..identity()
                },
                MeshHandle::from_raw(0),
                MaterialHandle::from_raw(0),
            )],
        )
        .unwrap(),
        terrain: None,
        effects: None,
    })
    .unwrap()
}

fn upload(context: &GpuContext, layouts: &ScenePipelines, source: &RenderScene) -> SceneGpu {
    let gpu = SceneGpu::upload(context.device(), context.submission(), source, layouts).unwrap();
    let mut frame = frame_from(CameraPose {
        eye_millimeters: [0, -4000, 1000],
        target_millimeters: [0, 0, 1000],
    });
    frame.unlit = true;
    frame.material_mip_bias = 3.0;
    gpu.set_frame(context.queue(), frame);
    gpu
}

fn coverage(context: &GpuContext, layouts: &ScenePipelines, source: &RenderScene) -> Vec<u8> {
    let gpu = upload(context, layouts, source);
    super::curved_wind::read_depth(
        context,
        &gpu,
        layouts.object_pipelines(),
        (Some(COLOR_FORMAT), None),
    )
}

fn count(bytes: &[u8]) -> usize {
    bytes
        .chunks_exact(4)
        .filter(|p| *p != 1.0_f32.to_le_bytes())
        .count()
}

#[test]
fn octahedral_flat_mask_preserves_varying_alpha_coverage_without_relief() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let layouts = ScenePipelines::new(context.device(), COLOR_FORMAT);
    let mut reference = None;
    for sampling in [SceneAtlasSampling::MipZero, SceneAtlasSampling::Implicit] {
        let control = coverage(&context, &layouts, &source(true, false, sampling));
        let flat = coverage(&context, &layouts, &source(false, false, sampling));
        let relief = coverage(&context, &layouts, &source(false, true, sampling));
        let counts = [count(&control), count(&flat), count(&relief)];
        println!("{sampling:?} constant/flat/relief covered pixels: {counts:?}");
        assert!(counts[0] > 0);
        // Half the main cell passes the cutoff, allowing filtering at contours.
        assert!(
            counts[1] > counts[0] / 3 && counts[1] < counts[0] * 2 / 3,
            "{counts:?}"
        );
        assert_eq!(
            flat, relief,
            "zero depth relief must preserve the flat mask"
        );
        if let Some(reference) = &reference {
            assert_ne!(&flat, reference, "the coarse mip has a different mask");
        } else {
            reference = Some(flat);
        }
    }
}
