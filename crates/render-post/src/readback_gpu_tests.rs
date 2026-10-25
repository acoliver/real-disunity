use super::*;

#[test]
fn abandoned_capture_readback_reuse_survives_cancel_and_preparation_failure() {
    use worldrunner_render_gpu::submission::{PrepareError, capture::CaptureWrites};
    let (device, queue) = crate::gpu_tests::gpu();
    let authority = crate::test_authority(&queue);
    let config = FrameConfig::new([17, 3], wgpu::TextureFormat::R32Float, 1);
    let source = diagnostic_texture(&device, config.format);
    let mut readback = Readback::new(&device, config, 4096).unwrap();
    for fail_preparation in [false, true] {
        let bytes = if fail_preparation { 456u32 } else { 123u32 }
            .to_le_bytes()
            .repeat(51);
        queue.write_texture(
            source.as_image_copy(),
            &bytes,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(68),
                rows_per_image: Some(3),
            },
            source.size(),
        );
        let prefix = authority
            .begin_capture(
                &device,
                CaptureWrites::new([readback.write_key()], [], []).unwrap(),
                (),
                |encoder, _, ()| readback.encode(encoder, &source),
            )
            .unwrap();
        drop(prefix.submit().unwrap());
        authority.drain_captures(&device).unwrap();
        let serial = authority.submitted_serial();
        let reuse = authority.prepare(&device, |encoder, _| {
            readback.encode(encoder, &source)?;
            if fail_preparation {
                Err(ReadbackError::Callback)
            } else {
                Ok(())
            }
        });
        if fail_preparation {
            assert!(matches!(
                reuse,
                Err(PrepareError::Preparation(ReadbackError::Callback))
            ));
        } else {
            reuse.unwrap().cancel();
        }
        assert_eq!(authority.submitted_serial(), serial);
        assert_eq!(readback.read(&device, &authority, &source).unwrap(), bytes);
    }
}

#[test]
fn abandoned_capture_readback_slot_is_reusable_only_after_drain() {
    use worldrunner_render_gpu::submission::capture::CaptureWrites;
    let (device, queue) = crate::gpu_tests::gpu();
    let authority = crate::test_authority(&queue);
    let config = FrameConfig::new([17, 3], wgpu::TextureFormat::R32Float, 1);
    let source = diagnostic_texture(&device, config.format);
    let mut readback = Readback::new(&device, config, 4096).unwrap();
    let writes = CaptureWrites::new([readback.write_key()], [], []).unwrap();
    let prefix = authority
        .begin_capture(&device, writes, 42u64, |encoder, _, _| {
            readback.encode(encoder, &source)
        })
        .unwrap();
    drop(prefix.submit().unwrap());
    authority.drain_captures(&device).unwrap();
    assert!(readback.read(&device, &authority, &source).is_ok());
}

#[test]
fn retained_texture_readback_declares_capture_slot_and_reuses_it_after_failure() {
    use worldrunner_render_gpu::submission::capture::{CaptureError, CaptureWrites};
    let (device, queue) = crate::gpu_tests::gpu();
    let authority = crate::test_authority(&queue);
    let config = FrameConfig::new([17, 3], wgpu::TextureFormat::R32Float, 1);
    let source = diagnostic_texture(&device, config.format);
    let bytes = 123u32.to_le_bytes().repeat(51);
    queue.write_texture(
        source.as_image_copy(),
        &bytes,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(68),
            rows_per_image: Some(3),
        },
        source.size(),
    );
    let mut readback = Readback::new(&device, config, 4096).unwrap();
    let writes = CaptureWrites::new([readback.write_key()], [], []).unwrap();
    let prefix = authority
        .begin_capture(&device, writes, 42u64, |encoder, _, _| {
            readback.encode(encoder, &source)
        })
        .unwrap();
    let result = prefix.submit().unwrap().resume(
        &device,
        |device, submission| readback.read_encoded(device, Some(submission)),
        |_, _, frame, read| {
            assert_eq!(*frame, 42);
            assert_eq!(*read, bytes);
            Err(ReadbackError::Callback)
        },
    );
    assert!(matches!(
        result,
        Err(CaptureError::Preparation(ReadbackError::Callback))
    ));
    authority.drain_captures(&device).unwrap();
    assert_eq!(readback.read(&device, &authority, &source).unwrap(), bytes);
}

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
            assert_eq!(
                readback
                    .read(&device, &crate::test_authority(&queue), &source)
                    .unwrap(),
                bytes
            );
        }
        assert_padded(&device, &queue, &mut readback, &source, &pixel, &bytes);
        let wrong = color_texture(
            &device,
            "wrong format",
            config.size,
            wgpu::TextureFormat::Rgba8Unorm,
        );
        assert!(matches!(
            readback.read(&device, &crate::test_authority(&queue), &wrong),
            Err(ReadbackError::Config(PostError::ReadbackSource))
        ));
        assert_eq!(
            readback
                .read(&device, &crate::test_authority(&queue), &source)
                .unwrap(),
            bytes
        );
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
        let mut encoder =
            worldrunner_render_gpu::submission::SubmissionAuthority::new(queue.clone())
                .begin(&device)
                .unwrap();
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
        let submission = encoder.finish().submit().unwrap();
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

fn assert_padded(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    readback: &mut Readback,
    source: &wgpu::Texture,
    pixel: &[u8],
    bytes: &[u8],
) {
    let mut encoder = worldrunner_render_gpu::submission::SubmissionAuthority::new(queue.clone())
        .begin(device)
        .unwrap();
    readback.encode(&mut encoder, source).unwrap();
    let submission = encoder.finish().submit().unwrap();
    let padded = readback
        .read_encoded_padded(device, Some(submission))
        .unwrap();
    let pitch = readback.padded_bytes_per_row();
    assert!(pitch.is_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT));
    assert_eq!(
        unpad_rows(pitch, 17 * pixel.len(), 3, &padded).unwrap(),
        bytes
    );
}
