use super::*;

#[test]
fn gpu_water_response_leaves_opaque_receivers_and_refraction_snapshot_identical() {
    let context = context();
    let baseline = water_capture(&context, None);
    let response = water_capture(
        &context,
        Some(SceneWaterShadowResponse {
            offset: SceneWaterShadowOffset::GeometricNormal,
            diffuse_floor_micro: 1_000_000,
            ..Default::default()
        }),
    );
    assert_eq!(baseline[0], response[0], "opaque receiver shading changed");
    assert_eq!(baseline[1], response[1], "opaque snapshot depth changed");
    assert_ne!(
        baseline[2], response[2],
        "water must respond to the shadow floor"
    );
}

fn water_scene(response: Option<SceneWaterShadowResponse>) -> RenderScene {
    let source = scene(&[[0, 0, 1_000]], 600_000, None);
    let mut water = unsupported_water_body();
    water.height_millimeters = 100;
    water.east_min_millimeters = -3000;
    water.north_min_millimeters = -3000;
    water.east_extent_millimeters = 6000;
    water.north_extent_millimeters = 6000;
    water.material.direct_surface = true;
    water.material.lighting_normal_micro = Some(0);
    water.material.shadow_response = response;
    let mut effects = fixture::effects_parts(600_000);
    effects.water = Some(water);
    RenderScene::new(SceneParts {
        resources: source.resources().clone(),
        geometry: source.geometry().clone(),
        terrain: None,
        effects: Some(SceneEffects::new(effects).unwrap()),
    })
    .unwrap()
}

fn water_capture(context: &GpuContext, response: Option<SceneWaterShadowResponse>) -> Vec<Vec<u8>> {
    use worldrunner_render_post::{FrameConfig, Readback, ToneMap};
    let source = water_scene(response);
    let view = water_view(&source);
    let mut frame = crate::compositor::FrameRenderer::hdr(
        context,
        [32, 32],
        OffscreenTarget::COLOR_FORMAT,
        1,
        ToneMap::Reinhard,
        source.effects(),
    )
    .unwrap();
    let mut renderer = ShadowedScene::with_target(
        context,
        &source,
        settings(),
        wgpu::TextureFormat::Rgba16Float,
        1,
    )
    .unwrap();
    let color = AttachmentView::new(
        ResourceRef::new("scene color"),
        frame.targets().scene().texture(),
    )
    .unwrap();
    let depth = AttachmentView::new(
        ResourceRef::new("scene depth"),
        frame.targets().depth().texture(),
    )
    .unwrap();
    let mut encoder = context.submission().begin(context.device()).unwrap();
    renderer
        .encode_with(
            context,
            &view,
            (&color, &depth),
            &mut encoder,
            Some(&mut frame),
        )
        .unwrap();
    encoder.finish().submit().unwrap();
    let snapshot = &frame.water.as_ref().unwrap().snapshots[0];
    [&snapshot.color, &snapshot.depth, frame.targets().texture()]
        .into_iter()
        .map(|texture| {
            Readback::new(
                context.device(),
                FrameConfig::new([32, 32], texture.format(), 1),
                100_000,
            )
            .unwrap()
            .read(context.device(), context.submission(), texture)
            .unwrap()
        })
        .collect()
}

fn water_view(source: &RenderScene) -> SceneView {
    SceneView::new(
        crate::CameraPose {
            eye_millimeters: [0, -4000, 6500],
            target_millimeters: [0; 3],
        },
        *source.effects().unwrap().camera(),
        crate::Viewport {
            width: 32,
            height: 32,
        },
    )
    .unwrap()
}

