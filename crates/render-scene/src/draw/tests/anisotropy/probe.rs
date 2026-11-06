//! Whether the device filters explicit-gradient samples anisotropically. Materials sample with
//! `textureSampleGrad` because their derivatives are taken before divergent control flow, and
//! llvmpipe applies anisotropic filtering only to implicit-derivative samples. This probe renders
//! the smallest case directly: one texel per pixel across, sixteen down, over a texture whose
//! level 0 has one-texel vertical stripes and whose other levels are their exact box average.
//! Only the minor axis has detail, so a correct anisotropic filter keeps it and 1x loses it.
use super::super::*;
use wgpu::{
    BindGroup, BindGroupDescriptor, BindGroupEntry, BindingResource, Color, FragmentState, LoadOp,
    MultisampleState, Operations, Origin3d, PipelineCompilationOptions, PrimitiveState,
    RenderPassColorAttachment, RenderPassDescriptor, RenderPipeline, RenderPipelineDescriptor,
    SamplerDescriptor, ShaderModuleDescriptor, ShaderSource, StoreOp, TexelCopyTextureInfo,
    TextureAspect, TextureDescriptor, TextureDimension, TextureUsages, VertexState,
};

const SIZE: u32 = 256;
const SHADER: &str = "
@group(0) @binding(0) var map: texture_2d<f32>;
@group(0) @binding(1) var map_sampler: sampler;
@vertex fn vs(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let corner = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    return vec4<f32>(corner * 2.0 - 1.0, 0.0, 1.0);
}
@fragment fn fs(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let uv = vec2<f32>(position.x, position.y * 16.0) / 256.0;
    return textureSampleGrad(map, map_sampler, uv, dpdx(uv), dpdy(uv));
}
";

fn checker_mips(context: &GpuContext) -> Texture {
    let levels = SIZE.ilog2() + 1;
    let texture = context.device().create_texture(&TextureDescriptor {
        label: Some("anisotropy probe"),
        size: Extent3d {
            width: SIZE,
            height: SIZE,
            depth_or_array_layers: 1,
        },
        mip_level_count: levels,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::Rgba8Unorm,
        usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
        view_formats: &[],
    });
    for level in 0..levels {
        let side = SIZE >> level;
        let texels = (0..side * side).flat_map(|index| {
            let stripe = (index % side).is_multiple_of(2);
            let value = match (level, stripe) {
                (0, true) => 255,
                (0, false) => 0,
                _ => 128,
            };
            [value, value, value, 255]
        });
        context.queue().write_texture(
            TexelCopyTextureInfo {
                texture: &texture,
                mip_level: level,
                origin: Origin3d::ZERO,
                aspect: TextureAspect::All,
            },
            &texels.collect::<Vec<u8>>(),
            TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(side * 4),
                rows_per_image: None,
            },
            Extent3d {
                width: side,
                height: side,
                depth_or_array_layers: 1,
            },
        );
    }
    texture
}

fn pipeline(device: &Device) -> RenderPipeline {
    let module = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("anisotropy probe"),
        source: ShaderSource::Wgsl(SHADER.into()),
    });
    device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some("anisotropy probe"),
        layout: None,
        vertex: VertexState {
            module: &module,
            entry_point: Some("vs"),
            compilation_options: PipelineCompilationOptions::default(),
            buffers: &[],
        },
        primitive: PrimitiveState::default(),
        depth_stencil: None,
        multisample: MultisampleState::default(),
        fragment: Some(FragmentState {
            module: &module,
            entry_point: Some("fs"),
            compilation_options: PipelineCompilationOptions::default(),
            targets: &[Some(COLOR_FORMAT.into())],
        }),
        multiview_mask: None,
        cache: None,
    })
}

fn bind(
    device: &Device,
    pipeline: &RenderPipeline,
    view: &TextureView,
    anisotropy: u16,
) -> BindGroup {
    let sampler = device.create_sampler(&SamplerDescriptor {
        anisotropy_clamp: anisotropy,
        ..crate::upload::material_sampler_descriptor(
            worldrunner_render_plan::TextureAnisotropy::default(),
        )
    });
    device.create_bind_group(&BindGroupDescriptor {
        label: Some("anisotropy probe"),
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[
            BindGroupEntry {
                binding: 0,
                resource: BindingResource::TextureView(view),
            },
            BindGroupEntry {
                binding: 1,
                resource: BindingResource::Sampler(&sampler),
            },
        ],
    })
}

fn render(context: &GpuContext, view: &TextureView, anisotropy: u16) -> Vec<u8> {
    let device = context.device();
    let pipeline = pipeline(device);
    let group = bind(device, &pipeline, view, anisotropy);
    let color = color_target(device);
    let readback = device.create_buffer(&BufferDescriptor {
        label: Some("anisotropy probe readback"),
        size: u64::from(TARGET) * u64::from(TARGET) * 4,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor { label: None });
    {
        let target = color.create_view(&TextureViewDescriptor::default());
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("anisotropy probe"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: &target,
                resolve_target: None,
                depth_slice: None,
                ops: Operations {
                    load: LoadOp::Clear(Color::BLACK),
                    store: StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.draw(0..3, 0..1);
    }
    let size = Extent3d {
        width: TARGET,
        height: TARGET,
        depth_or_array_layers: 1,
    };
    let layout = TexelCopyBufferLayout {
        offset: 0,
        bytes_per_row: Some(TARGET * 4),
        rows_per_image: Some(TARGET),
    };
    encoder.copy_texture_to_buffer(
        color.as_image_copy(),
        TexelCopyBufferInfo {
            buffer: &readback,
            layout,
        },
        size,
    );
    context.queue().submit([encoder.finish()]);
    map_pixels(device, &readback)
}

/// True when 16x keeps the stripes that 1x averages away, as anisotropic filtering requires.
pub(super) fn explicit_gradients_filter_anisotropically(context: &GpuContext) -> bool {
    let texture = checker_mips(context);
    let view = texture.create_view(&TextureViewDescriptor::default());
    let one = super::horizontal_energy(&render(context, &view, 1));
    let sixteen = super::horizontal_energy(&render(context, &view, 16));
    sixteen > one + 10_000
}
