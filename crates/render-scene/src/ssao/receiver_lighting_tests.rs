fn probe_layout(device: &wgpu::Device) -> wgpu::PipelineLayout {
    let frame = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: None,
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }],
    });
    let screen = receiver::layout(device);
    device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[Some(&frame), None, None, Some(&screen)],
        immediate_size: 0,
    })
}

use super::{receiver, tests::*};
use wgpu::util::DeviceExt;
use worldrunner_render_plan::{SceneSpecularOcclusion, SceneSsaoReceiver};

fn probe_pipeline(device: &wgpu::Device, source: &str, rough: bool) -> wgpu::RenderPipeline {
    let vertex = include_str!("fullscreen.wgsl")
        .split("@vertex")
        .nth(1)
        .unwrap()
        .split("fn reconstruct")
        .next()
        .unwrap();
    let source = format!(
        "{source}\n@vertex{vertex}\n@fragment fn receiver_probe() -> @location(0) vec4<f32> {{
        let light = scene_surface_lighting(vec3(0.0,0.0,-3.0), vec3(0.0,0.0,1.0),
            vec3(0.5), vec3(0.5,0.0,0.8), vec3(0.04));
        return vec4(light.ambient_diffuse.x, scene_ao(0.8,vec3(0.0,0.0,-3.0),0.0,0.0),
            scene_specular_ao(0.5,scene_ao(0.8,vec3(0.0,0.0,-3.0),0.0,0.0),0.5),1.0);
    }}"
    );
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("AO receiver lighting probe"),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    let constants = [("SCENE_SPECULAR_OCCLUSION", f64::from(rough))];
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("AO receiver lighting probe"),
        layout: Some(&probe_layout(device)),
        vertex: wgpu::VertexState {
            module: &module,
            entry_point: Some("vs_fullscreen"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module: &module,
            entry_point: Some("receiver_probe"),
            compilation_options: wgpu::PipelineCompilationOptions {
                constants: &constants,
                ..Default::default()
            },
            targets: &[Some(wgpu::ColorTargetState {
                format: wgpu::TextureFormat::Rgba32Float,
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

fn probe(context: &crate::device::GpuContext, source: &str, rough: bool) -> Vec<f32> {
    let pipeline = probe_pipeline(context.device(), source, rough);
    let mut frame = super::super::shader::gpu_math_tests::configured_frame(
        worldrunner_render_lighting::ExposureOwnership::SceneLinear(1.0),
    );
    frame.sun_irradiance = glam::Vec3::ZERO;
    frame.ambient_sky = glam::Vec3::ONE;
    frame.ambient_ground = glam::Vec3::ONE;
    let frame = context
        .device()
        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: &frame.to_bytes(),
            usage: wgpu::BufferUsages::UNIFORM,
        });
    let frame_group = context
        .device()
        .create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: frame.as_entire_binding(),
            }],
        });
    let ao = super::passes::texture(context.device(), [2, 2], wgpu::TextureFormat::Rg32Float);
    let values = [0.125_f32, 2.0, 0.25, 2.0, 0.75, 4.0, 0.875, 4.0];
    context.queue().write_texture(
        ao.as_image_copy(),
        &values
            .into_iter()
            .flat_map(f32::to_le_bytes)
            .collect::<Vec<_>>(),
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(16),
            rows_per_image: Some(2),
        },
        ao.size(),
    );
    let uniform = context
        .device()
        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: &glam::Mat4::IDENTITY
                .to_cols_array()
                .into_iter()
                .flat_map(f32::to_le_bytes)
                .collect::<Vec<_>>(),
            usage: wgpu::BufferUsages::UNIFORM,
        });
    let group = receiver::group(
        context.device(),
        &ao,
        &uniform,
        SceneSsaoReceiver::DepthAwareGather,
    );
    let output = super::passes::texture(context.device(), [1, 1], wgpu::TextureFormat::Rgba32Float);
    draw_probe(context, &pipeline, (&frame_group, &group), &output);
    read(context, &output)
}

fn draw_probe(
    context: &crate::device::GpuContext,
    pipeline: &wgpu::RenderPipeline,
    groups: (&wgpu::BindGroup, &wgpu::BindGroup),
    output: &wgpu::Texture,
) {
    let mut encoder = context.submission().begin(context.device()).unwrap();
    {
        let view = output.create_view(&wgpu::TextureViewDescriptor::default());
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, groups.0, &[]);
        pass.set_bind_group(3, groups.1, &[]);
        pass.draw(0..3, 0..1);
    }
    encoder.finish().submit().unwrap();
}

#[test]
fn gpu_mesh_terrain_receiver_agree_for_ambient_and_specular_occlusion() {
    let context = context();
    for rough in [false, true] {
        let mut results = Vec::new();
        for stock in [crate::shader::ENVIRONMENT_WGSL, crate::shader::TERRAIN_WGSL] {
            let source = receiver::source(
                &super::ambient_source(stock),
                SceneSsaoReceiver::DepthAwareGather,
            );
            results.push(probe(&context, &source, rough));
        }
        for (mesh, terrain) in results[0].iter().zip(&results[1]) {
            assert!((mesh - terrain).abs() < 2e-6);
        }
        let combined = 0.505_f32;
        assert!((results[0][0] - 0.5 * combined).abs() < 2e-6);
        assert!((results[0][1] - combined).abs() < 2e-6);
        let expected = if rough {
            worldrunner_render_lighting::specular_occlusion(0.5, combined, 0.5)
        } else {
            1.0
        };
        assert!((results[0][2] - expected).abs() < 2e-6);
    }
}

#[test]
fn gpu_receiver_variant_preserves_nearest_keys_and_reuses_gather() {
    let context = context();
    let pipelines = crate::renderer::ScenePipelines::with_ssao(
        context.device(),
        wgpu::TextureFormat::Rgba16Float,
        1,
    )
    .unwrap();
    let key = crate::pipeline::PipelineKey::ALL[0];
    let nearest = pipelines.for_receiver(SceneSsaoReceiver::Nearest);
    assert_eq!(nearest.get(key), pipelines.get(key));
    assert_eq!(nearest.terrain(), pipelines.terrain());
    let gather = pipelines.for_receiver(SceneSsaoReceiver::DepthAwareGather);
    let again = pipelines.for_receiver(SceneSsaoReceiver::DepthAwareGather);
    assert_eq!(gather.get(key), again.get(key));
    assert_eq!(gather.terrain(), again.terrain());
    assert_ne!(gather.get(key), nearest.get(key));
    let convention = worldrunner_render_plan::SceneSpecularConvention {
        occlusion: SceneSpecularOcclusion::RoughnessAware,
        ..Default::default()
    };
    assert_ne!(
        gather.for_convention(convention).terrain(),
        gather.terrain()
    );
}
