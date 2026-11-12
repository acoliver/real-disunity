use super::*;
use worldrunner_render_plan::{
    SceneSsrGridDomain, SceneSsrHitFacing, SceneSsrPyramid, SceneSsrTraversal,
};

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
        assert!(source.contains(
            "let strict = select(vec4<f32>(0.0), candidate, dot(hit_normal.xyz, direction) < 0.0)"
        ));
        assert!(
            source
                .contains("ReflectionBanks(strict, select(strict, candidate, receiver_is_water))")
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

#[test]
fn water_bank_sources_keep_the_opaque_bank_fixed() {
    assert_eq!(pyramid::bank_count(&SceneSsr::default()), 1);
    let settings = SceneSsr {
        hit_facing: SceneSsrHitFacing::AnyForWaterReceivers,
        radiance_pyramid: worldrunner_render_plan::SceneSsrPyramid::Full,
        ..Default::default()
    };
    let trace = grid::trace_source(1, true, &settings, [65, 65]);
    assert!(trace.contains("struct ReflectionBanks"));
    assert!(trace.contains("@location(0) opaque_bank"));
    assert!(trace.contains("@location(1) water_bank"));
    assert!(trace.contains("receiver_is_water"));
    let resolve = gpu::configured_resolve_source_variant(
        7,
        1,
        true,
        true,
        (
            worldrunner_render_plan::SceneSsaoReceiver::Nearest,
            wgpu::TextureFormat::R32Float,
        ),
        true,
    );
    let resolve =
        pyramid::resolve_source_variant(resolve, &settings.radiance_extents([65, 65]), 1, true);
    assert!(resolve.contains("texture_2d_array<f32>"));
    assert!(resolve.contains(
        "reflection_bank_index = select(0, 1, receiver.owner == 2u && camera.policy.x != 0.0)"
    ));
    assert!(
        resolve.contains("textureLoad(reflection_0, pixel + offset, reflection_bank_index, 0)")
    );
    for lookup in resolve
        .lines()
        .filter(|line| line.contains("textureLoad(reflection_0,"))
    {
        assert!(lookup.contains("reflection_bank_index"), "{lookup}");
    }
    assert_eq!(resolve.matches("@binding(").count(), 13);
    let wgsl = format!("{}\n{resolve}", include_str!("../ssao/fullscreen.wgsl"));
    let module = naga::front::wgsl::parse_str(&wgsl)
        .unwrap_or_else(|error| panic!("{}", error.emit_to_string(&wgsl)));
    naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .unwrap_or_else(|error| panic!("{}", error.emit_to_string(&wgsl)));
}

struct MixedCapture {
    strict: Vec<u8>,
    relaxed: Option<Vec<u8>>,
    strict_mips: Vec<Vec<u8>>,
    relaxed_mips: Option<Vec<Vec<u8>>>,
    resolved: Vec<u8>,
    water_mask: Vec<bool>,
}

fn mixed_capture(context: &GpuContext, policy: SceneSsr) -> MixedCapture {
    let size = [65, 65];
    let config = FrameConfig::new(size, wgpu::TextureFormat::Rgba16Float, 1);
    let mut screen = SceneMetadata::new(context.device(), size, neutral_ao()).unwrap();
    let ssr = Ssr::new(context.device(), config, policy, &mut screen).unwrap();
    assert_eq!(
        ssr.reflections[0].depth_or_array_layers(),
        pyramid::bank_count(&policy)
    );
    let frame = frame();
    let (depths, mut normals, materials, colors) =
        mirror_data(&frame, 0.4, glam::Vec2::new(0.65, 0.35), false);
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
    let water_mask = water_normals
        .chunks_exact(4)
        .map(|normal| normal[3] != 0.0)
        .collect();
    draw_mixed(context, &ssr, &water, &depth, &opaque, water_mask)
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
    water_mask: Vec<bool>,
) -> MixedCapture {
    let mut encoder = context.submission().begin(context.device()).unwrap();
    encode_trace(context, &mut encoder, ssr, water, depth, opaque);
    encode_reductions(context, &mut encoder, ssr);
    let output = encode_resolve(context, &mut encoder, ssr, water, depth);
    encoder.finish().submit().unwrap();
    let banks = pyramid::bank_count(&ssr.settings);
    let strict_mips = read_mip_bank(context, ssr, 0);
    let relaxed_mips = (banks == 2).then(|| read_mip_bank(context, ssr, 1));
    let resolved = read_layer(context, &output, 0);
    MixedCapture {
        strict: read_mip_layer(context, ssr, 0, 0),
        relaxed: (banks == 2).then(|| read_mip_layer(context, ssr, 0, 1)),
        strict_mips,
        relaxed_mips,
        resolved,
        water_mask,
    }
}

fn encode_trace(
    context: &GpuContext,
    encoder: &mut wgpu::CommandEncoder,
    ssr: &Ssr,
    water: &crate::water::Water,
    depth: &wgpu::Texture,
    opaque: &wgpu::Texture,
) {
    let mut feature = ReflectionFeature {
        water: Some(water::Inputs::new(water, 0)),
        ssr,
        frame: frame(),
        color: AttachmentView::new(ResourceRef::new("scene color"), opaque).unwrap(),
        groups: vec![],
    };
    let mut prepare = PrepareContext::new(context.device(), encoder).unwrap();
    feature.commit(&mut prepare);
    prepare.finish();
    let views = mixed_views(ssr, water, depth, opaque);
    let group = gpu::group(
        context.device(),
        &ssr.water_trace,
        Some(&ssr.uniform),
        &views,
    );
    let strict_view = ssr.reflections[0].create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(wgpu::TextureViewDimension::D2),
        base_mip_level: 0,
        mip_level_count: Some(1),
        base_array_layer: 0,
        array_layer_count: Some(1),
        ..Default::default()
    });
    let relaxed_view = (pyramid::bank_count(&ssr.settings) == 2).then(|| {
        ssr.reflections[0].create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2),
            base_mip_level: 0,
            mip_level_count: Some(1),
            base_array_layer: 1,
            array_layer_count: Some(1),
            ..Default::default()
        })
    });
    let clear = || wgpu::Operations {
        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
        store: wgpu::StoreOp::Store,
    };
    if let Some(relaxed_view) = &relaxed_view {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("mixed water and opaque facing receivers"),
            color_attachments: &[
                Some(wgpu::RenderPassColorAttachment {
                    view: &strict_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: clear(),
                }),
                Some(wgpu::RenderPassColorAttachment {
                    view: relaxed_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: clear(),
                }),
            ],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&ssr.water_trace);
        pass.set_bind_group(0, &group, &[]);
        pass.draw(0..3, 0..1);
    } else {
        worldrunner_render_post::fullscreen_pass(
            encoder,
            Some("mixed water and opaque facing receivers"),
            wgpu::RenderPassColorAttachment {
                view: &strict_view,
                depth_slice: None,
                resolve_target: None,
                ops: clear(),
            },
            &ssr.water_trace,
            &[&group],
        );
    }
}

