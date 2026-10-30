#[test]
fn sharpened_coverage_half_boundary_survives_derivative_clamping_and_rounding() {
    let context = context();
    let geometry = include_str!("../../shader/geometry.wgsl");
    let helper = geometry
        .split("fn scene_coverage_alpha")
        .nth(1)
        .unwrap()
        .split("fn scene_mask_alpha")
        .next()
        .unwrap();
    for width in [0.0, 1e-12, 0.03125, 1.0, 1e10] {
        for bits in [0x3eff_ffff_u32, 0x3f00_0000, 0x3f00_0001] {
            let alpha = f32::from_bits(bits);
            let shader = format!(
                "fn scene_coverage_alpha{helper}\n@fragment fn fs_screen() -> @location(0) f32 {{ return scene_coverage_alpha(bitcast<f32>({bits}u), 0.5, {width:?}) + textureLoad(unused, vec2<i32>(0), 0).r * 0.0; }}"
            );
            let output = channel_view(
                &context,
                &crate::ssao::passes::texture(context.device(), [32, 32], TextureFormat::R32Float)
                    .create_view(&TextureViewDescriptor::default()),
                &format!("@group(0) @binding(0) var unused: texture_2d<f32>;\n{shader}"),
            );
            assert_eq!(
                output[0] >= 0.5,
                alpha >= 0.5,
                "alpha {alpha}, width {width}: {}",
                output[0]
            );
            if bits == 0x3f00_0000 {
                assert_eq!(output[0].to_bits(), 0.5_f32.to_bits());
            }
        }
    }
}

use super::*;

fn receiver_material(masked: bool) -> SceneMaterial {
    SceneMaterial::new(SceneMaterialParts {
        albedo: TextureHandle::from_raw(u32::from(!masked)),
        normal: None,
        roughness_metallic: None,
        ambient_occlusion: None,
        self_illumination: None,
        roughness_micro: if masked { 250_000 } else { 750_000 },
        metalness_micro: 0,
        alpha_mode: if masked {
            SceneAlphaMode::Mask {
                cutoff_micro: 515_625,
            }
        } else {
            SceneAlphaMode::Opaque
        },
        double_sided: true,
    })
    .unwrap()
    .with_alpha_to_coverage(masked)
    .unwrap()
}

fn coverage_scene() -> RenderScene {
    let gradient = SceneTexture::Texels {
        format: worldrunner_render_plan::TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Linear,
        width: 2,
        height: 1,
        mips: 1,
        bytes: vec![255, 255, 255, 0, 255, 255, 255, 255].into(),
    };
    let vertices = [
        ([-1000, 0, -1000], [0, 0]),
        ([1000, 0, -1000], [500_000, 0]),
        ([1000, 0, 1000], [1_000_000, 0]),
        ([-1000, 0, 1000], [500_000, 0]),
    ]
    .map(|(position_millimeters, texcoord_micro)| SceneVertex {
        position_millimeters,
        texcoord_micro,
        tangent_micro: None,
        normal_micro: [0, -1_000_000, 0],
    });
    let mesh = SceneMesh::new(vec![
        SceneMeshLod::new(0, vertices.to_vec(), vec![0, 1, 2, 0, 2, 3], None).unwrap(),
    ])
    .unwrap();
    let objects = [2000, 4000]
        .into_iter()
        .enumerate()
        .map(|(index, distance)| {
            let mut placement = identity();
            placement.position_millimeters[1] = distance;
            SceneObject::new(
                placement,
                MeshHandle::from_raw(0),
                MaterialHandle::from_raw(u32::try_from(index).unwrap()),
            )
        })
        .collect();
    RenderScene::new(SceneParts {
        resources: SceneResources::new(
            vec![gradient, white_texture()],
            vec![receiver_material(true), receiver_material(false)],
        )
        .unwrap(),
        geometry: SceneGeometry::new(vec![mesh], objects).unwrap(),
        terrain: None,
        effects: Some(
            SceneEffects::new(effect_parts())
                .unwrap()
                .with_ssr(request())
                .unwrap(),
        ),
    })
    .unwrap()
}

fn coverage_frame(source: &RenderScene) -> FrameUniform {
    let view = SceneView::orthographic_with_up(
        CameraPose {
            eye_millimeters: [0; 3],
            target_millimeters: [0, 1000, 0],
        },
        crate::view::OrthographicLens {
            vertical_half_extent_meters: 1.0,
            near_meters: 0.1,
            far_meters: 10.0,
        },
        [0.0, 0.0, 1.0],
        Viewport {
            width: 32,
            height: 32,
        },
    )
    .unwrap();
    FrameUniform::new(&view, source.effects()).unwrap()
}

#[test]
fn masked_ssr_silhouettes_share_pixel_centre_majority_receivers() {
    let context = context();
    let source = coverage_scene();
    let frame = coverage_frame(&source);
    let renderer = render_with_environment(&context, &source, 4, &environment(&context), &frame);
    let screen = renderer.ssao.as_ref().unwrap();
    let ssr = renderer.ssr.as_ref().unwrap();
    let depth = channel(&context, screen.passes.depth_levels()[0], 0, 0);
    let roughness = channel(&context, ssr.material_target(), 0, 3);
    let normals = channel(&context, &screen.normals, 0, 3);
    let lanes: Vec<_> = (0..4)
        .map(|sample| channel(&context, renderer.targets().depth().texture(), sample, 0))
        .collect();
    for y in 8..24 {
        for (x, covered, expected_depth, expected_roughness) in
            [(y, 1, 4.0, 0.75_f32), (y + 1, 3, 2.0, 0.25)]
        {
            let i = y * 32 + x;
            assert!(
                (depth[i] - expected_depth).abs() < 0.00001,
                "receiver depth at {x},{y}: {}",
                depth[i]
            );
            assert_eq!(
                roughness[i].to_bits(),
                expected_roughness.to_bits(),
                "roughness at {x},{y}"
            );
            assert_eq!(normals[i].to_bits(), 1.0_f32.to_bits());
            let foreground_depth = (frame.view_projection * glam::Vec4::new(0.0, 2.0, 0.0, 1.0)).z;
            let count = lanes
                .iter()
                .filter(|lane| (lane[i] - foreground_depth).abs() < 0.00001)
                .count();
            assert_eq!(count, covered, "forward ownership at {x},{y}");
        }
    }
}
