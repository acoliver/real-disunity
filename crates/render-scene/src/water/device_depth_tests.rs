//! The device depth snapshot keeps the nearest sample of every pixel at each supported sample
//! count. The engine's depth is not reversed, so the nearest sample is the smallest value.
use super::snapshots::Snapshots;
use crate::device::GpuContext;
use worldrunner_render_post::{AttachmentView, FrameConfig, Readback, ResourceRef};

const SIZE: [u32; 2] = [8, 8];

/// A pipeline that writes `depth` to the samples `mask` selects, wherever it draws.
fn covering(context: &GpuContext, samples: u32, depth: f32, mask: u32) -> wgpu::RenderPipeline {
    let shader = format!(
        "
        @vertex fn vs(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {{
            let p = array<vec2<f32>,3>(vec2<f32>(-1.0,-1.0),vec2<f32>(3.0,-1.0),vec2<f32>(-1.0,3.0))[index];
            return vec4<f32>(p, {depth:?}, 1.0);
        }}
        @fragment fn fs() -> @builtin(sample_mask) u32 {{ return {mask}u; }}
    "
    );
    let module = context
        .device()
        .create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("covering samples"),
            source: wgpu::ShaderSource::Wgsl(shader.into()),
        });
    context
        .device()
        .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("covering samples"),
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
                targets: &[],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Always),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: samples,
                ..Default::default()
            },
            multiview_mask: None,
            cache: None,
        })
}

fn color_attachment(view: &wgpu::TextureView) -> wgpu::RenderPassColorAttachment<'_> {
    wgpu::RenderPassColorAttachment {
        view,
        depth_slice: None,
        resolve_target: None,
        ops: wgpu::Operations {
            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
            store: wgpu::StoreOp::Store,
        },
    }
}

#[test]
fn gpu_the_device_depth_snapshot_keeps_the_nearest_sample_of_each_pixel() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = GpuContext::shared_for_test().unwrap();
    let device = context.device();
    for samples in crate::msaa_tests::supported_samples(&context) {
        let config = FrameConfig::new(SIZE, wgpu::TextureFormat::Rgba16Float, samples);
        let depth = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("multisampled opaque depth"),
            size: wgpu::Extent3d {
                width: SIZE[0],
                height: SIZE[1],
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: samples,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        // Sample 0 holds 0.9, sample 1 holds 0.3 and any other sample keeps the cleared 1.0.
        let covers = [(0.9, 0b01), (0.3, 0b10)]
            .map(|(value, mask)| covering(&context, samples, value, mask));
        let mut encoder = context.submission().begin(device).unwrap();
        {
            let view = depth.create_view(&wgpu::TextureViewDescriptor::default());
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("opaque depth"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            for cover in &covers {
                pass.set_pipeline(cover);
                pass.draw(0..3, 0..1);
            }
        }
        let snapshots = Snapshots::new(device, config).with_device_depth(device);
        let source = AttachmentView::new(ResourceRef::new("opaque depth"), &depth).unwrap();
        let group = snapshots.source_group(device, &source, true);
        let written = snapshots
            .device_depth
            .as_ref()
            .expect("allocated on request");
        {
            let linear = snapshots
                .depth
                .create_view(&wgpu::TextureViewDescriptor::default());
            let device_view = written
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default());
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("depth snapshot with device depth"),
                color_attachments: &[
                    Some(color_attachment(&linear)),
                    Some(color_attachment(&device_view)),
                ],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&written.pipeline);
            pass.set_bind_group(0, &group, &[]);
            pass.draw(0..3, 0..1);
        }
        encoder.finish().submit().unwrap();

        let mut readback = Readback::new(
            device,
            FrameConfig::new(SIZE, wgpu::TextureFormat::R32Float, 1),
            1 << 16,
        )
        .unwrap();
        let bytes = readback
            .read(device, context.submission(), &written.texture)
            .unwrap();
        let expected: f32 = if samples == 1 { 0.9 } else { 0.3 };
        for (texel, bytes) in bytes.chunks_exact(4).enumerate() {
            let value = f32::from_le_bytes(bytes.try_into().unwrap());
            assert_eq!(
                value.to_bits(),
                expected.to_bits(),
                "texel {texel} at {samples} samples"
            );
        }
    }
}
