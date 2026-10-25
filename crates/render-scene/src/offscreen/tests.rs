use super::*;

mod adapters;
mod concurrent;
mod delayed_gpu;
mod dynamic;
mod wait;

fn viewport(width: u32, height: u32) -> Viewport {
    Viewport { width, height }
}

#[test]
fn odd_width_rows_are_tightly_packed_without_padding_or_shearing() {
    for width in [1, 3, 63, 64, 65, 257] {
        let layout =
            ReadbackLayout::new(viewport(width, 3), &wgpu::Limits::default(), 1 << 20).unwrap();
        assert_eq!(layout.bytes_per_row % wgpu::COPY_BYTES_PER_ROW_ALIGNMENT, 0);
        assert_eq!(layout.image_bytes, width as usize * 3 * 4);
        let mut padded = vec![0xEE; layout.padded_bytes];
        let mut expected = Vec::new();
        for row in 0..3 {
            let bytes = vec![u8::try_from(row + 1).unwrap(); width as usize * 4];
            let start = row * layout.bytes_per_row as usize;
            padded[start..start + bytes.len()].copy_from_slice(&bytes);
            expected.extend(bytes);
        }
        assert_eq!(layout.unpad(&padded).unwrap(), expected, "width {width}");
        assert!(matches!(
            layout.unpad(&padded[..padded.len() - 1]),
            Err(OffscreenError::ShortReadback { .. })
        ));
    }
}

#[test]
fn zero_extents_texture_limits_and_row_overflow_are_rejected() {
    let limits = wgpu::Limits::default();
    for size in [viewport(0, 1), viewport(1, 0), viewport(0, 0)] {
        assert!(matches!(
            ReadbackLayout::new(size, &limits, u64::MAX),
            Err(OffscreenError::EmptyViewport(_))
        ));
    }
    for size in [
        viewport(limits.max_texture_dimension_2d + 1, 1),
        viewport(1, limits.max_texture_dimension_2d + 1),
    ] {
        assert!(matches!(
            ReadbackLayout::new(size, &limits, u64::MAX),
            Err(OffscreenError::TextureLimit { .. })
        ));
    }
    let enormous = wgpu::Limits {
        max_texture_dimension_2d: u32::MAX,
        max_buffer_size: u64::MAX,
        ..limits
    };
    assert!(matches!(
        ReadbackLayout::new(viewport(u32::MAX, 1), &enormous, u64::MAX),
        Err(OffscreenError::LayoutOverflow)
    ));
}

#[test]
fn staging_cap_includes_padding_and_device_limit_is_independent() {
    let limits = wgpu::Limits::default();
    // 1x2 has only eight image bytes, but requires two complete 256-byte rows.
    assert!(matches!(
        ReadbackLayout::new(viewport(1, 2), &limits, 511),
        Err(OffscreenError::ReadbackLimit {
            required: 512,
            limit: 511
        })
    ));
    assert!(ReadbackLayout::new(viewport(1, 2), &limits, 512).is_ok());
    let constrained = wgpu::Limits {
        max_buffer_size: 511,
        ..limits
    };
    assert!(matches!(
        ReadbackLayout::new(viewport(1, 2), &constrained, u64::MAX),
        Err(OffscreenError::ReadbackLimit {
            required: 512,
            limit: 511
        })
    ));
    assert!(matches!(
        ReadbackLayout::new(viewport(1, 1), &constrained, 0),
        Err(OffscreenError::ReadbackLimit { limit: 0, .. })
    ));
}

fn required_context() -> GpuContext {
    let context = pollster::block_on(GpuContext::request_with_features(wgpu::Features::empty()))
        .expect("required headless GPU acceptance needs an adapter; it must not skip");
    eprintln!("required GPU: {:?}", context.adapter().get_info());
    assert!(
        context.device().features().is_empty(),
        "raw scenes must not require BC"
    );
    context
}

