use super::*;
use worldrunner_render_plan::{SceneImpostor, SceneImpostorLayout, TexelFormat};

fn cropped_scene(shaped: bool) -> RenderScene {
    let mut vertices = card(Some(BillboardPolicy::YawLocked)).lods()[0]
        .vertices()
        .to_vec();
    if !shaped {
        for v in &mut vertices {
            // Independently retain source U=0.5..1 on the right half of a two-metre plane.
            v.texcoord_micro[0] = v.texcoord_micro[0].midpoint(MICRO);
            v.position_millimeters = [
                v.texcoord_micro[0] / 500 - 1000,
                0,
                v.texcoord_micro[1] / 500 - 1000,
            ];
        }
    }
    let mesh = SceneMesh::new(vec![
        SceneMeshLod::new(
            0,
            vertices,
            vec![0, 1, 2, 0, 2, 3],
            Some(BillboardPolicy::YawLocked),
        )
        .unwrap(),
    ])
    .unwrap();
    let textures = cropped_textures();
    let mut atlas = SceneImpostor::new(SceneImpostorLayout::AzimuthElevation, [1, 1], 0, false)
        .unwrap()
        .with_plane_size([2000; 2])
        .unwrap();
    if shaped {
        atlas = atlas.with_borders(TextureHandle::from_raw(1));
    }
    RenderScene::new(SceneParts {
        resources: SceneResources::new(
            textures,
            vec![
                material(
                    SceneAlphaMode::Mask {
                        cutoff_micro: 500_000,
                    },
                    true,
                )
                .with_impostor(atlas),
            ],
        )
        .unwrap(),
        geometry: SceneGeometry::new(
            vec![mesh],
            vec![SceneObject::new(
                identity(),
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

#[test]
fn asymmetric_border_map_crops_instead_of_rescaling_source_image() {
    let context = context();
    let layouts = ScenePipelines::new(context.device(), COLOR_FORMAT);
    let mut images = Vec::new();
    for shaped in [false, true] {
        let gpu = SceneGpu::upload(
            context.device(),
            context.submission(),
            &cropped_scene(shaped),
            &layouts,
        )
        .unwrap();
        let mut frame = frame_from(CameraPose {
            eye_millimeters: [0, -4000, 0],
            target_millimeters: [0; 3],
        });
        frame.unlit = true;
        gpu.set_frame(context.queue(), frame);
        images.push(super::foliage::pass_color(
            &context,
            &gpu,
            &layouts,
            crate::shader::ENVIRONMENT_WGSL,
            "fs_masked",
        ));
    }
    assert!(images[0].chunks_exact(4).any(|p| p[0] > 100));
    for (actual, expected) in images[1].iter().zip(&images[0]) {
        assert!(actual.abs_diff(*expected) <= 1);
    }
}

#[test]
fn pitched_relief_instance_normals_match_inverse_capture_rotation() {
    let context = context();
    let layouts = ScenePipelines::new(context.device(), COLOR_FORMAT);
    let source = pitched_scene();
    let gpu = SceneGpu::upload(context.device(), context.submission(), &source, &layouts).unwrap();
    gpu.set_frame(
        context.queue(),
        frame_from(CameraPose {
            eye_millimeters: [0, -4000, 1000],
            target_millimeters: [0, 0, 1000],
        }),
    );
    let diagnostic = format!(
        "{}\n@fragment fn fs_normal_oracle(input: VertexOutput) -> @location(0) vec4<f32> {{ return vec4(sample_normal(input, input.world_normal, input.world_position, input.world_tangent)*0.5+0.5,1.0); }}",
        crate::shader::ENVIRONMENT_WGSL
    );
    let metadata = super::foliage::geometry_sources()[1]
        .replace("vec4<f32>(normal, 1.0)", "vec4<f32>(normal*0.5+0.5, 1.0)");
    // RG=128/255 decodes close to +Z. Inverse pitch maps it to +Y, not +Z.
    let a = (128.0_f64 / 255.0 - 0.5) * 8.0_f64.sqrt();
    let z = 1.0 - 2.0 * a * a;
    let xy = a * (1.0 + z).sqrt();
    let expected = [xy, z, -xy].map(|n| {
        let linear = n * 0.5 + 0.5;
        (1.055 * linear.powf(1.0 / 2.4) - 0.055) * 255.0
    });
    for (shader, entry) in [(&diagnostic, "fs_normal_oracle"), (&metadata, "fs_opaque")] {
        let pixels = super::foliage::pass_color(&context, &gpu, &layouts, shader, entry);
        let center = ((TARGET / 2 * TARGET + TARGET / 2) * 4) as usize;
        for (actual, expected) in pixels[center..center + 3].iter().zip(expected) {
            assert!(
                (f64::from(*actual) - expected).abs() < 1.0,
                "{actual} vs {expected}"
            );
        }
    }
}

fn cropped_textures() -> Vec<SceneTexture> {
    let mut bytes = Vec::new();
    for y in 0..16_u8 {
        for x in 0..16_u8 {
            bytes.extend_from_slice(&[
                x * 16,
                y * 16,
                64,
                if (9..14).contains(&x) && (2..11).contains(&y) {
                    255
                } else {
                    0
                },
            ]);
        }
    }
    vec![
        SceneTexture::Texels {
            format: TexelFormat::Rgba8Unorm,
            color_space: TextureColorSpace::Srgb,
            width: 16,
            height: 16,
            mips: 1,
            bytes: bytes.into(),
        },
        SceneTexture::Texels {
            format: TexelFormat::Rgba8Unorm,
            color_space: TextureColorSpace::Linear,
            width: 1,
            height: 1,
            mips: 1,
            bytes: vec![255, 0, 0, 0].into(),
        },
    ]
}

fn pitched_scene() -> RenderScene {
    let base = super::impostor::relief_scene();
    let mut materials = base.resources().materials().to_vec();
    materials[0] = materials[0].with_impostor(
        SceneImpostor::new(SceneImpostorLayout::AzimuthElevation, [4, 1], 0, false)
            .unwrap()
            .with_relief(TextureHandle::from_raw(2), 100)
            .unwrap(),
    );
    let mut transform = identity();
    transform.rotation = SceneRotation {
        x: 707_107,
        y: 0,
        z: 0,
        w: 707_107,
    };
    RenderScene::new(SceneParts {
        resources: SceneResources::new(base.resources().textures().to_vec(), materials).unwrap(),
        geometry: SceneGeometry::new(
            base.geometry().meshes().to_vec(),
            vec![SceneObject::new(
                transform,
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
