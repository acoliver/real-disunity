//! A texture copy staged on a `PrepareContext` is encoded into the frame's own encoder, in
//! staging order with the copies and dispatches around it, and a frame that does not reach
//! submission copies nothing.
use crate::prepare_context_tests::{buffer, bytes, storage};
use crate::{FrameError, PrepareContext, ResourceRef};

const WIDTH: u32 = 4;

fn extent(width: u32, layers: u32) -> wgpu::Extent3d {
    wgpu::Extent3d {
        width,
        height: 1,
        depth_or_array_layers: layers,
    }
}

fn texture(
    device: &wgpu::Device,
    (width, layers, mips): (u32, u32, u32),
    usage: wgpu::TextureUsages,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("texture copy fixture"),
        size: extent(width, layers),
        mip_level_count: mips,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::R32Uint,
        usage,
        view_formats: &[],
    })
}

fn info(
    texture: &wgpu::Texture,
    mip_level: u32,
    x: u32,
    layer: u32,
) -> wgpu::TexelCopyTextureInfo<'_> {
    wgpu::TexelCopyTextureInfo {
        texture,
        mip_level,
        origin: wgpu::Origin3d { x, y: 0, z: layer },
        aspect: wgpu::TextureAspect::All,
    }
}

fn write(queue: &wgpu::Queue, texture: &wgpu::Texture, mip_level: u32, words: &[u32]) {
    let bytes: Vec<u8> = words.iter().flat_map(|word| word.to_le_bytes()).collect();
    let width = u32::try_from(words.len()).unwrap();
    queue.write_texture(
        info(texture, mip_level, 0, 0),
        &bytes,
        wgpu::TexelCopyBufferLayout::default(),
        extent(width, 1),
    );
}

/// The first `count` texels of `layer` at mip 0, read after the frame was submitted.
fn read(
    (device, queue): (&wgpu::Device, &wgpu::Queue),
    texture: &wgpu::Texture,
    (layer, count): (u32, u32),
) -> Vec<u32> {
    let staging = buffer(device, u64::from(count) * 4);
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    encoder.copy_texture_to_buffer(
        info(texture, 0, 0, layer),
        wgpu::TexelCopyBufferInfo {
            buffer: &staging,
            layout: wgpu::TexelCopyBufferLayout::default(),
        },
        extent(count, 1),
    );
    queue.submit([encoder.finish()]);
    bytes(device, queue, &staging)
        .chunks_exact(4)
        .map(|word| u32::from_le_bytes(word.try_into().unwrap()))
        .collect()
}

fn pipeline(device: &wgpu::Device, source: &str) -> wgpu::ComputePipeline {
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("texture copy fixture"),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("texture copy fixture"),
        layout: None,
        module: &module,
        entry_point: Some("main"),
        compilation_options: wgpu::PipelineCompilationOptions::default(),
        cache: None,
    })
}

/// Doubles each texel of a sampled texture into a storage buffer.
const DOUBLE_TEXTURE: &str = "
@group(0) @binding(0) var source: texture_2d<u32>;
@group(0) @binding(1) var<storage, read_write> doubled: array<u32>;
@compute @workgroup_size(4)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    doubled[id.x] = textureLoad(source, vec2<i32>(i32(id.x), 0), 0).r * 2u;
}
";

/// Writes 100 + x into texel x of a storage texture.
const FILL_TEXTURE: &str = "
@group(0) @binding(0) var written: texture_storage_2d<r32uint, write>;
@compute @workgroup_size(4)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    textureStore(written, vec2<i32>(i32(id.x), 0), vec4<u32>(100u + id.x, 0u, 0u, 0u));
}
";

#[test]
fn a_texture_copy_lands_before_the_dispatch_that_samples_it_and_the_copy_after_it_sees_its_output()
{
    let (device, queue) = crate::gpu_tests::gpu();
    let authority = crate::test_authority(&queue);
    let words = [3_u32, 5, 0x4000_0000, 11];
    let origin = texture(
        &device,
        (WIDTH, 1, 1),
        wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::COPY_DST,
    );
    write(&queue, &origin, 0, &words);
    let sampled = texture(
        &device,
        (WIDTH, 1, 1),
        wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
    );
    let (doubled, copied) = (storage(&device, 16), buffer(&device, 16));
    let pipeline = pipeline(&device, DOUBLE_TEXTURE);
    let view = sampled.create_view(&wgpu::TextureViewDescriptor::default());
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("double texture"),
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: doubled.as_entire_binding(),
            },
        ],
    });
    let prepared = authority
        .prepare(&device, |encoder, _| {
            let mut context = PrepareContext::new(&device, encoder)?;
            context.copy_texture(
                info(&origin, 0, 0, 0),
                info(&sampled, 0, 0, 0),
                extent(WIDTH, 1),
            );
            context.dispatch("double texture", &pipeline, &group, [1, 1, 1]);
            context.copy_buffer_region((&doubled, 0), (&copied, 0), 16);
            context.finish();
            Ok::<_, FrameError>(())
        })
        .unwrap();
    prepared.submit().unwrap();
    let expected: Vec<u8> = words
        .iter()
        .flat_map(|word| (word * 2).to_le_bytes())
        .collect();
    assert_eq!(bytes(&device, &queue, &copied), expected);
}

