use super::*;

const PROBE: &str = r"
@vertex fn vs_probe(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let vertices = array<vec2<f32>, 3>(vec2<f32>(-1.0, -1.0), vec2<f32>(3.0, -1.0), vec2<f32>(-1.0, 3.0));
    return vec4<f32>(vertices[index], 0.0, 1.0);
}
@fragment fn fs_probe(@builtin(position) pixel: vec4<f32>) -> @location(0) vec4<f32> {
    let distances = array<f32, 16>(1.4, 1.5, 1.75, 1.9999, 2.0, 2.0001, 3.5, 3.75, 4.0, 5.25, 5.75, 6.0, 6.0001, 8.0, 8.1, 0.0);
    let visibility = sunlight_visibility(vec3<f32>(distances[u32(pixel.x)], 0.0, 0.0), vec3<f32>(0.0, 0.0, 1.0));
    return vec4<f32>(vec3<f32>(visibility), 1.0);
}
";

#[test]
fn gpu_receiver_selects_split_boundaries_and_blends_continuously() {
    let context = context();
    let source = super::cascades::source_with(
        &scene(&[], 600_000, None),
        SceneShadowCascades::camera_splits(vec![2_000, 4_000, 6_000, 8_000], 250_000).unwrap(),
    );
    let renderer = ShadowedScene::new(&context, &source, settings()).unwrap();
    for camera_splits in [true, false] {
        upload_probe(&context, &renderer, camera_splits, 4, 0.25);
        let image = render_probe(
            &context,
            &renderer,
            &format!("{}{PROBE}", resources::SHADOWED_WGSL),
        );
        let expected = if camera_splits {
            [
                0_u8, 0, 188, 255, 255, 255, 255, 188, 0, 0, 188, 255, 255, 255, 255, 0,
            ]
        } else {
            [
                0_u8, 0, 188, 255, 255, 255, 188, 137, 0, 188, 235, 255, 255, 255, 255, 0,
            ]
        };
        for (pixel, expected) in image.pixels.chunks_exact(4).zip(expected) {
            assert!(
                pixel[0].abs_diff(expected) <= 1,
                "{pixel:?}, expected {expected}, camera={camera_splits}"
            );
        }
    }
}

fn upload_probe(
    context: &GpuContext,
    renderer: &ShadowedScene,
    camera_splits: bool,
    count: usize,
    blend: f32,
) {
    let matrices = [2.0, 4.0, 6.0, 8.0].map(|extent| {
        let x_axis = if camera_splits {
            Vec4::ZERO
        } else {
            Vec4::X / extent
        };
        glam::Mat4::from_cols(
            x_axis,
            Vec4::ZERO,
            Vec4::ZERO,
            Vec4::new(0.0, 0.0, 0.5, 1.0),
        )
    })[..count]
        .to_vec();
    let views = super::super::cascades::CascadeViews {
        matrices,
        depth_bias: [0.0; 4],
        splits: [2.0, 4.0, 6.0, 8.0],
        blend,
        camera_splits,
        near: 0.0,
        view_depth: Vec4::X,
    };
    context.queue().write_buffer(
        &renderer.map.uniform,
        0,
        &resources::uniform_bytes(
            &views,
            DirectionalShadowSettings {
                depth_bias: 0.0,
                normal_bias_meters: 0.0,
                ..settings()
            },
        ),
    );
}

fn clear_layers(renderer: &ShadowedScene, encoder: &mut wgpu::CommandEncoder) {
    for index in 0..4 {
        let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: renderer.map.depth.layer_view(index).unwrap(),
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(if index % 2 == 0 { 0.0 } else { 1.0 }),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            ..Default::default()
        });
    }
}

fn probe_pipeline(
    context: &GpuContext,
    shadow_layout: &wgpu::BindGroupLayout,
    source: &str,
) -> wgpu::RenderPipeline {
    let empty = context
        .device()
        .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("empty"),
            entries: &[],
        });
    let layout = context
        .device()
        .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("cascade probe"),
            bind_group_layouts: &[Some(&empty), Some(&empty), Some(shadow_layout)],
            immediate_size: 0,
        });
    let module = context
        .device()
        .create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("cascade probe"),
            source: wgpu::ShaderSource::Wgsl(source.into()),
        });
    context
        .device()
        .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("cascade probe"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs_probe"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs_probe"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: OffscreenTarget::COLOR_FORMAT,
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

fn render_probe(context: &GpuContext, renderer: &ShadowedScene, source: &str) -> RgbaImage {
    let pipeline = probe_pipeline(context, &renderer.shadow_layout, source);
    let mut target = OffscreenTarget::new(
        context.device(),
        crate::Viewport {
            width: 16,
            height: 1,
        },
    )
    .unwrap();
    let mut encoder = context.submission().begin(context.device()).unwrap();
    clear_layers(renderer, &mut encoder);
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
        pass.set_bind_group(2, &renderer.map.group, &[]);
        pass.draw(0..3, 0..1);
    }
    target.encode_readback(&mut encoder).unwrap();
    let submission = encoder.finish().submit().unwrap();
    target.read_encoded(context, submission).unwrap()
}

#[test]
fn gpu_multi_cascade_sample_counts_match_main_exactly() {
    let context = context();
    let source = super::cascades::source_with(
        &scene(&[], 600_000, None),
        SceneShadowCascades::new(vec![2_000, 4_000, 6_000, 8_000], 250_000).unwrap(),
    );
    let renderer = ShadowedScene::new(&context, &source, settings()).unwrap();
    // Count entry to the production PCF helper. All selected projections in this
    // fixture have valid UV/depth, so each entry evaluates one nine-tap grid.
    let instrumented = resources::SHADOWED_WGSL.replace(
        "fn cascade_visibility(ndc: vec3<f32>, gradient: vec2<f32>, layer: i32) -> f32 {",
        "var<private> samples: u32;\nfn cascade_visibility(ndc: vec3<f32>, gradient: vec2<f32>, layer: i32) -> f32 { samples += 1u;",
    );
    let probe = PROBE.replace(
        "return vec4<f32>(vec3<f32>(visibility), 1.0);",
        "return vec4<f32>(f32(samples & 1u), f32((samples >> 1u) & 1u), f32((samples >> 2u) & 1u), 1.0);",
    );
    let instrumented = format!("{instrumented}{probe}");
    for count in 2..=4 {
        for camera in [false, true] {
            for blend in [0.0, 0.25] {
                upload_probe(&context, &renderer, camera, count, blend);
                let image = render_probe(&context, &renderer, &instrumented);
                let expected: Vec<u8> = [
                    1.4, 1.5, 1.75, 1.9999, 2.0, 2.0001, 3.5, 3.75, 4.0, 5.25, 5.75, 6.0, 6.0001,
                    8.0, 8.1, 0.0,
                ]
                .into_iter()
                .flat_map(|distance| {
                    let selected = [2.0, 4.0, 6.0, 8.0][..count]
                        .iter()
                        .position(|end| distance <= *end);
                    let samples = selected.map_or(0_u8, |index| {
                        if index + 1 == count || blend == 0.0 {
                            1
                        } else {
                            2
                        }
                    });
                    [255 * (samples & 1), 255 * ((samples >> 1) & 1), 0, 255]
                })
                .collect();
                assert_eq!(
                    image.pixels, expected,
                    "count={count}, camera={camera}, blend={blend}"
                );
            }
        }
    }
}
