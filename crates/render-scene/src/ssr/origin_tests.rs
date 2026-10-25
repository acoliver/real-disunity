use super::*;

fn clear_color(context: &GpuContext, texture: &wgpu::Texture, color: wgpu::Color) {
    let mut encoder = context.submission().begin(context.device()).unwrap();
    drop(encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("sample origin metadata"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: &texture.create_view(&wgpu::TextureViewDescriptor::default()),
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(color),
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    }));
    encoder.finish().submit().unwrap();
}
fn sample_depth(context: &GpuContext, config: FrameConfig) -> wgpu::Texture {
    let texture = context.device().create_texture(&wgpu::TextureDescriptor {
        label: Some("sample origin depth"),
        size: wgpu::Extent3d {
            width: config.size[0],
            height: config.size[1],
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: config.samples,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Depth32Float,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let mut encoder = context.submission().begin(context.device()).unwrap();
    drop(encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: None,
        color_attachments: &[],
        depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
            view: &texture.create_view(&wgpu::TextureViewDescriptor::default()),
            depth_ops: Some(wgpu::Operations {
                load: wgpu::LoadOp::Clear(0.9),
                store: wgpu::StoreOp::Store,
            }),
            stencil_ops: None,
        }),
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    }));
    encoder.finish().submit().unwrap();
    texture
}
fn probe_source(samples: u32, incident: bool) -> String {
    let source = gpu::trace_source(samples, true);
    let prefix = source
        .split("    let normal = normalize(packed.xyz);")
        .next()
        .unwrap();
    let helpers = source.split("fn reflection_near(").nth(1).unwrap();
    let value = if incident {
        "reflection_incident(origin_uv)"
    } else {
        "centre"
    };
    format!("{prefix}
        let keep = textureLoad(opaque, p, 0).r + depth_at(0u, origin_uv) + select(0.0, 1.0, matches_receiver_depth(0.0, 0.0, vec3<f32>(1.0), p));
        return vec4<f32>({value} + vec3<f32>(keep * 0.0), 1.0);
    }}\nfn reflection_near({helpers}")
}
fn probe(
    context: &GpuContext,
    samples: u32,
    scale: u32,
    orthographic: bool,
    incident: bool,
) -> Vec<u8> {
    let config = FrameConfig::new([9, 7], wgpu::TextureFormat::Rgba16Float, samples);
    let mut screen = SceneMetadata::new(context.device(), config.size, neutral_ao()).unwrap();
    let settings = SceneSsr {
        resolution_scale_micro: scale,
        ..settings()
    };
    let ssr = Ssr::new(context.device(), config, settings, &mut screen).unwrap();
    let water = water_tests::water_fixture(context, config);
    clear_color(
        context,
        &water.receivers[0][1],
        wgpu::Color {
            r: 0.25,
            g: 0.75,
            b: 0.02,
            a: 0.0,
        },
    );
    let depth = sample_depth(context, config);
    let mut frame = frame();
    if orthographic {
        frame.view_projection =
            glam::camera::rh::proj::directx::orthographic(-2.0, 2.0, -2.0, 2.0, 0.1, 200.0);
    }
    let color = AttachmentView::new(ResourceRef::new("scene color"), &ssr.material).unwrap();
    let mut feature = ReflectionFeature {
        ssr: &ssr,
        water: None,
        frame,
        color,
        groups: vec![],
    };
    let mut encoder = context.submission().begin(context.device()).unwrap();
    let mut prepare = PrepareContext::new(context.device(), &mut encoder).unwrap();
    feature.commit(&mut prepare);
    prepare.finish();
    let views = probe_views(&ssr, &water, &depth);
    let pipeline = gpu::pipeline(context.device(), &probe_source(samples, incident), 1, false);
    let group = gpu::group(context.device(), &pipeline, Some(&ssr.uniform), &views);
    worldrunner_render_post::fullscreen_pass(
        &mut encoder,
        None,
        wgpu::RenderPassColorAttachment {
            view: &ssr.reflections[0].create_view(&wgpu::TextureViewDescriptor::default()),
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                store: wgpu::StoreOp::Store,
            },
        },
        &pipeline,
        &[&group],
    );
    encoder.finish().submit().unwrap();
    read(context, &ssr.reflections[0])
}
#[test]
fn gpu_water_origin_preserves_sample_coordinates_at_both_trace_scales_and_projections() {
    let context = context();
    for samples in [1, 4] {
        for scale in [500_000, 1_000_000] {
            for orthographic in [false, true] {
                let bytes = probe(&context, samples, scale, orthographic, false);
                let width = if scale == 500_000 { 5 } else { 9 };
                let height = if scale == 500_000 { 4 } else { 7 };
                let projection = if orthographic {
                    glam::camera::rh::proj::directx::orthographic(-2.0, 2.0, -2.0, 2.0, 0.1, 200.0)
                } else {
                    frame().view_projection
                };
                for lane in 0..samples {
                    for y in 0..height {
                        for x in 0..width {
                            let px = ((gpu::scalar(x) + 0.5) / gpu::scalar(width) * 9.0).floor();
                            let py = ((gpu::scalar(y) + 0.5) / gpu::scalar(height) * 7.0).floor();
                            let uv = glam::Vec2::new((px + 0.25) / 9.0, (py + 0.75) / 7.0);
                            let h = projection.inverse()
                                * glam::Vec4::new(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, 0.9, 1.0);
                            let expected = h.truncate() / h.w;
                            let index =
                                usize::try_from((y * width * samples + lane * width + x) * 8)
                                    .unwrap();
                            for (channel, value) in expected.to_array().into_iter().enumerate() {
                                let actual = u16::from_le_bytes(
                                    bytes[index + channel * 2..index + channel * 2 + 2]
                                        .try_into()
                                        .unwrap(),
                                );
                                assert!(
                                    actual.abs_diff(half(value)) <= 1,
                                    "samples {samples}, scale {scale}, ortho {orthographic}: channel {channel} {actual} != {}",
                                    half(value)
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}

fn probe_views(
    ssr: &Ssr,
    water: &crate::water::Water,
    depth: &wgpu::Texture,
) -> Vec<wgpu::TextureView> {
    let textures: Vec<_> = [&ssr.material, &ssr.normals, &ssr.material]
        .into_iter()
        .chain(ssr.hierarchy.iter())
        .chain([&ssr.source_depth, &ssr.source_depth])
        .chain([&water.receivers[0][0], &water.receivers[0][1], depth])
        .collect();
    textures
        .iter()
        .map(|t| t.create_view(&wgpu::TextureViewDescriptor::default()))
        .collect()
}

#[test]
fn gpu_orthographic_water_uses_parallel_incident_rays() {
    let context = context();
    for samples in [1, 4] {
        for scale in [500_000, 1_000_000] {
            let bytes = probe(&context, samples, scale, true, true);
            let expected: Vec<_> = [0.0, 0.0, -1.0, 1.0]
                .into_iter()
                .flat_map(|v| half(v).to_le_bytes())
                .collect();
            for pixel in bytes.chunks_exact(8) {
                assert_eq!(pixel, expected);
            }
        }
    }
}
