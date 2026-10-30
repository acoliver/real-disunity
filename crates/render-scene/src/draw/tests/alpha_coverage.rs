#[test]
fn multisampled_masked_pipelines_support_velocity_and_reflection_targets() {
    let context = context();
    let device = context.device();
    let velocity =
        ScenePipelines::with_taa_velocity(device, TextureFormat::Rgba16Float, 4).unwrap();
    let key = crate::pipeline::PipelineKey::from_material(
        &material(
            SceneAlphaMode::Mask {
                cutoff_micro: 500_000,
            },
            true,
        )
        .with_alpha_to_coverage(true)
        .unwrap(),
    );
    let _ = velocity.get(key);
    let layout = crate::ssr::forward::layout(device);
    for writes_velocity in [false, true] {
        let (object, terrain) = crate::ssr::forward::velocity_sources(
            (crate::shader::ENVIRONMENT_WGSL, crate::shader::TERRAIN_WGSL),
            writes_velocity,
            false,
        );
        let pipelines = ScenePipelines::with_screen_space(
            device,
            (TextureFormat::Rgba16Float, 4),
            &object,
            &terrain,
            None,
            Some(&layout),
        );
        let _ = pipelines.get(key);
        let relief = crate::pipeline::PipelineKey {
            relief: true,
            ..key
        };
        let _ = pipelines.get(relief);
    }
}

use super::*;
use worldrunner_render_plan::TexelFormat;

fn gradient_scene(enabled: Option<bool>) -> RenderScene {
    let base = material(
        SceneAlphaMode::Mask {
            cutoff_micro: 507_812,
        },
        true,
    );
    let material = enabled.map_or(base, |value| base.with_alpha_to_coverage(value).unwrap());
    let texture = SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Linear,
        width: 2,
        height: 1,
        mips: 1,
        bytes: vec![255, 255, 255, 0, 255, 255, 255, 255].into(),
    };
    let mesh = SceneMesh::new(vec![
        SceneMeshLod::new(
            0,
            vec![
                vertex([-1000, -1000, 0], [0, 0]),
                vertex([1000, -1000, 0], [500_000, 0]),
                vertex([1000, 1000, 0], [MICRO, 0]),
                vertex([-1000, 1000, 0], [500_000, 0]),
            ],
            vec![0, 1, 2, 0, 2, 3],
            None,
        )
        .unwrap(),
    ])
    .unwrap();
    build(texture, &material, mesh, 1)
}

fn attachment(device: &Device, format: TextureFormat, samples: u32) -> Texture {
    device.create_texture(&TextureDescriptor {
        label: Some("alpha coverage regression"),
        size: Extent3d {
            width: TARGET,
            height: TARGET,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: samples,
        dimension: TextureDimension::D2,
        format,
        usage: TextureUsages::RENDER_ATTACHMENT
            | if samples == 1 {
                TextureUsages::COPY_SRC
            } else {
                TextureUsages::empty()
            },
        view_formats: &[],
    })
}

fn coverage(context: &GpuContext, enabled: Option<bool>, samples: u32) -> Vec<u8> {
    let device = context.device();
    let source = crate::shader::ENVIRONMENT_WGSL.replace(
        "apply_exposure(apply_height_fog(colour, input.world_position)), albedo_sample.a",
        "albedo_sample.rgb, albedo_sample.a",
    );
    let pipelines = ScenePipelines::with_environment(
        device,
        (TextureFormat::Rgba8Unorm, samples),
        &source,
        crate::shader::TERRAIN_WGSL,
        None,
    );
    let gpu = SceneGpu::upload(
        device,
        context.submission(),
        &gradient_scene(enabled),
        &pipelines,
    )
    .unwrap();
    let frame = FrameUniform {
        view_projection: glam::Mat4::IDENTITY,
        ..frame_from(pose())
    };
    gpu.set_frame(context.queue(), frame);
    let background = background(context, &pipelines, &frame);
    let color = attachment(device, TextureFormat::Rgba8Unorm, samples);
    let resolved = attachment(device, TextureFormat::Rgba8Unorm, 1);
    let depth = attachment(device, DEPTH_FORMAT, samples);
    let color_view = color.create_view(&TextureViewDescriptor::default());
    let resolved_view = resolved.create_view(&TextureViewDescriptor::default());
    let depth_view = depth.create_view(&TextureViewDescriptor::default());
    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor::default());
    draw(
        &mut encoder,
        (&color_view, &resolved_view, &depth_view),
        &gpu,
        &pipelines,
        samples,
        &background,
    );
    let output = if samples == 1 { &color } else { &resolved };
    finish(context, encoder, output)
}

