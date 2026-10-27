//! Behavioural claims about placing governed resources on the GPU.
//!
//! These tests run against a real adapter and lean on `wgpu` validation error
//! scopes rather than on assertions about our own code. That is deliberate:
//! the questions worth asking here — is this copy layout legal, does this bind
//! group satisfy this layout — are answered by the driver, not by us, and a
//! test that only checked our arithmetic would agree with itself while the GPU
//! rejected the work.

use super::{
    FRAME_GROUP, MATERIAL_GROUP, frame_bind_group_layout, material_bind_group_layout,
    material_sampler, placeholder_texture, upload_buffer, upload_texture,
};
use crate::device::GpuContext;
use crate::frame::FRAME_UNIFORM_BYTES;
use crate::pipeline::MATERIAL_UNIFORM_BYTES;
use crate::texture::prepare_texture;
use wgpu::{
    BindGroupDescriptor, BindGroupEntry, BindingResource, BufferUsages, Device, ErrorFilter,
    TextureView, TextureViewDescriptor,
};
use worldrunner_render_plan::{SceneTexture, TextureColorSpace, TextureEncoding};

const SUPPORTED_FORMATS: [(u32, u32, TextureColorSpace); 6] = [
    (71, 8, TextureColorSpace::Linear),
    (72, 8, TextureColorSpace::Srgb),
    (77, 16, TextureColorSpace::Linear),
    (78, 16, TextureColorSpace::Srgb),
    (80, 8, TextureColorSpace::Linear),
    (83, 16, TextureColorSpace::Linear),
];

fn context() -> GpuContext {
    pollster::block_on(GpuContext::request())
        .expect("GPU acceptance requires a real adapter with BC support")
}

