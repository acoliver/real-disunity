use super::*;
use worldrunner_render_plan::{SceneSsrGridDomain, SceneSsrHitFacing, SceneSsrTraversal};

#[test]
fn water_hit_facing_existing_sources_stay_byte_identical() {
    for samples in [1, 4] {
        for water in [false, true] {
            let baseline = gpu::trace_source(samples, water);
            for (facing, expected) in [
                (SceneSsrHitFacing::ShadingNormal, baseline.clone()),
                (
                    SceneSsrHitFacing::Any,
                    baseline.replace(" || dot(hit_normal.xyz, direction) >= 0.0", ""),
                ),
            ] {
                let settings = SceneSsr {
                    hit_facing: facing,
                    ..Default::default()
                };
                assert_eq!(
                    grid::trace_source(samples, water, &settings, [65, 65]).as_bytes(),
                    expected.as_bytes()
                );
            }
        }
    }
}

#[test]
fn water_hit_facing_generic_source_is_shading_normal_source() {
    for traversal in [SceneSsrTraversal::Conservative, SceneSsrTraversal::Grid] {
        for samples in [1, 4] {
            for scale in [500_000, 1_000_000] {
                let policy = SceneSsr {
                    traversal,
                    resolution_scale_micro: scale,
                    grid_domain: SceneSsrGridDomain::ReflectionOutput,
                    ..settings()
                };
                let shading = grid::trace_source(samples, false, &policy, [65, 65]);
                let water_only = SceneSsr {
                    hit_facing: SceneSsrHitFacing::AnyForWaterReceivers,
                    ..policy
                };
                assert_eq!(
                    grid::trace_source(samples, false, &water_only, [65, 65]).as_bytes(),
                    shading.as_bytes()
                );
            }
        }
    }
}

#[test]
fn water_hit_facing_source_carries_receiver_policy_to_both_paths() {
    for samples in [1, 4] {
        let policy = SceneSsr {
            hit_facing: SceneSsrHitFacing::AnyForWaterReceivers,
            ..settings()
        };
        let source = grid::trace_source(samples, true, &policy, [65, 65]);
        assert!(source.contains("let receiver_is_water = water_normal.a != 0.0;"));
        assert!(source.contains("!receiver_is_water && dot(hit_normal.xyz, direction) >= 0.0"));
        assert!(
            source.contains("!policy.receiver_is_water && dot(hit_normal.xyz, direction) >= 0.0")
        );
        let module = naga::front::wgsl::parse_str(&source).unwrap();
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .unwrap();
    }
}

fn mixed_capture(context: &GpuContext, policy: SceneSsr) -> Vec<u8> {
    let size = [65, 65];
    let config = FrameConfig::new(size, wgpu::TextureFormat::Rgba16Float, 1);
    let mut screen = SceneMetadata::new(context.device(), size, neutral_ao()).unwrap();
    let ssr = Ssr::new(context.device(), config, policy, &mut screen).unwrap();
    let frame = frame();
    let (depths, mut normals, materials, colors) =
        mirror_data(&frame, 0.0, glam::Vec2::new(0.65, 0.35), false);
    let mut water_normals = vec![0.0; normals.len()];
    for (i, depth) in depths.iter().enumerate() {
        if depth.to_bits() == 5.0f32.to_bits() {
            normals[i * 4..i * 4 + 4].copy_from_slice(&[0.0, 0.0, -1.0, 1.0]);
        } else if i % 65 <= 32 {
            water_normals[i * 4..i * 4 + 4].copy_from_slice(&normals[i * 4..i * 4 + 4]);
        }
    }
    let mut water = water_tests::water_fixture(context, config);
    for texture in &mut water.receivers[0][..2] {
        *texture =
            crate::ssao::passes::texture(context.device(), size, wgpu::TextureFormat::Rgba16Float);
    }
    upload(context, &water.receivers[0][0], &water_normals);
    upload(
        context,
        &water.receivers[0][1],
        &[0.5, 0.5, 0.0, 0.0].repeat(65 * 65),
    );
    let depth = mirror_water_depth(context, config, &normals);
    upload(context, &ssr.normals, &normals);
    upload(context, &ssr.material, &materials);
    upload_hierarchy(context, &ssr, size, depths);
    let opaque =
        crate::ssao::passes::texture(context.device(), size, wgpu::TextureFormat::Rgba16Float);
    upload(context, &opaque, &colors);
    draw_mixed(context, &ssr, &water, &depth, &opaque)
}

