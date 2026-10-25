use super::*;
use worldrunner_render_post::{BufferReadback, FrameError as ScheduleError};

fn texture(context: &GpuContext, format: wgpu::TextureFormat) -> wgpu::Texture {
    context.device().create_texture(&wgpu::TextureDescriptor {
        label: Some("subresource isolation"),
        size: wgpu::Extent3d {
            width: 514,
            height: 482,
            depth_or_array_layers: 2,
        },
        mip_level_count: 2,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}

fn resource(mip: u32, layer: u32) -> ResourceRef {
    ResourceRef {
        mip,
        layer,
        version: 7,
        ..ResourceRef::new("host attachment")
    }
}

fn initialize(context: &GpuContext, color: &wgpu::Texture, depth: &wgpu::Texture) {
    let mut encoder = context.submission().begin(context.device()).unwrap();
    for mip in 0..2 {
        for layer in 0..2 {
            let descriptor = wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::D2),
                base_mip_level: mip,
                mip_level_count: Some(1),
                base_array_layer: layer,
                array_layer_count: Some(1),
                ..Default::default()
            };
            let color_view = color.create_view(&descriptor);
            let depth_view = depth.create_view(&descriptor);
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &color_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 1.0,
                            g: 0.0,
                            b: 1.0,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(0.25),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });
        }
    }
    encoder.finish().submit().unwrap();
}

fn texture_copy(texture: &wgpu::Texture, mip: u32, layer: u32) -> wgpu::TexelCopyTextureInfo<'_> {
    wgpu::TexelCopyTextureInfo {
        texture,
        mip_level: mip,
        origin: wgpu::Origin3d {
            x: 0,
            y: 0,
            z: layer,
        },
        aspect: if texture.format().is_depth_stencil_format() {
            wgpu::TextureAspect::DepthOnly
        } else {
            wgpu::TextureAspect::All
        },
    }
}

fn read_all(context: &GpuContext, texture: &wgpu::Texture) -> Vec<Vec<u8>> {
    let mut encoder = context.submission().begin(context.device()).unwrap();
    let mut captures = Vec::new();
    for mip in 0..2 {
        let width = texture.width() >> mip;
        let height = texture.height() >> mip;
        let pitch = (width * 4).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
        let size = u64::from(pitch) * u64::from(height);
        for layer in 0..2 {
            let buffer = context.device().create_buffer(&wgpu::BufferDescriptor {
                label: Some("subresource copy"),
                size,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            });
            encoder.copy_texture_to_buffer(
                texture_copy(texture, mip, layer),
                wgpu::TexelCopyBufferInfo {
                    buffer: &buffer,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(pitch),
                        rows_per_image: Some(height),
                    },
                },
                wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
            );
            let mut capture = BufferReadback::new(context.device(), size, size).unwrap();
            capture.encode(&mut encoder, &buffer).unwrap();
            captures.push((capture, pitch, width, height));
        }
    }
    let submission = encoder.finish().submit().unwrap();
    captures
        .iter_mut()
        .map(|(capture, pitch, width, height)| {
            read_capture(
                context,
                capture,
                [*pitch, *width, *height],
                submission.clone(),
            )
        })
        .collect()
}

fn read_capture(
    context: &GpuContext,
    capture: &mut BufferReadback,
    [pitch, width, height]: [u32; 3],
    submission: wgpu::SubmissionIndex,
) -> Vec<u8> {
    let bytes = capture
        .read_encoded(context.device(), Some(submission))
        .unwrap();
    worldrunner_render_post::unpad_rows(pitch, usize::try_from(width * 4).unwrap(), height, &bytes)
        .unwrap()
}

fn assert_isolation(before: &[Vec<Vec<u8>>; 2], after: &[Vec<Vec<u8>>; 2], selected: usize) {
    for (old, new) in before.iter().zip(after) {
        for index in 0..4 {
            if index == selected {
                assert_ne!(old[index], new[index], "selected {index}");
            } else {
                assert_eq!(old[index], new[index], "unselected {index}");
            }
        }
    }
    let pixels = &after[0][selected];
    assert!(pixels.chunks_exact(4).any(|rgba| rgba == [0, 0, 0, 255]));
    assert!(pixels.chunks_exact(4).any(|rgba| rgba[1] > 0));
    let depths: Vec<_> = after[1][selected]
        .chunks_exact(4)
        .map(|bytes| f32::from_le_bytes(bytes.try_into().unwrap()))
        .collect();
    assert!(depths.contains(&1.0));
    assert!(depths.iter().any(|value| *value < 1.0));
}

#[test]
fn stock_adapters_render_only_selected_mip_and_layer() {
    let context = context();
    let source = scene(&[[0, 0, 1_000]], 600_000, None);
    let mut shadowed = ShadowedScene::new(&context, &source, settings()).unwrap();
    let pipelines = ScenePipelines::new(context.device(), OffscreenTarget::COLOR_FORMAT);
    let mut gpu =
        SceneGpu::upload(context.device(), context.submission(), &source, &pipelines).unwrap();
    let mut scheduler = crate::compositor::FrameRenderer::new(context.device());
    for shadows in [false, true] {
        for (mip, layer) in [(0, 1), (1, 0), (1, 1)] {
            let color = texture(&context, OffscreenTarget::COLOR_FORMAT);
            let depth = texture(&context, wgpu::TextureFormat::Depth32Float);
            initialize(&context, &color, &depth);
            let before = [read_all(&context, &color), read_all(&context, &depth)];
            let color_view = AttachmentView::new(resource(mip, layer), &color).unwrap();
            let depth_view = AttachmentView::new(resource(mip, layer), &depth).unwrap();
            assert_eq!(color_view.size(), [514 >> mip, 482 >> mip]);
            assert_eq!(color_view.format(), OffscreenTarget::COLOR_FORMAT);
            if shadows {
                // The submitting convenience API must preserve the same selection.
                shadowed
                    .render(&context, &view(), &color_view, &depth_view)
                    .unwrap();
            } else {
                let mut encoder = context.submission().begin(context.device()).unwrap();
                crate::compositor::encode_scene(
                    &mut scheduler,
                    context.device(),
                    &mut encoder,
                    (&mut gpu, &pipelines),
                    (&color_view, &depth_view),
                    FrameUniform::new(&view(), source.effects()).unwrap(),
                )
                .unwrap();
                encoder.finish().submit().unwrap();
            }
            let after = [read_all(&context, &color), read_all(&context, &depth)];
            let selected = usize::try_from(mip * 2 + layer).unwrap();
            assert_isolation(&before, &after, selected);
        }
    }
}

#[test]
fn checked_attachments_reject_out_of_range_selections() {
    let context = context();
    for format in [
        OffscreenTarget::COLOR_FORMAT,
        wgpu::TextureFormat::Depth32Float,
    ] {
        let texture = texture(&context, format);
        for (mip, layer) in [(2, 0), (0, 2), (u32::MAX, 0), (0, u32::MAX)] {
            let id = resource(mip, layer);
            assert!(matches!(AttachmentView::new(id, &texture),
                Err(ScheduleError::AttachmentView(actual)) if actual == id));
        }
    }
}