/// Builds a DX10 DDS container whose payload covers the whole mip chain.
fn dds(dxgi: u32, block_bytes: u32, width: u32, height: u32, mip_count: u32) -> Vec<u8> {
    let mut payload = 0usize;
    for level in 0..mip_count {
        let w = (width >> level).max(1);
        let h = (height >> level).max(1);
        let bytes = w.div_ceil(4).max(1) * block_bytes * h.div_ceil(4).max(1);
        payload += bytes as usize;
    }
    let mut bytes = Vec::with_capacity(148 + payload);
    bytes.extend_from_slice(b"DDS ");
    bytes.extend_from_slice(&124u32.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&height.to_le_bytes());
    bytes.extend_from_slice(&width.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&mip_count.to_le_bytes());
    bytes.resize(76, 0);
    bytes.extend_from_slice(&32u32.to_le_bytes());
    bytes.extend_from_slice(&0x0004u32.to_le_bytes());
    bytes.extend_from_slice(b"DX10");
    bytes.resize(4 + 124, 0);
    bytes.extend_from_slice(&dxgi.to_le_bytes());
    bytes.extend_from_slice(&3u32.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&1u32.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.resize(148 + payload, 0x7F);
    bytes
}

fn encoded(dxgi: u32, block_bytes: u32, width: u32, height: u32, mips: u32) -> SceneTexture {
    let space = SUPPORTED_FORMATS
        .iter()
        .find(|(code, ..)| *code == dxgi)
        .map_or(TextureColorSpace::Linear, |(.., space)| *space);
    SceneTexture::Encoded {
        encoding: TextureEncoding::DdsContainer,
        color_space: space,
        bytes: dds(dxgi, block_bytes, width, height, mips).into(),
    }
}

/// Runs a body inside a validation error scope and returns any error message.
fn validation_error<T>(device: &Device, body: impl FnOnce() -> T) -> Option<String> {
    let scope = device.push_error_scope(ErrorFilter::Validation);
    let _kept = body();
    pollster::block_on(scope.pop()).map(|error| error.to_string())
}

fn view(texture: &wgpu::Texture) -> TextureView {
    texture.create_view(&TextureViewDescriptor::default())
}

#[test]
fn every_supported_block_format_uploads_without_validation_error() {
    let context = context();
    for (dxgi, block_bytes, _) in SUPPORTED_FORMATS {
        let texture = encoded(dxgi, block_bytes, 16, 16, 5);
        let prepared = prepare_texture(&texture).expect("the fixture container parses");
        let error = validation_error(context.device(), || {
            upload_texture(context.device(), context.queue(), &prepared).unwrap()
        });
        assert_eq!(error, None, "DXGI format {dxgi} was rejected on upload");
    }
}

#[test]
fn mip_levels_narrower_than_one_block_upload_at_their_natural_stride() {
    // The tail of a mip chain has rows of 8 or 16 bytes, far below the 256-byte
    // stride a buffer-to-texture copy would demand. Queue::write_texture is
    // used precisely because it stages the copy itself; were that assumption
    // wrong, every texture would fail at its smallest levels, which is exactly
    // where it would be least visible.
    let context = context();
    let texture = encoded(71, 8, 16, 16, 5);
    let prepared = prepare_texture(&texture).expect("the fixture container parses");
    let mips = prepared.mips();
    assert_eq!(mips.len(), 5);
    assert_eq!(
        mips[4].bytes_per_row, 8,
        "the last level is a single block, well under any alignment rule"
    );
    let error = validation_error(context.device(), || {
        upload_texture(context.device(), context.queue(), &prepared).unwrap()
    });
    assert_eq!(error, None, "an unaligned mip stride was rejected");
}

#[test]
fn a_material_bind_group_with_every_texture_present_satisfies_its_layout() {
    let context = context();
    let device = context.device();
    let queue = context.queue();
    let layout = material_bind_group_layout(device);
    let sampler = material_sampler(device);
    let uniform = material_uniform(&context);
    let albedo_source = encoded(72, 8, 16, 16, 5);
    let normal_source = encoded(83, 16, 16, 16, 5);
    let rough_source = encoded(71, 8, 16, 16, 5);
    let data_source = encoded(80, 8, 16, 16, 5);
    let albedo = prepare_texture(&albedo_source).expect("albedo parses");
    let normal = prepare_texture(&normal_source).expect("normal parses");
    let rough = prepare_texture(&rough_source).expect("rough/metal parses");
    let data = prepare_texture(&data_source).expect("single channel parses");
    let textures = [
        upload_texture(device, queue, &albedo).unwrap(),
        upload_texture(device, queue, &normal).unwrap(),
        upload_texture(device, queue, &rough).unwrap(),
        upload_texture(device, queue, &data).unwrap(),
        upload_texture(device, queue, &data).unwrap(),
    ];
    let views: Vec<TextureView> = textures.iter().map(view).collect();
    let error = validation_error(device, || {
        let mut entries = vec![BindGroupEntry {
            binding: 0,
            resource: uniform.as_entire_binding(),
        }];
        for (index, texture_view) in views.iter().enumerate() {
            entries.push(BindGroupEntry {
                binding: u32::try_from(index).expect("five bindings fit") + 1,
                resource: BindingResource::TextureView(texture_view),
            });
        }
        entries.push(BindGroupEntry {
            binding: 6,
            resource: BindingResource::Sampler(&sampler),
        });
        let heights = contact_buffer(device, queue);
        entries.push(BindGroupEntry {
            binding: 7,
            resource: heights.as_entire_binding(),
        });
        let extra = view(&placeholder_texture(device, queue));
        for binding in [8, 9, 10] {
            entries.push(BindGroupEntry {
                binding,
                resource: BindingResource::TextureView(&extra),
            });
        }
        device.create_bind_group(&BindGroupDescriptor {
            label: Some("material"),
            layout: &layout,
            entries: &entries,
        })
    });
    assert_eq!(error, None, "a fully textured material was rejected");
}

#[test]
fn the_placeholder_stands_in_for_all_four_optional_textures_at_once() {
    let context = context();
    let device = context.device();
    let queue = context.queue();
    let layout = material_bind_group_layout(device);
    let sampler = material_sampler(device);
    let uniform = material_uniform(&context);
    let albedo_source = encoded(72, 8, 16, 16, 5);
    let albedo = prepare_texture(&albedo_source).expect("albedo parses");
    let albedo_texture = upload_texture(device, queue, &albedo).unwrap();
    let placeholder = placeholder_texture(device, queue);
    let albedo_view = view(&albedo_texture);
    let placeholder_view = view(&placeholder);
    let error = validation_error(device, || {
        let mut entries = vec![
            BindGroupEntry {
                binding: 0,
                resource: uniform.as_entire_binding(),
            },
            BindGroupEntry {
                binding: 1,
                resource: BindingResource::TextureView(&albedo_view),
            },
        ];
        for binding in 2..=5u32 {
            entries.push(BindGroupEntry {
                binding,
                resource: BindingResource::TextureView(&placeholder_view),
            });
        }
        entries.push(BindGroupEntry {
            binding: 6,
            resource: BindingResource::Sampler(&sampler),
        });
        let heights = contact_buffer(device, queue);
        entries.push(BindGroupEntry {
            binding: 7,
            resource: heights.as_entire_binding(),
        });
        let extra = view(&placeholder_texture(device, queue));
        for binding in [8, 9, 10] {
            entries.push(BindGroupEntry {
                binding,
                resource: BindingResource::TextureView(&extra),
            });
        }
        device.create_bind_group(&BindGroupDescriptor {
            label: Some("material"),
            layout: &layout,
            entries: &entries,
        })
    });
    assert_eq!(
        error, None,
        "the placeholder was rejected in an optional slot"
    );
}

#[test]
fn the_frame_layout_accepts_a_block_of_the_size_the_shader_declares() {
    let context = context();
    let device = context.device();
    let layout = frame_bind_group_layout(device);
    let buffer = upload_buffer(
        device,
        context.queue(),
        "frame",
        BufferUsages::UNIFORM,
        &[0u8; FRAME_UNIFORM_BYTES],
    );
    let error = validation_error(device, || {
        device.create_bind_group(&BindGroupDescriptor {
            label: Some("frame"),
            layout: &layout,
            entries: &[BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            }],
        })
    });
    assert_eq!(error, None, "the frame uniform was rejected");
    assert_eq!(FRAME_GROUP, 0);
    assert_eq!(MATERIAL_GROUP, 1);
}

