//! The depth-aware AO gather receivers of SSAO and of the reflection forward pass, which select
//! a sibling source and a screen layout of their own.

use wgpu::BindGroup;
use worldrunner_render_plan::SceneSsaoReceiver;

use super::{Case, Target, capture_scenes, recipe};
use crate::device::GpuContext;
use crate::dynamic::capture_tests::{CONFIGS, build, context, one_texel, supported};
use crate::pipeline::PipelineKey;
use crate::renderer::legacy_objects::{compose, gathered};

fn gather_ssao(context: &GpuContext) -> BindGroup {
    let device = context.device();
    let filtered = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("capture gather texel"),
        size: wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rg32Float,
        usage: wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let view: Vec<u8> = glam::Mat4::IDENTITY
        .to_cols_array()
        .into_iter()
        .flat_map(f32::to_le_bytes)
        .collect();
    let uniform = crate::upload::upload_buffer(
        device,
        context.queue(),
        "capture gather view",
        wgpu::BufferUsages::UNIFORM,
        &view,
    );
    crate::ssao::receiver::group(
        device,
        &filtered,
        &uniform,
        SceneSsaoReceiver::DepthAwareGather,
    )
}

fn gather_reflection(context: &GpuContext) -> BindGroup {
    let device = context.device();
    let layout = crate::ssr::forward::receiver_layout(device, SceneSsaoReceiver::DepthAwareGather);
    let [cube, plane] = crate::water::receivers::empty_environment(device);
    let ambient = one_texel(device);
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor::default());
    let zeroed = |label, size| {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size,
            usage: wgpu::BufferUsages::UNIFORM,
            mapped_at_creation: false,
        })
    };
    let intensity = zeroed("capture intensity", 16);
    let view = zeroed("capture gather view", 64);
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("capture reflection gather"),
        layout: &layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&ambient),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&cube),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(&plane),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::Sampler(&sampler),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: intensity.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 5,
                resource: view.as_entire_binding(),
            },
        ],
    })
}

#[test]
fn gather_receivers_match_the_fixed_pipeline_byte_for_byte_for_every_key() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let (mut run, mut skipped) = (0, 0);
    for (name, index, group) in [
        ("ssao-gather-1x", 6, gather_ssao(&context)),
        ("reflection-gather-1x", 8, gather_reflection(&context)),
    ] {
        let config = &CONFIGS[index];
        if !supported(&context, config) {
            println!("OBJECT-CAPTURE config={name} skipped: format or samples unsupported");
            skipped += 1;
            continue;
        }
        let built = build(context.device(), config);
        let source = gathered(
            &compose(recipe(config.kind)),
            SceneSsaoReceiver::DepthAwareGather,
        );
        let (keys, compared) = capture_scenes(
            &context,
            &Case {
                name,
                scene: "",
                target: Target {
                    formats: &built.formats,
                    samples: config.samples,
                },
                pipelines: &built.pipelines,
                source: &source,
                screen: Some(&group),
                shadow: None,
                mask: 0,
            },
            Some(SceneSsaoReceiver::DepthAwareGather),
            &mut None,
        );
        assert_eq!(
            keys.len(),
            PipelineKey::ALL.len(),
            "{name}: every key is drawn"
        );
        run += compared;
    }
    println!("OBJECT-CAPTURE-SUMMARY set=gather compared={run} skipped={skipped}");
    assert_eq!(skipped, 0, "a skipped capture fails");
}
