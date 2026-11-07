//! Resolve-free, injective float-bit readback of every attachment sample.
use super::*;
use wgpu::{
    BindGroupDescriptor, BindGroupEntry, BindingResource, PipelineCompilationOptions,
    ShaderModuleDescriptor, ShaderSource,
};

pub(super) fn attachment(device: &Device, format: TextureFormat, samples: u32) -> Texture {
    device.create_texture(&TextureDescriptor {
        label: Some("forward differential samples"),
        size: Extent3d {
            width: SIZE[0],
            height: SIZE[1],
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: samples,
        dimension: TextureDimension::D2,
        format,
        usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    })
}

pub(crate) fn read(context: &GpuContext, texture: &Texture) -> Vec<u8> {
    let device = context.device();
    let samples = texture.sample_count();
    let [width, height] = [texture.width(), texture.height()];
    let depth = texture.format().is_depth_stencil_format();
    let kind = match (depth, samples) {
        (true, 1) => "texture_depth_2d",
        (true, _) => "texture_depth_multisampled_2d",
        (false, 1) => "texture_2d<f32>",
        (false, _) => "texture_multisampled_2d<f32>",
    };
    let load = if samples == 1 {
        "textureLoad(input, vec2<i32>(id.xy), 0)"
    } else {
        "textureLoad(input, vec2<i32>(id.xy), i32(id.z))"
    };
    let value = if depth {
        format!("vec4<f32>({load}, 0.0, 0.0, 0.0)")
    } else {
        load.to_owned()
    };
    let source = format!(
        "@group(0) @binding(0) var input: {kind};\n@group(0) @binding(1) var<storage, read_write> output: array<vec4<u32>>;\n@compute @workgroup_size(1) fn read(@builtin(global_invocation_id) id: vec3<u32>) {{ output[(id.y * {width}u + id.x) * {samples}u + id.z] = bitcast<vec4<u32>>({value}); }}"
    );
    let module = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("per-sample bit copy"),
        source: ShaderSource::Wgsl(source.into()),
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: None,
        layout: None,
        module: &module,
        entry_point: Some("read"),
        compilation_options: PipelineCompilationOptions::default(),
        cache: None,
    });
    let size = u64::from(width * height * samples * 16);
    let output = device.create_buffer(&BufferDescriptor {
        label: None,
        size,
        usage: BufferUsages::STORAGE | BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let readback = device.create_buffer(&BufferDescriptor {
        label: None,
        size,
        usage: BufferUsages::MAP_READ | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let view = texture.create_view(&TextureViewDescriptor {
        aspect: if texture.format().is_depth_stencil_format() {
            wgpu::TextureAspect::DepthOnly
        } else {
            wgpu::TextureAspect::All
        },
        ..Default::default()
    });
    let group = device.create_bind_group(&BindGroupDescriptor {
        label: None,
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[
            BindGroupEntry {
                binding: 0,
                resource: BindingResource::TextureView(&view),
            },
            BindGroupEntry {
                binding: 1,
                resource: output.as_entire_binding(),
            },
        ],
    });
    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor::default());
    {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor::default());
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.dispatch_workgroups(width, height, samples);
    }
    encoder.copy_buffer_to_buffer(&output, 0, &readback, 0, size);
    context.queue().submit([encoder.finish()]);
    let pixels = map_pixels(device, &readback);
    assert!(
        pixels
            .chunks_exact(4)
            .all(|lane| f32::from_le_bytes(lane.try_into().unwrap()).is_finite()),
        "float texture loads must preserve each finite attachment value exactly"
    );
    pixels
}

pub(crate) fn screen_group(
    context: &GpuContext,
    layout: &wgpu::BindGroupLayout,
) -> wgpu::BindGroup {
    use wgpu::util::DeviceExt;
    let device = context.device();
    let views = [1, 6, 1].map(|layers| screen_view(context, layers));
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::MipmapFilterMode::Linear,
        ..Default::default()
    });
    let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: &[1_000_000_u32, 0, 0, 0]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect::<Vec<_>>(),
        usage: BufferUsages::UNIFORM,
    });
    device.create_bind_group(&BindGroupDescriptor {
        label: None,
        layout,
        entries: &[
            BindGroupEntry {
                binding: 0,
                resource: BindingResource::TextureView(&views[0]),
            },
            BindGroupEntry {
                binding: 1,
                resource: BindingResource::TextureView(&views[1]),
            },
            BindGroupEntry {
                binding: 2,
                resource: BindingResource::TextureView(&views[2]),
            },
            BindGroupEntry {
                binding: 3,
                resource: BindingResource::Sampler(&sampler),
            },
            BindGroupEntry {
                binding: 4,
                resource: uniform.as_entire_binding(),
            },
        ],
    })
}

fn screen_view(context: &GpuContext, layers: u32) -> TextureView {
    let device = context.device();
    let texture = device.create_texture(&TextureDescriptor {
        label: Some("synthetic forward environment"),
        size: Extent3d {
            width: 4,
            height: 4,
            depth_or_array_layers: layers,
        },
        mip_level_count: 3,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::Rgba8Unorm,
        usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
        view_formats: &[],
    });
    for level in 0..3 {
        let size = 4 >> level;
        let bytes: Vec<_> = (0..size * size * layers)
            .flat_map(|i| [180 + u8::try_from(i % 31).unwrap(), 91, 47, 255])
            .collect();
        context.queue().write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: level,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &bytes,
            TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(size * 4),
                rows_per_image: Some(size),
            },
            Extent3d {
                width: size,
                height: size,
                depth_or_array_layers: layers,
            },
        );
    }
    texture.create_view(&TextureViewDescriptor {
        dimension: Some(if layers == 6 {
            wgpu::TextureViewDimension::Cube
        } else {
            wgpu::TextureViewDimension::D2
        }),
        ..Default::default()
    })
}