fn encode_reductions(context: &GpuContext, encoder: &mut wgpu::CommandEncoder, ssr: &Ssr) {
    let extents = ssr.settings.radiance_extents([65, 65]);
    let banks = pyramid::bank_count(&ssr.settings);
    for index in 0..ssr.reflections.len() - 1 {
        let source_view = ssr.reflection_view(index);
        let reduction_group = gpu::group(
            context.device(),
            &ssr.reduce,
            ssr.reduction_extents.get(index),
            &[source_view],
        );
        let target_index = if pyramid::shared(&ssr.settings) {
            0
        } else {
            index + 1
        };
        let mip = u32::try_from(if pyramid::shared(&ssr.settings) {
            index + 1
        } else {
            0
        })
        .unwrap();
        let target = &ssr.reflections[target_index];
        let outputs = (0..banks)
            .map(|bank| {
                target.create_view(&wgpu::TextureViewDescriptor {
                    dimension: Some(wgpu::TextureViewDimension::D2),
                    base_mip_level: mip,
                    mip_level_count: Some(1),
                    base_array_layer: bank,
                    array_layer_count: Some(1),
                    ..Default::default()
                })
            })
            .collect::<Vec<_>>();
        let attachments = outputs
            .iter()
            .map(|view| {
                Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })
            })
            .collect::<Vec<_>>();
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("mixed receiver bank reduction"),
            color_attachments: &attachments,
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_scissor_rect(0, 0, extents[index + 1][0], extents[index + 1][1]);
        pass.set_pipeline(&ssr.reduce);
        pass.set_bind_group(0, &reduction_group, &[]);
        pass.draw(0..3, 0..1);
    }
}

