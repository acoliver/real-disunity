//! Shared helpers for the TAA GPU tests: the test-only passes, half-float conversion, texture
//! upload and readback, a passthrough pipeline builder and deterministic noise images.
use crate::cas_reference::Image;
use crate::{FrameConfig, Readback};

/// Test-only passes: a depth upload (`Depth32Float` cannot be a copy destination) and an
/// identity pass to the chain output, which a stage set requires as its last stage.
pub(crate) const SHADER: &str = "
@group(0) @binding(0) var source: texture_2d<f32>;
@vertex fn vertex(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    return vec4<f32>(f32((i << 1u) & 2u) * 2.0 - 1.0, f32(i & 2u) * 2.0 - 1.0, 0.0, 1.0);
}
struct Depth { @builtin(frag_depth) value: f32 }
@fragment fn upload_depth(@builtin(position) p: vec4<f32>) -> Depth {
    return Depth(textureLoad(source, vec2<i32>(p.xy), 0).r);
}
struct SampleDepth { @builtin(frag_depth) value: f32 }
// Runs once per sample. Plane `s` of the source is laid out in columns x * count + s.
@fragment fn upload_sample_depth(
    @builtin(position) p: vec4<f32>,
    @builtin(sample_index) s: u32,
) -> SampleDepth {
    return SampleDepth(textureLoad(source, vec2<i32>(i32(p.x) * 4 + i32(s), i32(p.y)), 0).r);
}
@fragment fn identity(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    return textureLoad(source, vec2<i32>(p.xy), 0);
}
";

/// Round-to-nearest f32 to f16 bits for zero and normal values within the f16 range.
pub(crate) fn f16_bits(v: f32) -> u16 {
    let bits = v.to_bits();
    let sign = u16::try_from((bits >> 16) & 0x8000).unwrap();
    let exponent = i32::try_from((bits >> 23) & 0xff).unwrap() - 127 + 15;
    assert!(exponent < 31, "{v} is outside the f16 range");
    if exponent <= 0 {
        return sign;
    }
    let rounded = ((bits & 0x007f_ffff) + 0x1000) >> 13;
    sign | u16::try_from((u32::try_from(exponent).unwrap() << 10) + rounded).unwrap()
}

pub(crate) fn f16_to_f32(bits: u16) -> f32 {
    let sign = if bits & 0x8000 == 0 { 1.0 } else { -1.0 };
    let exponent = i32::from((bits >> 10) & 0x1f);
    let mantissa = f32::from(bits & 0x3ff);
    if exponent == 0 {
        sign * mantissa * 2f32.powi(-24)
    } else {
        sign * (1.0 + mantissa / 1024.0) * 2f32.powi(exponent - 15)
    }
}

pub(crate) fn write_texture(
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    bytes: &[u8],
    pixel_bytes: u32,
) {
    let size = texture.size();
    queue.write_texture(
        texture.as_image_copy(),
        bytes,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(size.width * pixel_bytes),
            rows_per_image: Some(size.height),
        },
        size,
    );
}

pub(crate) fn sampled(
    device: &wgpu::Device,
    label: &str,
    size: [u32; 2],
    format: wgpu::TextureFormat,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: crate::targets::extent(size),
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    })
}

pub(crate) fn view(texture: &wgpu::Texture) -> wgpu::TextureView {
    texture.create_view(&wgpu::TextureViewDescriptor::default())
}

pub(crate) fn passthrough_pipeline(
    device: &wgpu::Device,
    module: &wgpu::ShaderModule,
    fragment: &str,
    format: Option<wgpu::TextureFormat>,
) -> wgpu::RenderPipeline {
    passthrough_pipeline_with(device, module, fragment, format, 1)
}

pub(crate) fn passthrough_pipeline_with(
    device: &wgpu::Device,
    module: &wgpu::ShaderModule,
    fragment: &str,
    format: Option<wgpu::TextureFormat>,
    samples: u32,
) -> wgpu::RenderPipeline {
    let depth = format.is_none();
    let targets: Vec<_> = format.map(|f| Some(f.into())).into_iter().collect();
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(fragment),
        layout: None,
        vertex: wgpu::VertexState {
            module,
            entry_point: Some("vertex"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module,
            entry_point: Some(fragment),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &targets,
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: depth.then(|| wgpu::DepthStencilState {
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

pub(crate) fn read(
    (device, queue): (&wgpu::Device, &wgpu::Queue),
    texture: &wgpu::Texture,
) -> Vec<[f64; 4]> {
    let size = [texture.width(), texture.height()];
    let raw = Readback::new(
        device,
        FrameConfig::new(size, texture.format(), 1),
        1_000_000,
    )
    .unwrap()
    .read(device, &crate::test_authority(queue), texture)
    .unwrap();
    raw.chunks_exact(8)
        .map(|p| {
            std::array::from_fn(|c| {
                let bits = u16::from_le_bytes([p[c * 2], p[c * 2 + 1]]);
                f64::from(f16_to_f32(bits))
            })
        })
        .collect()
}

/// Deterministic value in 0..=1 that varies per pixel and channel.
pub(crate) fn noise(x: i32, y: i32, channel: u32) -> f32 {
    let mut h = u32::try_from(x).unwrap().wrapping_mul(0x9E37_79B1)
        ^ u32::try_from(y).unwrap().wrapping_mul(0x85EB_CA6B)
        ^ channel.wrapping_mul(0xC2B2_AE35);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    f32::from(u16::try_from(h & 0xffff).unwrap()) / 65535.0 * 4.0
}

pub(crate) fn noisy(size: [i32; 2], channel_offset: u32) -> Image {
    Image::from_fn(size, |x, y| {
        [0, 1, 2, 3]
            .map(|c| noise(x, y, c + channel_offset))
            .map(|v| v.min(8.0))
    })
}