#[test]
fn encoded_bytes_reach_the_gpu_unchanged() {
    // The ban on unsafe code means every buffer is hand-encoded, so the one
    // thing that must not silently go wrong is the bytes arriving reordered or
    // short. Reading them back is the only way to know.
    let context = context();
    let device = context.device();
    let queue = context.queue();
    let source: Vec<u8> = (0..64u32).map(|value| (value * 3 % 251) as u8).collect();
    let buffer = upload_buffer(
        device,
        queue,
        "vertices",
        BufferUsages::VERTEX | BufferUsages::COPY_SRC,
        &source,
    );
    let staging = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("readback"),
        size: source.len() as u64,
        usage: BufferUsages::MAP_READ | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("readback"),
    });
    encoder.copy_buffer_to_buffer(&buffer, 0, &staging, 0, source.len() as u64);
    queue.submit(Some(encoder.finish()));
    let slice = staging.slice(..);
    slice.map_async(wgpu::MapMode::Read, |result| {
        result.expect("the readback buffer maps");
    });
    device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: None,
        })
        .expect("the queue drains");
    let read = slice
        .get_mapped_range()
        .expect("the mapped range is readable")
        .to_vec();
    staging.unmap();
    assert_eq!(read, source, "the buffer contents changed in transit");
}

#[test]
fn raw_npot_mip_chain_uploads_on_a_real_adapter() {
    let context = context();
    let bytes: Vec<u8> = (0..72)
        .map(|value| u8::try_from(value).expect("fixture byte fits"))
        .collect();
    let source = SceneTexture::Texels {
        format: worldrunner_render_plan::TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Linear,
        width: 5,
        height: 3,
        mips: 3,
        bytes: bytes.into(),
    };
    let prepared = prepare_texture(&source).expect("the NPOT mip chain parses");
    assert_eq!(
        prepared
            .mips()
            .iter()
            .map(|mip| (mip.width, mip.height))
            .collect::<Vec<_>>(),
        [(5, 3), (2, 1), (1, 1)]
    );
    let error = validation_error(context.device(), || {
        upload_texture(context.device(), context.queue(), &prepared).unwrap()
    });
    assert_eq!(error, None, "the GPU rejected the raw NPOT mip chain");
}

fn material_uniform(context: &GpuContext) -> wgpu::Buffer {
    upload_buffer(
        context.device(),
        context.queue(),
        "material",
        BufferUsages::UNIFORM,
        &[0u8; MATERIAL_UNIFORM_BYTES],
    )
}

fn contact_buffer(device: &wgpu::Device, queue: &wgpu::Queue) -> wgpu::Buffer {
    upload_buffer(
        device,
        queue,
        "contact heights",
        BufferUsages::STORAGE,
        &[0u8; 48],
    )
}