#[test]
fn a_texture_copy_staged_after_a_dispatch_copies_what_the_dispatch_wrote() {
    let (device, queue) = crate::gpu_tests::gpu();
    let authority = crate::test_authority(&queue);
    let written = texture(
        &device,
        (WIDTH, 1, 1),
        wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::COPY_SRC,
    );
    let landed = texture(
        &device,
        (WIDTH, 1, 1),
        wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::COPY_DST,
    );
    let pipeline = pipeline(&device, FILL_TEXTURE);
    let view = written.create_view(&wgpu::TextureViewDescriptor::default());
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("fill texture"),
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::TextureView(&view),
        }],
    });
    let prepared = authority
        .prepare(&device, |encoder, _| {
            let mut context = PrepareContext::new(&device, encoder)?;
            context.dispatch("fill texture", &pipeline, &group, [1, 1, 1]);
            context.copy_texture(
                info(&written, 0, 0, 0),
                info(&landed, 0, 0, 0),
                extent(WIDTH, 1),
            );
            context.finish();
            Ok::<_, FrameError>(())
        })
        .unwrap();
    prepared.submit().unwrap();
    assert_eq!(
        read((&device, &queue), &landed, (0, WIDTH)),
        [100, 101, 102, 103]
    );
}

#[test]
fn a_texture_copy_between_two_dispatches_stays_between_them() {
    let (device, queue) = crate::gpu_tests::gpu();
    let authority = crate::test_authority(&queue);
    let written = texture(
        &device,
        (WIDTH, 1, 1),
        wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::COPY_SRC,
    );
    let sampled = texture(
        &device,
        (WIDTH, 1, 1),
        wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
    );
    let doubled = storage(&device, 16);
    let copied = buffer(&device, 16);
    let fill = pipeline(&device, FILL_TEXTURE);
    let double = pipeline(&device, DOUBLE_TEXTURE);
    let written_view = written.create_view(&wgpu::TextureViewDescriptor::default());
    let sampled_view = sampled.create_view(&wgpu::TextureViewDescriptor::default());
    let fill_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("fill texture"),
        layout: &fill.get_bind_group_layout(0),
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::TextureView(&written_view),
        }],
    });
    let double_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("double texture"),
        layout: &double.get_bind_group_layout(0),
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&sampled_view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: doubled.as_entire_binding(),
            },
        ],
    });
    let prepared = authority
        .prepare(&device, |encoder, _| {
            let mut context = PrepareContext::new(&device, encoder)?;
            context.dispatch("fill texture", &fill, &fill_group, [1, 1, 1]);
            context.copy_texture(
                info(&written, 0, 0, 0),
                info(&sampled, 0, 0, 0),
                extent(WIDTH, 1),
            );
            context.dispatch("double texture", &double, &double_group, [1, 1, 1]);
            context.copy_buffer_region((&doubled, 0), (&copied, 0), 16);
            context.finish();
            Ok::<_, FrameError>(())
        })
        .unwrap();
    prepared.submit().unwrap();
    let expected: Vec<u8> = [200_u32, 202, 204, 206]
        .iter()
        .flat_map(|word| word.to_le_bytes())
        .collect();
    assert_eq!(bytes(&device, &queue, &copied), expected);
}

#[test]
fn a_texture_copy_takes_the_mip_origin_and_layer_it_is_given() {
    let (device, queue) = crate::gpu_tests::gpu();
    let authority = crate::test_authority(&queue);
    let origin = texture(
        &device,
        (WIDTH, 1, 2),
        wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::COPY_DST,
    );
    write(&queue, &origin, 0, &[1, 2, 3, 4]);
    write(&queue, &origin, 1, &[50, 60]);
    let layers = texture(
        &device,
        (2, 2, 1),
        wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::COPY_DST,
    );
    let prepared = authority
        .prepare(&device, |encoder, _| {
            let mut context = PrepareContext::new(&device, encoder)?;
            context.copy_texture(info(&origin, 0, 2, 0), info(&layers, 0, 0, 0), extent(2, 1));
            context.copy_texture(info(&origin, 1, 0, 0), info(&layers, 0, 0, 1), extent(2, 1));
            context.finish();
            Ok::<_, FrameError>(())
        })
        .unwrap();
    prepared.submit().unwrap();
    assert_eq!(read((&device, &queue), &layers, (0, 2)), [3, 4]);
    assert_eq!(read((&device, &queue), &layers, (1, 2)), [50, 60]);
}

#[test]
fn a_cancelled_or_failed_frame_copies_no_texels() {
    let (device, queue) = crate::gpu_tests::gpu();
    let authority = crate::test_authority(&queue);
    let origin = texture(
        &device,
        (WIDTH, 1, 1),
        wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::COPY_DST,
    );
    write(&queue, &origin, 0, &[7, 7, 7, 7]);
    let destination = texture(
        &device,
        (WIDTH, 1, 1),
        wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::COPY_DST,
    );
    write(&queue, &destination, 0, &[1, 1, 1, 1]);
    let cancelled = authority
        .prepare(&device, |encoder, _| {
            let mut context = PrepareContext::new(&device, encoder)?;
            context.copy_texture(
                info(&origin, 0, 0, 0),
                info(&destination, 0, 0, 0),
                extent(WIDTH, 1),
            );
            context.finish();
            Ok::<_, FrameError>(())
        })
        .unwrap();
    cancelled.cancel();
    assert_eq!(read((&device, &queue), &destination, (0, WIDTH)), [1; 4]);

    let failed = authority.prepare(&device, |encoder, _| {
        let mut context = PrepareContext::new(&device, encoder)?;
        context.copy_texture(
            info(&origin, 0, 0, 0),
            info(&destination, 0, 0, 0),
            extent(WIDTH, 1),
        );
        Err::<(), _>(FrameError::AttachmentView(ResourceRef::new("failed")))
    });
    assert!(failed.is_err());
    assert_eq!(read((&device, &queue), &destination, (0, WIDTH)), [1; 4]);
}