fn encode_resolve(
    context: &GpuContext,
    encoder: &mut wgpu::CommandEncoder,
    ssr: &Ssr,
    water: &crate::water::Water,
    depth: &wgpu::Texture,
) -> wgpu::Texture {
    let mut resolve_inputs = vec![
        ssr.material
            .create_view(&wgpu::TextureViewDescriptor::default()),
        ssr.normals
            .create_view(&wgpu::TextureViewDescriptor::default()),
        ssr.hierarchy[0].create_view(&wgpu::TextureViewDescriptor::default()),
        ssr.hierarchy[0].create_view(&wgpu::TextureViewDescriptor::default()),
    ];
    resolve_inputs.extend(ssr.reflection_views());
    resolve_inputs.extend(
        ssr.forward_products
            .each_ref()
            .map(|texture| texture.create_view(&wgpu::TextureViewDescriptor::default())),
    );
    water::Inputs::new(water, 0).resolve_views(
        &mut resolve_inputs,
        depth.create_view(&wgpu::TextureViewDescriptor::default()),
    );
    ssr.application_views(&mut resolve_inputs);
    let resolve_group = gpu::group(
        context.device(),
        &ssr.water_resolve,
        Some(&ssr.uniform),
        &resolve_inputs,
    );
    let output =
        crate::ssao::passes::texture(context.device(), [65, 65], wgpu::TextureFormat::Rgba16Float);
    let output_view = output.create_view(&wgpu::TextureViewDescriptor::default());
    worldrunner_render_post::fullscreen_pass(
        encoder,
        Some("mixed receiver bank resolve"),
        wgpu::RenderPassColorAttachment {
            view: &output_view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                store: wgpu::StoreOp::Store,
            },
        },
        &ssr.water_resolve,
        &[&resolve_group],
    );
    output
}

fn read_mip_bank(context: &GpuContext, ssr: &Ssr, bank: u32) -> Vec<Vec<u8>> {
    ssr.settings
        .radiance_extents([65, 65])
        .iter()
        .enumerate()
        .map(|(index, _)| read_mip_layer(context, ssr, index, bank))
        .collect()
}