#[test]
fn gpu_water_geometric_lookup_removes_normal_map_structure_along_shadow_edge() {
    let context = context();
    let source = super::cascades::source_with(
        &scene(&[], 600_000, None),
        SceneShadowCascades::new(vec![2_000, 4_000, 6_000, 8_000], 0).unwrap(),
    );
    let renderer = ShadowedScene::new(&context, &source, settings()).unwrap();
    super::selection::upload_probe(&context, &renderer, false, 1, 0.0);
    let matrix = glam::Mat4::from_cols(
        Vec4::new(1.0, 0.0, 0.2, 0.0),
        Vec4::new(0.0, 1.0, 0.1, 0.0),
        Vec4::new(0.5, 0.0, 0.4, 0.0),
        Vec4::new(0.0, 0.0, 0.5, 1.0),
    );
    context.queue().write_buffer(
        &renderer.map.uniform,
        0,
        &matrix
            .to_cols_array()
            .into_iter()
            .flat_map(f32::to_le_bytes)
            .collect::<Vec<_>>(),
    );
    context
        .queue()
        .write_buffer(&renderer.map.uniform, 260, &0.2f32.to_le_bytes());
    let (layout, group) = normal_map(&context);
    let shading = probe(&context, &renderer, (&layout, &group), 1);
    let geometric = probe(&context, &renderer, (&layout, &group), 2);
    let none = probe(&context, &renderer, (&layout, &group), 3);
    for (offset, image) in [(1, &shading), (2, &geometric), (3, &none)] {
        assert_lookup_samples(image, offset);
    }
    assert!(
        shading.pixels.chunks_exact(64).any(|row| {
            row.chunks_exact(4).map(|v| v[0]).min() != row.chunks_exact(4).map(|v| v[0]).max()
        }),
        "shading lookup must follow the ripple along the edge"
    );
    for image in [&geometric, &none] {
        for row in image.pixels.chunks_exact(64) {
            assert!(row.chunks_exact(4).all(|pixel| pixel[0] == row[0]));
        }
        assert_ne!(image.pixels[0], image.pixels[15 * 64]);
    }
    assert_ne!(geometric.pixels, none.pixels);
    // A 0.2 m geometric bias projects to +0.1 in X, exactly two receiver rows.
    assert_eq!(&geometric.pixels[..14 * 64], &none.pixels[2 * 64..]);
}

fn assert_lookup_samples(image: &RgbaImage, offset: u32) {
    for (index, pixel) in image.pixels.chunks_exact(4).enumerate() {
        let x = index % 16;
        let y = index / 16;
        let nx = f64::from([32, 64, 192, 224][x % 4]) / 255.0 * 2.0 - 1.0;
        let ny = 128.0 / 255.0 * 2.0 - 1.0;
        let normal = match offset {
            1 => [nx, ny, (1.0 - nx * nx - ny * ny).sqrt()],
            2 => [0.0, 0.0, 1.0],
            _ => [0.0; 3],
        };
        let position = [
            (f64::from(u32::try_from(y).unwrap()) - 7.5) * 0.05,
            (f64::from(u32::try_from(x).unwrap()) + 0.5) * 0.02,
        ];
        let biased = [
            position[0] + normal[0] * 0.2,
            position[1] + normal[1] * 0.2,
            normal[2] * 0.2,
        ];
        let clip = [
            biased[0] + 0.5 * biased[2],
            biased[1],
            0.5 + 0.2 * biased[0] + 0.1 * biased[1] + 0.4 * biased[2],
        ];
        assert!(clip[2] > 0.1 && clip[2] < 0.9);
        // Every sample is outside the three-tap filter footprint around X=0.
        assert!(clip[0].abs() > 3.0 / f64::from(settings().resolution));
        let expected = if clip[0] > 0.0 { 255 } else { 0 };
        assert_eq!(
            pixel[0], expected,
            "offset={offset}, pixel=({x},{y}), clip={clip:?}"
        );
    }
}