fn finish(context: &GpuContext, mut encoder: CommandEncoder, output: &Texture) -> Vec<u8> {
    let device = context.device();
    let readback = device.create_buffer(&BufferDescriptor {
        label: None,
        size: u64::from(TARGET * TARGET * 4),
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    encoder.copy_texture_to_buffer(
        output.as_image_copy(),
        TexelCopyBufferInfo {
            buffer: &readback,
            layout: TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(TARGET * 4),
                rows_per_image: Some(TARGET),
            },
        },
        output.size(),
    );
    context.queue().submit([encoder.finish()]);
    map_pixels(device, &readback)
}

fn draw(
    encoder: &mut CommandEncoder,
    views: (&TextureView, &TextureView, &TextureView),
    gpu: &SceneGpu,
    pipelines: &ScenePipelines,
    samples: u32,
    background: &SceneGpu,
) {
    let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
        label: None,
        color_attachments: &[Some(RenderPassColorAttachment {
            view: views.0,
            depth_slice: None,
            resolve_target: (samples > 1).then_some(views.1),
            ops: Operations {
                load: LoadOp::Clear(Color::TRANSPARENT),
                store: StoreOp::Store,
            },
        })],
        depth_stencil_attachment: Some(RenderPassDepthStencilAttachment {
            view: views.2,
            depth_ops: Some(Operations {
                load: LoadOp::Clear(1.0),
                store: StoreOp::Store,
            }),
            stencil_ops: None,
        }),
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });
    gpu.draw(&mut pass, pipelines);
    background.draw(&mut pass, pipelines);
}

#[test]
fn four_sample_alpha_gradient_resolves_to_quarter_coverage() {
    let context = context();
    let hard = coverage(&context, Some(false), 4);
    assert!(
        hard.chunks_exact(4)
            .all(|pixel| [0, 255].contains(&pixel[0]))
    );
    let smooth = coverage(&context, Some(true), 4);
    let levels: std::collections::BTreeSet<_> = smooth.chunks_exact(4).map(|p| p[0]).collect();
    assert!(
        levels
            .iter()
            .all(|value| [0, 64, 128, 191, 255].contains(value)),
        "{levels:?}"
    );
    assert!(
        levels.iter().any(|value| *value > 0 && *value < 255),
        "{levels:?}"
    );
    // Interior alpha = 0.5 + (x-y)/64 and fwidth = 2/64. Moving
    // the cutoff by 1/128 predicts one and three covered samples on
    // the two transition diagonals, independently of sample ordering.
    for y in 16..48_usize {
        for (x, expected) in [(y - 1, 0), (y, 64), (y + 1, 191), (y + 2, 255)] {
            assert_eq!(smooth[(y * 64 + x) * 4], expected, "({x},{y})");
        }
    }
    // A later blue background must fill every uncovered depth sample.
    assert!(smooth.chunks_exact(4).all(|pixel| pixel[2] == 255));
    assert_ne!(smooth, hard);
    assert_eq!(hard, coverage(&context, None, 4));
}

#[test]
fn single_sample_alpha_coverage_keeps_hard_cutoff_bytes() {
    let context = context();
    assert_eq!(
        coverage(&context, Some(true), 1),
        coverage(&context, Some(false), 1)
    );
}

#[test]
fn metadata_prepass_and_shadow_keep_hard_cutoff_bytes() {
    let context = context();
    let pipelines = ScenePipelines::new(context.device(), COLOR_FORMAT);
    let capture = |enabled| {
        let gpu = SceneGpu::upload(
            context.device(),
            context.submission(),
            &gradient_scene(Some(enabled)),
            &pipelines,
        )
        .unwrap();
        gpu.set_frame(context.queue(), frame_from(pose()));
        super::foliage::geometry_sources()
            .iter()
            .enumerate()
            .skip(1)
            .map(|(index, source)| {
                super::foliage::pass_depth(
                    &context,
                    &gpu,
                    &pipelines,
                    source,
                    if index == 3 {
                        "fs_depth_masked"
                    } else {
                        "fs_masked"
                    },
                    index != 3,
                )
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(capture(true), capture(false));
}

fn background(context: &GpuContext, pipelines: &ScenePipelines, frame: &FrameUniform) -> SceneGpu {
    let texture = SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Linear,
        width: 1,
        height: 1,
        mips: 1,
        bytes: vec![0, 0, 255, 255].into(),
    };
    let scene = build(texture, &material(SceneAlphaMode::Opaque, true), quad(), 1);
    let gpu = SceneGpu::upload(context.device(), context.submission(), &scene, pipelines).unwrap();
    gpu.set_frame(
        context.queue(),
        FrameUniform {
            view_projection: glam::Mat4::from_translation(glam::Vec3::new(0.0, 0.0, 0.5)),
            ..*frame
        },
    );
    gpu
}
