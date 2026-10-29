use super::*;
use worldrunner_render_plan::{SceneDownsampleRounding, SceneSsrPyramid};

fn full(rounding: SceneDownsampleRounding) -> SceneSsr {
    SceneSsr {
        radiance_pyramid: SceneSsrPyramid::Full,
        radiance_rounding: rounding,
        ..settings()
    }
}

fn oracle(mut size: [u32; 2], ceil: bool) -> Vec<[u32; 2]> {
    let mut result = vec![size];
    while size != [1, 1] {
        size = size.map(|n| (n / 2 + u32::from(ceil && n % 2 != 0)).max(1));
        result.push(size);
    }
    result
}

#[test]
fn full_chain_counts_and_rejection_precede_allocation() {
    for samples in crate::msaa_tests::all_counts() {
        assert_eq!(
            pyramid::reduction_source(samples, &settings()),
            gpu::reduce_source(samples)
        );
    }
    for (rounding, size) in [
        (SceneDownsampleRounding::Ceil, [257, 5]),
        (SceneDownsampleRounding::Floor, [513, 5]),
    ] {
        let policy = full(rounding);
        let expected = oracle(size, rounding == SceneDownsampleRounding::Ceil);
        assert_eq!(expected.len(), 10);
        assert_eq!(policy.radiance_extents(size), expected);
        for samples in crate::msaa_tests::all_counts() {
            let config = FrameConfig::new(size, wgpu::TextureFormat::Rgba16Float, samples);
            let mut limits = wgpu::Limits::downlevel_defaults();
            limits.max_texture_dimension_2d = 8192;
            limits.max_sampled_textures_per_shader_stage = 16;
            assert!(validate_extent(config, &policy, &limits).is_ok());
            limits.max_sampled_textures_per_shader_stage = 10;
            assert!(validate_extent(config, &policy, &limits).is_err());
            limits.max_sampled_textures_per_shader_stage = 16;
            limits.max_texture_dimension_2d = size[0] * samples - 1;
            assert!(validate_extent(config, &policy, &limits).is_err());
        }
    }
    let config = FrameConfig::new([u32::MAX, 1], wgpu::TextureFormat::Rgba16Float, 4);
    assert!(
        validate_extent(
            config,
            &full(SceneDownsampleRounding::Ceil),
            &wgpu::Limits::default()
        )
        .is_err()
    );
}

fn level_bytes(
    context: &GpuContext,
    texture: &wgpu::Texture,
    mip: u32,
    size: [u32; 2],
    grid: [u32; 2],
) -> Vec<u8> {
    let samples = grid[0] * grid[1];
    let target = crate::ssao::passes::texture(
        context.device(),
        [size[0] * samples, size[1]],
        texture.format(),
    );
    let mut encoder = context.submission().begin(context.device()).unwrap();
    for lane in 0..samples {
        encoder.copy_texture_to_texture(
            wgpu::TexelCopyTextureInfo {
                mip_level: mip,
                origin: wgpu::Origin3d {
                    x: lane % grid[0] * size[0],
                    y: lane / grid[0] * size[1],
                    z: 0,
                },
                ..texture.as_image_copy()
            },
            wgpu::TexelCopyTextureInfo {
                origin: wgpu::Origin3d {
                    x: lane * size[0],
                    y: 0,
                    z: 0,
                },
                ..target.as_image_copy()
            },
            wgpu::Extent3d {
                width: size[0],
                height: size[1],
                depth_or_array_layers: 1,
            },
        );
    }
    encoder.finish().submit().unwrap();
    read(context, &target)
}

fn draw_level(
    context: &GpuContext,
    pipeline: &wgpu::RenderPipeline,
    group: &wgpu::BindGroup,
    target: (&wgpu::Texture, u32, [u32; 2]),
) {
    let view = pyramid::view(target.0, target.1);
    let mut encoder = context.submission().begin(context.device()).unwrap();
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        pass.set_scissor_rect(0, 0, target.2[0], target.2[1]);
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, group, &[]);
        pass.draw(0..3, 0..1);
    }
    encoder.finish().submit().unwrap();
}