fn normal_map(context: &GpuContext) -> (wgpu::BindGroupLayout, wgpu::BindGroup) {
    let device = context.device();
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("rippling water normal map"),
        size: wgpu::Extent3d {
            width: 16,
            height: 1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rg8Unorm,
        usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let texels: Vec<u8> = [32, 64, 192, 224]
        .into_iter()
        .cycle()
        .take(16)
        .flat_map(|x| [x, 128])
        .collect();
    context.queue().write_texture(
        texture.as_image_copy(),
        &texels,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(32),
            rows_per_image: Some(1),
        },
        texture.size(),
    );
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("normal probe"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: false },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        }],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("normal probe"),
        layout: &layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::TextureView(&view),
        }],
    });
    (layout, group)
}

fn probe_source(offset: u32) -> String {
    let lookup = include_str!("../../water/shadow_lookup.wgsl")
        .split("fn water_shadow_depth")
        .next()
        .unwrap();
    let vertex = super::selection::PROBE.split("@fragment").next().unwrap();
    format!(
        "{}\n{}\n{lookup}\n{vertex}\n{}",
        worldrunner_render_shadow::shadow_wgsl!(),
        include_str!("../../shader/shadows.wgsl"),
        format_args!(
            r"
struct WaterProbe {{ disk: vec4<f32> }}
const water = WaterProbe(vec4<f32>(0.0, 0.0, 0.0, {offset}.0));
@group(0) @binding(0) var normal_map: texture_2d<f32>;
@fragment fn fs_probe(@builtin(position) pixel: vec4<f32>) -> @location(0) vec4<f32> {{
 let xy = textureLoad(normal_map, vec2<i32>(i32(pixel.x), 0), 0).rg * 2.0 - 1.0;
 let normal = vec3<f32>(xy, sqrt(1.0 - dot(xy, xy)));
 let visibility = sunlight_visibility(vec3<f32>((pixel.y - 8.0) * 0.05, pixel.x * 0.02, 0.0), water_shadow_normal(normal));
 return vec4<f32>(vec3<f32>(visibility), 1.0);
}}"
        )
    )
}

fn probe(
    context: &GpuContext,
    renderer: &ShadowedScene,
    normal: (&wgpu::BindGroupLayout, &wgpu::BindGroup),
    offset: u32,
) -> RgbaImage {
    let pipeline = super::selection::probe_pipeline(
        context,
        &renderer.shadow_layout,
        &probe_source(offset),
        Some(normal.0),
    );
    let mut target = OffscreenTarget::new(
        context.device(),
        crate::Viewport {
            width: 16,
            height: 16,
        },
    )
    .unwrap();
    let mut encoder = context.submission().begin(context.device()).unwrap();
    clear_shadow(renderer, &mut encoder, &edge_pipeline(context.device()));
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target.color_view(),
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, normal.1, &[]);
        pass.set_bind_group(2, &renderer.map.group, &[]);
        pass.draw(0..3, 0..1);
    }
    target.encode_readback(&mut encoder).unwrap();
    let submission = encoder.finish().submit().unwrap();
    target.read_encoded(context, submission).unwrap()
}

fn clear_shadow(
    renderer: &ShadowedScene,
    encoder: &mut wgpu::CommandEncoder,
    pipeline: &wgpu::RenderPipeline,
) {
    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
            view: renderer.map.depth.layer_view(0).unwrap(),
            depth_ops: Some(wgpu::Operations {
                load: wgpu::LoadOp::Clear(1.0),
                store: wgpu::StoreOp::Store,
            }),
            stencil_ops: None,
        }),
        ..Default::default()
    });
    pass.set_pipeline(pipeline);
    pass.set_scissor_rect(0, 0, settings().resolution / 2, settings().resolution);
    pass.draw(0..3, 0..1);
}

fn edge_pipeline(device: &wgpu::Device) -> wgpu::RenderPipeline {
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("synthetic depth shadow edge"),
        source: wgpu::ShaderSource::Wgsl(
            super::selection::PROBE
                .split("@fragment")
                .next()
                .unwrap()
                .into(),
        ),
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("synthetic depth shadow edge"),
        layout: None,
        vertex: wgpu::VertexState {
            module: &module,
            entry_point: Some("vs_probe"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[],
        },
        fragment: None,
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::Always),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}