fn mirror_water_depth(context: &GpuContext, config: FrameConfig, normals: &[f32]) -> wgpu::Texture {
    let projection = frame().view_projection;
    let i = (45 * 65 + 32) * 4;
    let normal = glam::Vec3::from_slice(&normals[i..i + 3]);
    let point = projection.inverse() * glam::Vec4::new(0.0, -0.4, 0.0, 1.0);
    let origin = point.truncate() * (-3.0 / point.z);
    let metres = format!(
        "{} / dot(vec3<f32>((p.x / 65.0 * 2.0 - 1.0) / {}, (1.0 - p.y / 65.0 * 2.0) / {}, -1.0), vec3<f32>({}, {}, {}))",
        origin.dot(normal),
        projection.x_axis.x,
        projection.y_axis.y,
        normal.x,
        normal.y,
        normal.z
    );
    water_tests::raw_depth::texture(context, config, &metres)
}

fn draw_mixed(
    context: &GpuContext,
    ssr: &Ssr,
    water: &crate::water::Water,
    depth: &wgpu::Texture,
    opaque: &wgpu::Texture,
) -> Vec<u8> {
    let mut encoder = context.submission().begin(context.device()).unwrap();
    let mut feature = ReflectionFeature {
        water: None,
        ssr,
        frame: frame(),
        color: AttachmentView::new(ResourceRef::new("scene color"), opaque).unwrap(),
        groups: vec![],
    };
    let mut prepare = PrepareContext::new(context.device(), &mut encoder).unwrap();
    feature.commit(&mut prepare);
    prepare.finish();
    let views = mixed_views(ssr, water, depth, opaque);
    let group = gpu::group(
        context.device(),
        &ssr.water_trace,
        Some(&ssr.uniform),
        &views,
    );
    worldrunner_render_post::fullscreen_pass(
        &mut encoder,
        Some("mixed water and opaque facing receivers"),
        wgpu::RenderPassColorAttachment {
            view: &ssr.reflections[0].create_view(&wgpu::TextureViewDescriptor::default()),
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                store: wgpu::StoreOp::Store,
            },
        },
        &ssr.water_trace,
        &[&group],
    );
    encoder.finish().submit().unwrap();
    read(context, &ssr.reflections[0])
}

fn mixed_views(
    ssr: &Ssr,
    water: &crate::water::Water,
    depth: &wgpu::Texture,
    opaque: &wgpu::Texture,
) -> Vec<wgpu::TextureView> {
    let mut views = [opaque, &ssr.normals, &ssr.material]
        .map(|t| t.create_view(&wgpu::TextureViewDescriptor::default()))
        .to_vec();
    views.extend(
        ssr.hierarchy
            .iter()
            .map(|t| t.create_view(&wgpu::TextureViewDescriptor::default())),
    );
    views.extend(
        [
            &ssr.source_depth,
            &ssr.source_depth,
            &water.receivers[0][0],
            &water.receivers[0][1],
            depth,
        ]
        .map(|t| t.create_view(&wgpu::TextureViewDescriptor::default())),
    );
    views
}

fn mixed_receiver(bytes: &[u8], coefficient: u32, water: bool) -> &[u8] {
    let size = 65u32.div_ceil(coefficient);
    let y = if coefficient == 2 { 23 } else { 45 };
    let x = size / 2 + u32::from(!water);
    let offset = usize::try_from((y * size + x) * 8).unwrap();
    &bytes[offset..offset + 8]
}

#[test]
fn gpu_water_hit_facing_accepts_water_and_rejects_opaque_in_one_frame() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    for (traversal, coefficient) in [
        (SceneSsrTraversal::Conservative, 1),
        (SceneSsrTraversal::Conservative, 2),
        (SceneSsrTraversal::Grid, 1),
        (SceneSsrTraversal::Grid, 2),
    ] {
        let policy = SceneSsr {
            traversal,
            grid_domain: SceneSsrGridDomain::ReflectionOutput,
            resolution_scale_micro: 1_000_000 / coefficient,
            ..settings()
        };
        for facing in [
            SceneSsrHitFacing::Any,
            SceneSsrHitFacing::ShadingNormal,
            SceneSsrHitFacing::AnyForWaterReceivers,
        ] {
            let bytes = mixed_capture(
                &context,
                SceneSsr {
                    hit_facing: facing,
                    ..policy
                },
            );
            for water in [true, false] {
                let pixel = mixed_receiver(&bytes, coefficient, water);
                let accepted = facing == SceneSsrHitFacing::Any
                    || (water && facing == SceneSsrHitFacing::AnyForWaterReceivers);
                let confidence =
                    ::half::f16::from_le_bytes(pixel[6..8].try_into().unwrap()).to_f32();
                assert_eq!(
                    confidence > 0.0,
                    accepted,
                    "{traversal:?}, {coefficient}, {facing:?}, water={water}"
                );
                if accepted {
                    assert_eq!(&pixel[..2], &pixel[6..8]);
                    assert_eq!(&pixel[2..6], [0; 4]);
                } else {
                    assert_eq!(pixel, [0; 8]);
                }
            }
        }
    }
}