fn read_mip_layer(context: &GpuContext, ssr: &Ssr, index: usize, bank: u32) -> Vec<u8> {
    let extent = ssr.settings.radiance_extents([65, 65])[index];
    let texture_index = if pyramid::shared(&ssr.settings) {
        0
    } else {
        index
    };
    let mip = u32::try_from(if pyramid::shared(&ssr.settings) {
        index
    } else {
        0
    })
    .unwrap();
    let texture = &ssr.reflections[texture_index];
    let copy = context.device().create_texture(&wgpu::TextureDescriptor {
        label: Some("SSR mip readback"),
        size: wgpu::Extent3d {
            width: extent[0],
            height: extent[1],
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: texture.format(),
        usage: wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let mut encoder = context.submission().begin(context.device()).unwrap();
    encoder.copy_texture_to_texture(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: mip,
            origin: wgpu::Origin3d {
                x: 0,
                y: 0,
                z: bank,
            },
            aspect: wgpu::TextureAspect::All,
        },
        copy.as_image_copy(),
        wgpu::Extent3d {
            width: extent[0],
            height: extent[1],
            depth_or_array_layers: 1,
        },
    );
    encoder.finish().submit().unwrap();
    read_layer(context, &copy, 0)
}

fn read_layer(context: &GpuContext, texture: &wgpu::Texture, layer: u32) -> Vec<u8> {
    let mut readback = Readback::new(
        context.device(),
        FrameConfig::new([texture.width(), texture.height()], texture.format(), 1),
        1_000_000,
    )
    .unwrap();
    let mut encoder = context.submission().begin(context.device()).unwrap();
    readback
        .encode_lanes_layer(
            &mut encoder,
            texture,
            1,
            1,
            [texture.width(), texture.height()],
            layer,
        )
        .unwrap();
    let submission = encoder.finish().submit().unwrap();
    readback
        .read_encoded(context.device(), Some(submission))
        .unwrap()
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

fn assert_resolved_owners(
    actual: &[u8],
    strict: &[u8],
    water_mask: &[bool],
    traversal: SceneSsrTraversal,
    coefficient: u32,
) {
    let mut changed_water = false;
    for (index, ((actual, strict), water)) in actual
        .chunks_exact(8)
        .zip(strict.chunks_exact(8))
        .zip(water_mask)
        .enumerate()
    {
        if *water {
            changed_water |= actual != strict;
        } else {
            assert_eq!(
                actual, strict,
                "opaque output pixel {index} changed under {traversal:?}, {coefficient}"
            );
        }
    }
    assert!(
        changed_water,
        "no water output changed under {traversal:?}, {coefficient}"
    );
}

#[test]
fn gpu_water_hit_facing_accepts_water_and_rejects_opaque_in_one_frame() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    for radiance_pyramid in [SceneSsrPyramid::Full, SceneSsrPyramid::SixLevels] {
        for (traversal, coefficient) in [
            (SceneSsrTraversal::Conservative, 1),
            (SceneSsrTraversal::Conservative, 2),
            (SceneSsrTraversal::Grid, 1),
            (SceneSsrTraversal::Grid, 2),
        ] {
            let policy = SceneSsr {
                traversal,
                radiance_pyramid,
                grid_domain: SceneSsrGridDomain::ReflectionOutput,
                resolution_scale_micro: 1_000_000 / coefficient,
                ..settings()
            };
            let mut strict_reference = None;
            let mut strict_mip_reference = None;
            let mut strict_resolved_reference = None;
            for facing in [
                SceneSsrHitFacing::Any,
                SceneSsrHitFacing::ShadingNormal,
                SceneSsrHitFacing::AnyForWaterReceivers,
            ] {
                let capture = mixed_capture(
                    &context,
                    SceneSsr {
                        hit_facing: facing,
                        ..policy
                    },
                );
                if facing == SceneSsrHitFacing::ShadingNormal {
                    strict_reference = Some(capture.strict.clone());
                    strict_mip_reference = Some(capture.strict_mips.clone());
                    strict_resolved_reference = Some(capture.resolved.clone());
                }
                if facing == SceneSsrHitFacing::AnyForWaterReceivers {
                    assert_eq!(
                        capture.strict,
                        *strict_reference
                            .as_ref()
                            .expect("strict reference captured first"),
                        "bank 0 must preserve every strict trace texel for {traversal:?}, {coefficient}"
                    );
                    assert_eq!(
                        capture.strict_mips,
                        *strict_mip_reference
                            .as_ref()
                            .expect("strict mip reference captured first"),
                        "bank 0 mips must remain byte-identical for {traversal:?}, {coefficient}"
                    );
                    let strict_resolved = strict_resolved_reference
                        .as_ref()
                        .expect("strict resolved reference captured first");
                    assert_resolved_owners(
                        &capture.resolved,
                        strict_resolved,
                        &capture.water_mask,
                        traversal,
                        coefficient,
                    );
                    assert!(
                        capture
                            .relaxed_mips
                            .as_ref()
                            .expect("water-facing mip bank exists")
                            .iter()
                            .zip(&capture.strict_mips)
                            .any(|(relaxed, strict)| relaxed != strict)
                    );
                }
                for water in [true, false] {
                    let bytes = if facing == SceneSsrHitFacing::AnyForWaterReceivers && water {
                        capture.relaxed.as_ref().expect("water-facing bank exists")
                    } else {
                        &capture.strict
                    };
                    let pixel = mixed_receiver(bytes, coefficient, water);
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
}