fn reduce_chain(context: &GpuContext, config: FrameConfig, policy: &SceneSsr) -> Vec<Vec<u8>> {
    let textures = gpu::radiance_textures(context.device(), config, policy);
    let sizes = policy.radiance_extents(config.size);
    let shared = pyramid::shared(policy);
    let grid = if shared {
        pyramid::lane_grid(config.samples)
    } else {
        [config.samples, 1]
    };
    let mut values = Vec::new();
    for y in 0..textures[0].height() {
        for x in 0..textures[0].width() {
            let valid = x < sizes[0][0] * grid[0] && y < sizes[0][1] * grid[1];
            let value = if valid {
                gpu::scalar(
                    (x % sizes[0][0] + y % sizes[0][1]) % 7
                        + (x / sizes[0][0] + y / sizes[0][1] * grid[0]) * 8,
                )
            } else {
                99.0
            };
            values.extend([value, value * 0.5, 0.25, 1.0]);
        }
    }
    upload(context, &textures[0], &values);
    let uniforms = pyramid::uniforms(context.device(), config, policy);
    let pipeline = gpu::reduction_pipeline(context.device(), config.samples, policy);
    let mut result = Vec::new();
    for (index, size) in sizes.iter().enumerate() {
        let mip = if shared {
            u32::try_from(index).unwrap()
        } else {
            0
        };
        let packed = [size[0] * grid[0], size[1] * grid[1]];
        if index > 0 {
            let source = pyramid::view(&textures[index - 1], mip.saturating_sub(1));
            let group = gpu::group(
                context.device(),
                &pipeline,
                uniforms.get(index - 1),
                &[source],
            );
            draw_level(context, &pipeline, &group, (&textures[index], mip, packed));
        }
        result.push(level_bytes(context, &textures[index], mip, *size, grid));
    }
    result
}

#[test]
fn gpu_full_pyramid_preserves_first_six_levels_and_sample_lanes() {
    let context = context();
    for (rounding, size) in [
        (SceneDownsampleRounding::Ceil, [257, 5]),
        (SceneDownsampleRounding::Floor, [513, 5]),
        (SceneDownsampleRounding::Ceil, [5, 257]),
        (SceneDownsampleRounding::Ceil, [1, 7]),
    ] {
        for samples in crate::msaa_tests::supported_samples(&context) {
            let config = FrameConfig::new(size, wgpu::TextureFormat::Rgba16Float, samples);
            let policy = full(rounding);
            let actual = reduce_chain(&context, config, &policy);
            let legacy = reduce_chain(
                &context,
                config,
                &SceneSsr {
                    radiance_pyramid: SceneSsrPyramid::SixLevels,
                    ..policy
                },
            );
            assert_eq!(
                actual.len(),
                oracle(size, rounding == SceneDownsampleRounding::Ceil).len()
            );
            assert_eq!(&actual[..legacy.len()], legacy);
            for (lane, pixel) in actual.last().unwrap().chunks_exact(8).enumerate() {
                let red = half::f16::from_le_bytes(pixel[..2].try_into().unwrap()).to_f32();
                let base = gpu::scalar(u32::try_from(lane).unwrap() * 8);
                assert!(
                    (base..=base + 6.0).contains(&red),
                    "sample lane {lane}: {red}"
                );
            }
        }
    }
}

fn upload_lod_tags(context: &GpuContext, texture: &wgpu::Texture, sizes: &[[u32; 2]]) {
    for (level, size) in sizes.iter().enumerate() {
        let mut bytes = Vec::new();
        for y in 0..size[1] * 2 {
            for x in 0..size[0] * 2 {
                let lane = x / size[0] + y / size[1] * 2;
                bytes.extend(
                    [half(gpu::scalar(u32::try_from(level).unwrap() + lane * 16)).to_le_bytes(); 4]
                        .concat(),
                );
            }
        }
        context.queue().write_texture(
            wgpu::TexelCopyTextureInfo {
                mip_level: u32::try_from(level).unwrap(),
                ..texture.as_image_copy()
            },
            &bytes,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(size[0] * 2 * 8),
                rows_per_image: Some(size[1] * 2),
            },
            wgpu::Extent3d {
                width: size[0] * 2,
                height: size[1] * 2,
                depth_or_array_layers: 1,
            },
        );
    }
}

fn lod_pipeline(context: &GpuContext, sizes: &[[u32; 2]]) -> wgpu::RenderPipeline {
    let source = pyramid::resolve_source(gpu::sampled_resolve_source(10, 4, false), sizes, 4);
    let start = source.find("@fragment fn fs_screen").unwrap();
    let end = source.find("fn reflection_coordinates").unwrap();
    let shader = format!("{}\n{}\n@fragment fn fs_screen(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {{
        let roughness = array<f32, 6>(0.0, 0.25, 0.5, 0.75, 0.875, 1.0)[u32(p.x)];
        let lod = reflection_lod(roughness);
        let uv = vec2<f32>(0.999, 0.95);
        let lane = u32(p.y);
        return mix(reflected(u32(floor(lod)), uv, lane), reflected(min(u32(floor(lod))+1u, 9u), uv, lane), fract(lod));
    }}", &source[..start], &source[end..]);
    gpu::pipeline(context.device(), &shader, 1, false)
}

