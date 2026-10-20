use std::{sync::mpsc, time::Duration};
use worldrunner_render_gpu::{device::*, texture::*};

fn read_level(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    level: u32,
    layer: u32,
) -> Vec<u8> {
    let width = (texture.width() >> level).max(1);
    let height = (texture.height() >> level).max(1);
    let format = texture.format();
    let (bw, bh) = format.block_dimensions();
    let row_bytes = width.div_ceil(bw) * format.block_copy_size(None).unwrap();
    let rows = height.div_ceil(bh);
    let pitch = row_bytes.next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("contract readback"),
        size: u64::from(pitch) * u64::from(rows),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: level,
            origin: wgpu::Origin3d {
                x: 0,
                y: 0,
                z: layer,
            },
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(pitch),
                rows_per_image: Some(rows),
            },
        },
        wgpu::Extent3d {
            width: width.next_multiple_of(bw),
            height: height.next_multiple_of(bh),
            depth_or_array_layers: 1,
        },
    );
    queue.submit([encoder.finish()]);
    mapped_rows(device, &buffer, pitch, row_bytes)
}

#[test]
fn real_gpu_preserves_raw_integer_bc_mips_layers_reduction_and_cube_bytes() {
    let instance = headless_instance();
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .expect("GPU contract requires a real PRIMARY adapter (WGPU_BACKEND may override)");
    eprintln!("GPU contracts: {:?}", adapter.get_info());
    let (device, queue, negotiated) = pollster::block_on(request_device(
        &adapter,
        &DeviceRequirements {
            required_features: wgpu::Features::TEXTURE_COMPRESSION_BC,
            required_limits: wgpu::Limits {
                max_bind_groups: 8,
                ..Default::default()
            },
            ..Default::default()
        },
    ))
    .unwrap();
    assert!(
        negotiated
            .enabled_features
            .contains(wgpu::Features::TEXTURE_COMPRESSION_BC)
    );
    assert_eq!(device.limits().max_bind_groups, 8);
    let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    check_arrays(&device, &queue);
    check_cube(&device, &queue);
    assert!(pollster::block_on(scope.pop()).is_none());
}

fn check_arrays(device: &wgpu::Device, queue: &wgpu::Queue) {
    for format in [
        wgpu::TextureFormat::R8Uint,
        wgpu::TextureFormat::Rgba8UnormSrgb,
        wgpu::TextureFormat::Bc1RgbaUnorm,
        wgpu::TextureFormat::Bc3RgbaUnormSrgb,
        wgpu::TextureFormat::Bc4RUnorm,
        wgpu::TextureFormat::Bc5RgUnorm,
        wgpu::TextureFormat::Bc7RgbaUnorm,
    ] {
        let sources = [vec![37; 4096], vec![193; 4096]];
        let mip_sets: Vec<_> = sources
            .iter()
            .map(|bytes| packed_mips(format, 12, 20, 5, bytes).unwrap())
            .collect();
        let layers: Vec<_> = mip_sets
            .iter()
            .map(|mips| TextureLayer {
                format,
                width: 12,
                height: 20,
                mips,
            })
            .collect();
        let texture = TextureUpload::new(&layers, 0, TextureKind::Array)
            .unwrap()
            .upload(
                device,
                queue,
                "array contract",
                wgpu::TextureUsages::COPY_SRC,
            )
            .unwrap();
        for (layer, mips) in mip_sets.iter().enumerate() {
            for (level, mip) in mips.iter().enumerate() {
                assert_eq!(
                    read_level(
                        device,
                        queue,
                        &texture,
                        u32::try_from(level).unwrap(),
                        u32::try_from(layer).unwrap()
                    ),
                    mip.bytes,
                    "{format:?}, layer {layer}, mip {level}"
                );
            }
        }
        check_reduction(device, queue, format, &layers, &mip_sets);
    }
}

