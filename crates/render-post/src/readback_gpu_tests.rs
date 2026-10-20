use super::*;

#[test]
fn native_float_readback_preserves_every_bit_and_rejects_wrong_format() {
    let (device, queue) = crate::gpu_tests::gpu();
    for (format, pixel) in [
        (
            wgpu::TextureFormat::Rgba16Float,
            vec![0, 128, 0, 124, 1, 126, 0, 60],
        ),
        (
            wgpu::TextureFormat::R32Float,
            0x7fc0_0123_u32.to_le_bytes().to_vec(),
        ),
        (
            wgpu::TextureFormat::Rgba32Float,
            [0x8000_0000_u32, 0x7f80_0000, 0x7fc0_0123, 0x3f80_0000]
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect(),
        ),
    ] {
        let config = FrameConfig::new([17, 3], format, 1);
        let source = diagnostic_texture(&device, format);
        let bytes = pixel.repeat(51);
        queue.write_texture(
            source.as_image_copy(),
            &bytes,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(17 * u32::try_from(pixel.len()).unwrap()),
                rows_per_image: Some(3),
            },
            source.size(),
        );
        let mut readback = Readback::new(&device, config, 4096).unwrap();
        for _ in 0..2 {
            assert_eq!(readback.read(&device, &queue, &source).unwrap(), bytes);
        }
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        readback.encode(&mut encoder, &source).unwrap();
        let submission = queue.submit([encoder.finish()]);
        let padded = readback
            .read_encoded_padded(&device, Some(submission))
            .unwrap();
        let pitch = readback.padded_bytes_per_row();
        assert!(pitch.is_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT));
        assert_eq!(
            unpad_rows(pitch, 17 * pixel.len(), 3, &padded).unwrap(),
            bytes
        );
        let wrong = color_texture(
            &device,
            "wrong format",
            config.size,
            wgpu::TextureFormat::Rgba8Unorm,
        );
        assert!(matches!(
            readback.read(&device, &queue, &wrong),
            Err(ReadbackError::Config(PostError::ReadbackSource))
        ));
        assert_eq!(readback.read(&device, &queue, &source).unwrap(), bytes);
    }
}

#[test]
fn ordered_capture_observes_copy_point_and_preserves_opaque_diagnostic_bytes() {
    let (device, queue) = crate::gpu_tests::gpu();
    let config = FrameConfig::new([17, 3], wgpu::TextureFormat::Bgra8Unorm, 1);
    let source = color_texture(&device, "ordered capture", config.size, config.format);
    let mut readback = Readback::new(&device, config, 4096).unwrap();
    let mut diagnostic = BufferReadback::new(&device, 4, 4).unwrap();
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 4,
        usage: wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    for bits in [0x7fc0_1234_u32, 0x8000_0000] {
        queue.write_buffer(&buffer, 0, &bits.to_le_bytes());
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        clear_capture(
            &mut encoder,
            &source,
            wgpu::Color {
                r: 1.0,
                g: 0.0,
                b: 0.5,
                a: 1.0,
            },
        );
        readback.encode(&mut encoder, &source).unwrap();
        diagnostic.encode(&mut encoder, &buffer).unwrap();
        clear_capture(&mut encoder, &source, wgpu::Color::BLACK);
        let submission = queue.submit([encoder.finish()]);
        assert_eq!(
            readback.read_encoded(&device, Some(submission)).unwrap(),
            [128, 0, 255, 255].repeat(51)
        );
        assert_eq!(
            diagnostic.read_encoded(&device, None).unwrap(),
            bits.to_le_bytes()
        );
    }
    assert!(matches!(
        BufferReadback::new(&device, 0, 4),
        Err(ReadbackError::Limit)
    ));
    assert!(matches!(
        BufferReadback::new(&device, 3, 4),
        Err(ReadbackError::Limit)
    ));
    assert!(matches!(
        BufferReadback::new(&device, 8, 4),
        Err(ReadbackError::Limit)
    ));
}

fn diagnostic_texture(device: &wgpu::Device, format: wgpu::TextureFormat) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("native float diagnostic"),
        size: wgpu::Extent3d {
            width: 17,
            height: 3,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    })
}

fn clear_capture(encoder: &mut wgpu::CommandEncoder, source: &wgpu::Texture, color: wgpu::Color) {
    let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: &source.create_view(&wgpu::TextureViewDescriptor::default()),
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(color),
                store: wgpu::StoreOp::Store,
            },
        })],
        ..Default::default()
    });
}