#[test]
fn required_gpu_odd_width_custom_pass_and_repeated_readback() {
    let context = required_context();
    let mut target = OffscreenTarget::new(context.device(), viewport(65, 3)).unwrap();
    for color in [wgpu::Color::RED, wgpu::Color::GREEN] {
        let mut encoder = context
            .device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        {
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target.color_view(),
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
        context.queue().submit([encoder.finish()]);
        let image = target.read_rgba8(&context).unwrap();
        let expected = if color == wgpu::Color::RED {
            [255, 0, 0, 255]
        } else {
            [0, 255, 0, 255]
        };
        assert_eq!(image.viewport, viewport(65, 3));
        assert_eq!(image.pixels.len(), 65 * 3 * 4);
        assert!(image.pixels.chunks_exact(4).all(|pixel| pixel == expected));
        assert_eq!(
            target.read_rgba8(&context).unwrap(),
            image,
            "readback must unmap for reuse"
        );
    }
    assert!(matches!(
        OffscreenTarget::new(context.device(), viewport(0, 3)),
        Err(OffscreenError::EmptyViewport(_))
    ));
    assert!(matches!(
        OffscreenTarget::with_max_readback_bytes(context.device(), viewport(65, 3), 1),
        Err(OffscreenError::ReadbackLimit { .. })
    ));
}

#[test]
fn required_gpu_capture_draws_geometry_and_preserves_upload_errors() {
    let context = required_context();
    let scene = fixture::scene(false);
    let view = fixture::view(&scene, viewport(65, 47));
    let image = capture_scene(&context, &scene, &view).unwrap();
    assert_eq!(image.pixels.len(), 65 * 47 * 4);
    assert_eq!(image.viewport, view.viewport());
    let center = (23 * 65 + 32) * 4;
    let pixel = &image.pixels[center..center + 4];
    assert!(
        pixel[0] > 40 && pixel[1] < 5 && pixel[2] < 5,
        "red quad must cover the center, got {pixel:?}"
    );
    assert_eq!(
        &image.pixels[..4],
        &[0, 0, 0, 255],
        "black clear remains outside the quad"
    );
    assert!(image.pixels.chunks_exact(4).all(|p| p[3] == 255));
    assert_eq!(capture_scene(&context, &scene, &view).unwrap(), image);

    let malformed = fixture::scene(true);
    let oversized_view = fixture::view(
        &malformed,
        viewport(context.device().limits().max_texture_dimension_2d + 1, 1),
    );
    assert!(
        matches!(
            capture_scene(&context, &malformed, &oversized_view),
            Err(OffscreenError::TextureLimit { .. })
        ),
        "target limits must be checked before uploading even a malformed scene"
    );
    let error = capture_scene(&context, &malformed, &view).unwrap_err();
    assert!(matches!(
        error,
        OffscreenError::Draw(DrawError::Texture {
            index: 0,
            source: crate::texture::TextureError::NotADdsContainer
        })
    ));
    assert!(std::error::Error::source(&error).is_some());
    assert_eq!(
        capture_scene(&context, &scene, &view).unwrap(),
        image,
        "failed upload must not poison later captures"
    );
}

#[test]
fn required_gpu_device_loss_is_returned_not_panicked() {
    let context = required_context();
    let mut target = OffscreenTarget::new(context.device(), viewport(3, 2)).unwrap();
    context.device().destroy();
    let error = target.read_rgba8(&context).unwrap_err();
    assert!(
        matches!(error, OffscreenError::Map(_) | OffscreenError::Poll(_)),
        "got {error:?}"
    );
}

mod fixture;

#[test]
fn required_gpu_odd_width_pixel_coordinates_survive_readback() {
    let context = required_context();
    let device = context.device();
    let mut target = OffscreenTarget::new(device, viewport(65, 3)).unwrap();
    let pipeline = coordinate_pipeline(device);
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target.color_view(),
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        pass.set_pipeline(&pipeline);
        pass.draw(0..3, 0..1);
    }
    context.queue().submit([encoder.finish()]);
    let image = target.read_rgba8(&context).unwrap();
    for (row, rgb) in [[255, 0, 0], [0, 255, 0], [0, 0, 255]].iter().enumerate() {
        for column in 0..65 {
            let offset = (row * 65 + column) * 4;
            let expected = if column == 64 {
                [255, 255, 255, 255]
            } else {
                [rgb[0], rgb[1], rgb[2], 255]
            };
            assert_eq!(
                &image.pixels[offset..offset + 4],
                expected,
                "pixel ({column}, {row})"
            );
        }
    }
}

fn coordinate_pipeline(device: &wgpu::Device) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("coordinate-coded readback acceptance"),
        source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(r"
            @vertex fn vs(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
                let xy = array<vec2<f32>, 3>(vec2(-1.0, -1.0), vec2(3.0, -1.0), vec2(-1.0, 3.0));
                return vec4(xy[index], 0.0, 1.0);
            }
            @fragment fn fs(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
                let row = u32(p.y);
                if u32(p.x) == 64u { return vec4(1.0); }
                return vec4(select(0.0, 1.0, row == 0u), select(0.0, 1.0, row == 1u), select(0.0, 1.0, row == 2u), 1.0);
            }
        ")),
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("coordinate-coded readback acceptance"),
        layout: None,
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[],
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: OffscreenTarget::COLOR_FORMAT,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}
