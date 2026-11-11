//! Synthetic shadow layers with nonzero receiver bias and depth transitions.
use super::*;
use wgpu::util::DeviceExt;
use wgpu::{BindGroupDescriptor, BindGroupEntry, BindingResource};

pub(super) fn binding(
    context: &GpuContext,
    count: u32,
) -> (wgpu::BindGroupLayout, wgpu::BindGroup) {
    let device = context.device();
    let layout = worldrunner_render_shadow::receiver_layout(
        device,
        worldrunner_render_shadow::ReceiverDescriptor {
            label: Some("forward reference shadow"),
            dimension: worldrunner_render_shadow::ReceiverDimension::D2Array,
            texture_binding: 1,
            sampler_binding: 2,
            uniform_binding: 0,
            min_uniform_binding_size: None,
        },
    );
    let depth = device.create_texture(&TextureDescriptor {
        label: None,
        size: Extent3d {
            width: 16,
            height: 16,
            depth_or_array_layers: 4,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: DEPTH_FORMAT,
        usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor::default());
    for layer in 0..4 {
        let view = depth.create_view(&TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2),
            base_array_layer: layer,
            array_layer_count: Some(1),
            ..Default::default()
        });
        let _pass = encoder.begin_render_pass(&RenderPassDescriptor {
            depth_stencil_attachment: Some(RenderPassDepthStencilAttachment {
                view: &view,
                depth_ops: Some(Operations {
                    load: LoadOp::Clear(if layer % 2 == 0 { 0.5 } else { 0.65 }),
                    store: StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            ..Default::default()
        });
    }
    context.queue().submit([encoder.finish()]);
    let uniform = uniform(device, count);
    let view = depth.create_view(&TextureViewDescriptor {
        dimension: Some(wgpu::TextureViewDimension::D2Array),
        ..Default::default()
    });
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        compare: Some(wgpu::CompareFunction::LessEqual),
        ..Default::default()
    });
    let group = device.create_bind_group(&BindGroupDescriptor {
        label: None,
        layout: &layout,
        entries: &[
            BindGroupEntry {
                binding: 0,
                resource: uniform.as_entire_binding(),
            },
            BindGroupEntry {
                binding: 1,
                resource: BindingResource::TextureView(&view),
            },
            BindGroupEntry {
                binding: 2,
                resource: BindingResource::Sampler(&sampler),
            },
        ],
    });
    (layout, group)
}

fn uniform(device: &Device, count: u32) -> wgpu::Buffer {
    let mut floats = Vec::new();
    for _ in 0..4 {
        floats.extend(
            glam::Mat4::from_scale_rotation_translation(
                glam::Vec3::splat(0.2),
                glam::Quat::IDENTITY,
                glam::Vec3::new(0.0, 0.0, 0.45),
            )
            .to_cols_array(),
        );
    }
    floats.extend([0.0, 0.021, 1.0, 0.17]);
    floats.extend([
        if count == 1 { 1.0 } else { 4.0 },
        0.25,
        if count == 1 { 0.0 } else { 1.0 },
        0.1,
    ]);
    floats.extend([0.2, 0.8, 1.5, 4.0]);
    floats.extend([0.0, 0.0, 1.0, 0.0]);
    floats.extend([0.0001; 4]);
    floats.extend([0.0; 4]);
    let bytes: Vec<_> = floats.into_iter().flat_map(f32::to_le_bytes).collect();
    assert_eq!(bytes.len(), 352);
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: &bytes,
        usage: BufferUsages::UNIFORM,
    })
}