fn lod_group(
    context: &GpuContext,
    pipeline: &wgpu::RenderPipeline,
    texture: &wgpu::Texture,
    mapping: bool,
    downsample: f32,
) -> wgpu::BindGroup {
    use wgpu::util::DeviceExt;
    let mut values = [0.0f32; 52];
    values[34] = 1.0;
    values[38] = downsample;
    values[47] = f32::from(u8::from(mapping));
    let uniform = context
        .device()
        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: &values
                .into_iter()
                .flat_map(f32::to_le_bytes)
                .collect::<Vec<_>>(),
            usage: wgpu::BufferUsages::UNIFORM,
        });
    context
        .device()
        .create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(
                        &texture.create_view(&wgpu::TextureViewDescriptor::default()),
                    ),
                },
            ],
        })
}

#[test]
fn gpu_full_chain_resolve_lod_matches_independent_oracle() {
    let context = context();
    let policy = full(SceneDownsampleRounding::Ceil);
    let config = FrameConfig::new([257, 5], wgpu::TextureFormat::Rgba16Float, 4);
    let sizes = policy.radiance_extents(config.size);
    assert_eq!(sizes.len(), 10);
    let textures = gpu::radiance_textures(context.device(), config, &policy);
    upload_lod_tags(&context, &textures[0], &sizes);
    let pipeline = lod_pipeline(&context, &sizes);
    let target =
        crate::ssao::passes::texture(context.device(), [6, 4], wgpu::TextureFormat::Rgba16Float);
    for (mapping, downsample, expected) in [
        (false, 1.0, [0.0f32, 2.25, 4.5, 6.75, 7.875, 9.0]),
        (true, 1.0, [0.0, 2.25, 4.5, 6.75, 7.875, 9.0]),
        (true, 2.0, [0.0, 1.25, 3.5, 5.75, 6.875, 8.0]),
    ] {
        let group = lod_group(&context, &pipeline, &textures[0], mapping, downsample);
        application_policy_tests::draw_policy(&context, &pipeline, &group, &target);
        for (index, pixel) in read(&context, &target).chunks_exact(8).enumerate() {
            let oracle =
                expected[index % 6] + gpu::scalar(u32::try_from(index / 6).unwrap()) * 16.0;
            assert_eq!(&pixel[..2], &half(oracle).to_le_bytes());
        }
    }
    for samples in crate::msaa_tests::supported_samples(&context) {
        gpu::resolve_pipelines(context.device(), samples, 10, true, false, Some(&sizes));
    }
}

#[test]
fn gpu_full_pyramid_runs_in_the_existing_frame_schedule() {
    let context = context();
    let mut renderer = renderer(&context);
    renderer.ssr = Some(
        Ssr::new(
            context.device(),
            renderer.targets().config(),
            full(SceneDownsampleRounding::Ceil),
            renderer.ssao.as_mut().unwrap(),
        )
        .unwrap(),
    );
    let (color, depth, opaque) = attachments(&renderer);
    let mut encoder = context.submission().begin(context.device()).unwrap();
    renderer
        .encode_frame(
            context.device(),
            &mut encoder,
            (&[opaque], &[color, depth]),
            &mut [],
            (frame(), None),
        )
        .unwrap();
    encoder.finish().submit().unwrap();
    context
        .device()
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(std::time::Duration::from_secs(10)),
        })
        .unwrap();
}

#[test]
fn portrait_four_sample_preflight_fits_stock_limits() {
    let config = FrameConfig::new([1440, 2560], wgpu::TextureFormat::Rgba16Float, 4);
    let policy = full(SceneDownsampleRounding::Ceil);
    let mut limits = wgpu::Limits::downlevel_defaults();
    limits.max_texture_dimension_2d = 8192;
    limits.max_sampled_textures_per_shader_stage = 16;
    assert_eq!(pyramid::storage_extent(config, &policy), Some([8192, 8192]));
    assert!(validate_extent(config, &policy, &limits).is_ok());
    limits.max_texture_dimension_2d = 8191;
    assert!(validate_extent(config, &policy, &limits).is_err());
    let floor = full(SceneDownsampleRounding::Floor);
    let single = FrameConfig::new([513, 5], wgpu::TextureFormat::Rgba16Float, 1);
    assert_eq!(pyramid::storage_extent(single, &floor), Some([513, 5]));
}
