use super::*;
use worldrunner_render_plan::{SceneDownsampleRounding, SceneSsrLodMapping};

fn lod_pipeline(device: &wgpu::Device) -> wgpu::RenderPipeline {
    let production = include_str!("resolve.wgsl");
    let declarations = production.split("@group(0) @binding(1)").next().unwrap();
    let helper = production.split("fn reflection_lod(").nth(1).unwrap();
    let source = format!(
        "{declarations}\nfn reflection_lod({helper}\n
        @fragment fn fs_screen(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {{
            let roughness = array<f32, 6>(0.0, 0.125, 0.25, 0.5, 0.75, 1.0)[u32(p.x)];
            let legacy = clamp(roughness / camera.settings.z, 0.0, 1.0) * 5.0;
            return vec4<f32>(reflection_lod(roughness), legacy, 0.0, 1.0);
        }}"
    )
    .replace("REFLECTION_LEVELS", "5");
    gpu::pipeline(device, &source, 1, false)
}

#[test]
fn gpu_application_lod_matches_literal_oracles_and_default_bytes() {
    let context = context();
    let pipeline = lod_pipeline(context.device());
    let uniform = context.device().create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 208,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let group = gpu::group(context.device(), &pipeline, Some(&uniform), &[]);
    let target =
        crate::ssao::passes::texture(context.device(), [6, 1], wgpu::TextureFormat::Rgba16Float);
    for cutoff in [0.5, 1.0] {
        for (downsample, expected) in [
            (1.0, [0.0, 0.625, 1.25, 2.5, 3.75, 5.0]),
            (2.0, [0.0, 0.0, 0.25, 1.5, 2.75, 4.0]),
        ] {
            for mapping in [
                SceneSsrLodMapping::CutoffNormalized,
                SceneSsrLodMapping::RoughnessDownsample,
            ] {
                let policy = SceneSsr {
                    application_lod: mapping,
                    ..settings()
                };
                let mut values = [0.0f32; 52];
                values[34] = cutoff;
                values[38] = downsample;
                values[40..].copy_from_slice(&gpu::controls(&policy));
                context.queue().write_buffer(
                    &uniform,
                    0,
                    &values
                        .into_iter()
                        .flat_map(f32::to_le_bytes)
                        .collect::<Vec<_>>(),
                );
                draw_policy(&context, &pipeline, &group, &target);
                for (pixel, oracle) in read(&context, &target).chunks_exact(8).zip(expected) {
                    if mapping == SceneSsrLodMapping::CutoffNormalized {
                        assert_eq!(&pixel[..2], &pixel[2..4]);
                    } else {
                        assert_eq!(&pixel[..2], &half(oracle).to_le_bytes());
                    }
                }
            }
        }
    }
}

pub(super) fn draw_policy(
    context: &GpuContext,
    pipeline: &wgpu::RenderPipeline,
    group: &wgpu::BindGroup,
    target: &wgpu::Texture,
) {
    let mut encoder = context.submission().begin(context.device()).unwrap();
    worldrunner_render_post::fullscreen_pass(
        &mut encoder,
        Some("reflection policy"),
        wgpu::RenderPassColorAttachment {
            view: &target.create_view(&wgpu::TextureViewDescriptor::default()),
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                store: wgpu::StoreOp::Store,
            },
        },
        pipeline,
        &[group],
    );
    encoder.finish().submit().unwrap();
}

#[test]
fn gpu_floor_reduction_keeps_sample_lanes_and_drops_incomplete_bins() {
    let context = context();
    let policy = SceneSsr {
        radiance_rounding: SceneDownsampleRounding::Floor,
        ..settings()
    };
    let textures = gpu::radiance_textures(
        context.device(),
        FrameConfig::new([7, 5], wgpu::TextureFormat::Rgba16Float, 4),
        &policy,
    );
    let mut values = Vec::new();
    for y in 0..5 {
        for x in 0..28 {
            let lane = x / 7;
            let value = if x % 7 == 6 || y == 4 {
                9.0
            } else {
                gpu::scalar(lane + 1)
            };
            values.extend([value; 4]);
        }
    }
    upload(&context, &textures[0], &values);
    let pipeline = gpu::pipeline(
        context.device(),
        &gpu::configured_reduce_source(4, policy.radiance_rounding),
        1,
        false,
    );
    let group = gpu::group(
        context.device(),
        &pipeline,
        None,
        &[textures[0].create_view(&wgpu::TextureViewDescriptor::default())],
    );
    draw_policy(&context, &pipeline, &group, &textures[1]);
    assert_eq!([textures[1].width(), textures[1].height()], [12, 2]);
    for (index, pixel) in read(&context, &textures[1]).chunks_exact(8).enumerate() {
        let lane = u32::try_from(index % 12 / 3).unwrap();
        assert_eq!(
            pixel,
            [half(gpu::scalar(lane + 1)).to_le_bytes(); 4].concat()
        );
    }
    assert_eq!(
        gpu::configured_reduce_source(4, SceneDownsampleRounding::Ceil),
        gpu::reduce_source(4)
    );
}
