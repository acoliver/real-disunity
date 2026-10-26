use super::*;

#[test]
fn prepared_environment_matches_replaceable_lobe_and_sample_receivers() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    let maps = prepare_environment(&context);
    for samples in [1, 4] {
        let mut renderer = regressions::renderer(&context, samples);
        let body = reflective_body();
        renderer.install_water(context.device(), body).unwrap();
        renderer.set_water_environment(&maps).unwrap();
        let color = AttachmentView::new(
            ResourceRef::new("scene color"),
            renderer.targets().scene().texture(),
        )
        .unwrap();
        let depth = AttachmentView::new(
            ResourceRef::new("scene depth"),
            renderer.targets().depth().texture(),
        )
        .unwrap();
        let frame = FrameUniform::new(&fixture_view(), None).unwrap();
        encode_fixture(&context, &mut renderer, (&color, &depth), &frame, 0.5)
            .finish()
            .submit()
            .unwrap();
        let (normal, material, specular) = renderer.water_receivers().unwrap();
        let mut normals = Vec::new();
        for sample in 0..samples {
            let n = sample_read(&context, normal.texture(), sample);
            let m = sample_read(&context, material.texture(), sample);
            let s = sample_read(&context, specular.texture(), sample);
            let c = sample_read(&context, renderer.targets().scene().texture(), sample);
            assert!(
                n.chunks_exact(8).any(|p| p == [0; 8]),
                "uncovered samples clear the water flag"
            );
            let offset = (16 * 32 + 16) * 8;
            assert_eq!(
                &n[offset + 6..offset + 8],
                &0x3c00u16.to_le_bytes(),
                "water flag"
            );
            assert_eq!(
                &m[offset + 6..offset + 8],
                &0x3800u16.to_le_bytes(),
                "roughness"
            );
            assert_eq!(&s[offset..offset + 8], &c[offset..offset + 8]);
            assert!(u16::from_le_bytes(s[offset..offset + 2].try_into().unwrap()) > 0);
            normals.push(n[offset..offset + 6].to_vec());
            let f0 = numerical::positive_half(&m[offset + 4..offset + 6]);
            assert!((f0 - 0.02037).abs() < 0.00002);
        }
        if samples == 4 {
            assert!(
                normals.windows(2).any(|pair| pair[0] != pair[1]),
                "sample-position normals must not be pixel-centre copies"
            );
        }
    }
}

fn reflective_body() -> SceneWaterBody {
    let mut body = settings();
    body.east_min_millimeters = -1000;
    body.north_min_millimeters = -1000;
    body.east_extent_millimeters = 2000;
    body.north_extent_millimeters = 2000;
    body.color_micro = [0; 3];
    body.absorption_micro = [10_000_000; 3];
    body.roughness_micro = 500_000;
    body
}

pub(crate) fn prepare_environment(
    context: &GpuContext,
) -> crate::sky::preparation::PreparedEnvironmentMaps {
    let source = context.device().create_texture(&wgpu::TextureDescriptor {
        label: Some("constant linear environment"),
        size: wgpu::Extent3d {
            width: 4,
            height: 4,
            depth_or_array_layers: 6,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba16Float,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let bytes: Vec<_> = (0..4 * 4 * 6)
        .flat_map(|_| [0x3800u16, 0x3400, 0x3000, 0x3c00])
        .flat_map(u16::to_le_bytes)
        .collect();
    context.queue().write_texture(
        source.as_image_copy(),
        &bytes,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(32),
            rows_per_image: Some(4),
        },
        source.size(),
    );
    let view = source.create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(wgpu::TextureViewDimension::Cube),
        ..Default::default()
    });
    let mut encoder = context.submission().begin(context.device()).unwrap();
    let maps = crate::sky::preparation::encode(
        context.device(),
        &mut encoder,
        &view,
        crate::sky::preparation::EnvironmentPreparation {
            cube_size: 4,
            mip_count: 3,
            lut_size: 8,
            samples: 64,
        },
        worldrunner_render_gpu::configuration::AllocationBudget(100_000),
    )
    .unwrap();
    encoder.finish().submit().unwrap();
    maps
}

pub(crate) fn sample_read(context: &GpuContext, texture: &wgpu::Texture, sample: u32) -> Vec<u8> {
    let samples = texture.sample_count();
    if samples == 1 {
        return read(context, texture);
    }
    let pipeline = sample_pipeline(context.device(), sample);
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let group = context
        .device()
        .create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            }],
        });
    let output =
        crate::ssao::passes::texture(context.device(), [32, 32], wgpu::TextureFormat::Rgba16Float);
    let output_view = output.create_view(&wgpu::TextureViewDescriptor::default());
    let mut encoder = context.submission().begin(context.device()).unwrap();
    worldrunner_render_post::fullscreen_pass(
        &mut encoder,
        None,
        wgpu::RenderPassColorAttachment {
            view: &output_view,
            resolve_target: None,
            depth_slice: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                store: wgpu::StoreOp::Store,
            },
        },
        &pipeline,
        &[&group],
    );
    encoder.finish().submit().unwrap();
    read(context, &output)
}

fn sample_pipeline(device: &wgpu::Device, sample: u32) -> wgpu::RenderPipeline {
    let source = format!(
        "@group(0) @binding(0) var source: texture_multisampled_2d<f32>;\n@vertex fn vs(@builtin(vertex_index) i:u32)->@builtin(position) vec4<f32> {{ let p=array<vec2<f32>,3>(vec2(-1.0,-1.0),vec2(3.0,-1.0),vec2(-1.0,3.0)); return vec4(p[i],0.0,1.0); }}\n@fragment fn fs(@builtin(position) p:vec4<f32>)->@location(0) vec4<f32> {{ return textureLoad(source,vec2<i32>(p.xy),{sample}); }}"
    );
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("sample receiver probe"),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("sample receiver probe"),
        layout: None,
        vertex: wgpu::VertexState {
            module: &module,
            entry_point: Some("vs"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module: &module,
            entry_point: Some("fs"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: wgpu::TextureFormat::Rgba16Float,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}