fn check_cube(device: &wgpu::Device, queue: &wgpu::Queue) {
    let sources: Vec<_> = (0..6).map(|face| vec![face + 1; 1024]).collect();
    let mips: Vec<_> = sources
        .iter()
        .map(|bytes| packed_mips(wgpu::TextureFormat::Bc1RgbaUnorm, 16, 16, 5, bytes).unwrap())
        .collect();
    let layers: Vec<_> = mips
        .iter()
        .map(|mips| TextureLayer {
            format: wgpu::TextureFormat::Bc1RgbaUnorm,
            width: 16,
            height: 16,
            mips,
        })
        .collect();
    let cube = TextureUpload::new(&layers, 1, TextureKind::Cube)
        .unwrap()
        .upload(
            device,
            queue,
            "cube contract",
            wgpu::TextureUsages::COPY_SRC,
        )
        .unwrap();
    let _view = cube.create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(wgpu::TextureViewDimension::Cube),
        ..Default::default()
    });
    for (face, mips) in mips.iter().enumerate() {
        assert_eq!(
            read_level(device, queue, &cube, 0, u32::try_from(face).unwrap()),
            mips[1].bytes
        );
    }
}

fn mapped_rows(
    device: &wgpu::Device,
    buffer: &wgpu::Buffer,
    pitch: u32,
    row_bytes: u32,
) -> Vec<u8> {
    let (send, recv) = mpsc::sync_channel(1);
    buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            send.send(result).unwrap();
        });
    device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(Duration::from_secs(30)),
        })
        .unwrap();
    recv.recv_timeout(Duration::from_secs(30)).unwrap().unwrap();
    let mapped = buffer.slice(..).get_mapped_range().unwrap();
    let result = mapped
        .chunks_exact(pitch as usize)
        .flat_map(|row| row[..row_bytes as usize].iter().copied())
        .collect();
    drop(mapped);
    buffer.unmap();
    result
}

fn check_reduction(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    format: wgpu::TextureFormat,
    layers: &[TextureLayer<'_>],
    mip_sets: &[Vec<MipLevel<'_>>],
) {
    if !format.is_compressed() {
        let reduced = TextureUpload::new(layers, 1, TextureKind::Array)
            .unwrap()
            .upload(
                device,
                queue,
                "reduced contract",
                wgpu::TextureUsages::COPY_SRC,
            )
            .unwrap();
        assert_eq!(
            (reduced.width(), reduced.height(), reduced.mip_level_count()),
            (6, 10, 4)
        );
        assert_eq!(
            read_level(device, queue, &reduced, 0, 1),
            mip_sets[1][1].bytes
        );
    }
}

#[test]
fn texture_store_reuses_content_only_within_its_device_context() {
    use worldrunner_render_gpu::CanonicalTextureStore;
    let instance = headless_instance();
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .unwrap();
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
    let bytes = [17, 83, 201, 255];
    let mut store = CanonicalTextureStore::new(&device, &queue);
    let first = stored_pixel(&mut store, "caller:content-a");
    let shared = store
        .get_or_upload(
            "caller:content-a",
            "again",
            wgpu::TextureUsages::COPY_SRC,
            || panic!("a cache hit must not prepare/upload again"),
        )
        .unwrap();
    assert_eq!(read_level(&device, &queue, &shared, 0, 0), bytes);
    assert_eq!(first, shared);
    let distinct = stored_pixel(&mut store, "caller:content-b");
    assert_ne!(first, distinct);
    let (other_device, other_queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
    let mut other = CanonicalTextureStore::new(&other_device, &other_queue);
    let independent = stored_pixel(&mut other, "caller:content-a");
    assert_ne!(first, independent);
    assert_eq!(
        read_level(&other_device, &other_queue, &independent, 0, 0),
        bytes
    );
    drop(store);
    assert_eq!(read_level(&device, &queue, &shared, 0, 0), bytes);
}

fn stored_pixel(
    store: &mut worldrunner_render_gpu::CanonicalTextureStore,
    id: &str,
) -> wgpu::Texture {
    let bytes = [17, 83, 201, 255];
    let mips = packed_mips(wgpu::TextureFormat::Rgba8Unorm, 1, 1, 1, &bytes).unwrap();
    store
        .get_or_upload(id, "stored pixel", wgpu::TextureUsages::COPY_SRC, || {
            TextureUpload::new(
                &[TextureLayer {
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    width: 1,
                    height: 1,
                    mips: &mips,
                }],
                0,
                TextureKind::D2,
            )
        })
        .unwrap()
}
