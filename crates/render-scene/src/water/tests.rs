#[path = "contracts.rs"]
mod contracts;
#[path = "numerical.rs"]
mod numerical;
#[path = "regressions.rs"]
mod regressions;

use super::*;
use crate::{CameraPose, SceneView, Viewport, device::GpuContext};
use worldrunner_render_plan::SceneCamera;
use worldrunner_render_post::{FrameConfig, Readback, ToneMap};

fn settings() -> SceneWaterPlane {
    SceneWaterPlane {
        height_millimeters: 0,
        east_min_millimeters: -10000,
        north_min_millimeters: -10000,
        east_extent_millimeters: 20000,
        north_extent_millimeters: 20000,
        color_micro: [50000, 200_000, 250_000],
        absorption_micro: [300_000, 100_000, 50000],
        roughness_micro: 200_000,
    }
}
#[test]
fn absorption_increases_with_distance_and_foreground_is_rejected() {
    for coefficient in [0.05, 0.1, 0.3] {
        assert!((transmittance(coefficient, 0.0) - 1.0).abs() < f32::EPSILON);
        assert!(transmittance(coefficient, 1.0) > transmittance(coefficient, 5.0));
        assert!(transmittance(coefficient, 5.0) > transmittance(coefficient, 20.0));
    }
    assert_eq!(refraction_pixel([3, 4], [5, 6], 2.0, 3.0), [3, 4]);
    assert_eq!(refraction_pixel([3, 4], [5, 6], 4.0, 3.0), [5, 6]);
    assert!(matches!(
        validate_settings(&SceneWaterPlane {
            roughness_micro: 1_000_001,
            ..settings()
        }),
        Err(FrameError::WaterSettings(_))
    ));
    assert!(matches!(
        validate_settings(&SceneWaterPlane {
            absorption_micro: [u32::MAX; 3],
            ..settings()
        }),
        Err(FrameError::WaterSettings(_))
    ));
}
fn read(context: &GpuContext, texture: &wgpu::Texture) -> Vec<u8> {
    Readback::new(
        context.device(),
        FrameConfig::new([32, 32], texture.format(), 1),
        100_000,
    )
    .unwrap()
    .read(context.device(), context.queue(), texture)
    .unwrap()
}
#[test]
fn gpu_snapshots_preserve_opaque_hdr_for_overlapping_slots_at_1x_and_4x() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    let view = fixture_view();
    let frame = FrameUniform::new(&view, None).unwrap();
    for samples in [1, 4] {
        let mut renderer = crate::compositor::FrameRenderer::hdr(
            &context,
            [32, 32],
            wgpu::TextureFormat::Rgba8UnormSrgb,
            samples,
            ToneMap::Reinhard,
            None,
        )
        .unwrap();
        renderer
            .install_water(context.device(), settings())
            .unwrap();
        let color = AttachmentView::new(
            ResourceRef::new("scene color"),
            renderer.targets().scene().texture(),
        )
        .unwrap();
        let depth = AttachmentView::new(
            ResourceRef::new("scene depth"),
            renderer.targets().depth().texture(),
        )
        .unwrap();
        let mut submissions = Vec::new();
        for (slot, red) in [(0, 0.25), (1, 0.5)] {
            renderer.select_frame_slot(slot);
            renderer
                .set_frame_time(if slot == 0 { 0.0 } else { 2.0 })
                .unwrap();
            let encoder = encode_fixture(&context, &mut renderer, (&color, &depth), frame, red);
            submissions.push(context.queue().submit([encoder.finish()]));
        }
        context
            .device()
            .poll(wgpu::PollType::wait_indefinitely())
            .unwrap();
        assert_snapshots(&context, &renderer);
        assert_surface_depth(&context, &renderer);
        assert_foreground_rejection(&context, &renderer);
        assert!(renderer.resize(context.device(), [0, 32]).is_err());
        assert_eq!(renderer.water.as_ref().unwrap().snapshots[0].size, [32, 32]);
        assert!(renderer.opaque_snapshot().is_some());
        assert!(renderer.post_water_depth().is_some());
        assert!(renderer.resize(context.device(), [16, 16]).unwrap());
        assert!(renderer.opaque_snapshot().is_none());
        assert!(renderer.post_water_depth().is_none());
        assert_eq!(renderer.water.as_ref().unwrap().snapshots[0].size, [16, 16]);
        assert_eq!(submissions.len(), 2);
    }
}

fn encode_fixture(
    context: &GpuContext,
    renderer: &mut crate::compositor::FrameRenderer,
    attachments: (&AttachmentView, &AttachmentView),
    frame: FrameUniform,
    red: f64,
) -> wgpu::CommandEncoder {
    let (color, depth) = attachments;
    let mut pass = PassDeclaration::new(PassSlot::Opaque);
    let mut attachment = crate::compositor::attachment(color.resource(), color, false);
    attachment.load = AttachmentLoad::ClearColor(wgpu::Color {
        r: red,
        g: 0.125,
        b: 0.0625,
        a: 1.0,
    });
    pass.color = Some(attachment);
    pass.depth = Some(crate::compositor::attachment(depth.resource(), depth, true));
    let mut encoder = context
        .device()
        .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    renderer
        .encode_frame(
            context.device(),
            &mut encoder,
            (&[pass], &[color.clone(), depth.clone()]),
            &mut [],
            (frame, None),
        )
        .unwrap();
    renderer.targets().encode_resolve(&mut encoder);

    encoder
}

fn assert_snapshots(context: &GpuContext, renderer: &crate::compositor::FrameRenderer) {
    let water = renderer.water.as_ref().unwrap();
    for (slot, red_bits) in [(0, 0x3400u16), (1, 0x3800u16)] {
        let bytes = read(context, &water.snapshots[slot].color);
        let expected: Vec<_> = [red_bits, 0x3000, 0x2c00, 0x3c00]
            .into_iter()
            .flat_map(u16::to_le_bytes)
            .collect();
        assert!(bytes.chunks_exact(8).all(|pixel| pixel == expected));
        let depths = read(context, &water.snapshots[slot].depth);
        let far = f32::from_le_bytes(depths[0..4].try_into().unwrap());
        assert!((far - 100.0).abs() < 0.05, "far {far}");
    }
    assert_ne!(
        read(context, renderer.targets().texture()),
        read(context, &water.snapshots[1].color),
        "water changes the resolved frame, not the opaque snapshot"
    );
}

fn assert_surface_depth(context: &GpuContext, renderer: &crate::compositor::FrameRenderer) {
    let snapshot = &renderer.water.as_ref().unwrap().snapshots[1];
    let group = snapshot.source_group(
        context.device(),
        &AttachmentView::new(
            ResourceRef::new("scene depth"),
            renderer.targets().depth().texture(),
        )
        .unwrap(),
        true,
    );
    let texture =
        crate::ssao::passes::texture(context.device(), [32, 32], wgpu::TextureFormat::R32Float);
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let mut encoder = context
        .device()
        .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    worldrunner_render_post::fullscreen_pass(
        &mut encoder,
        Some("post-water depth probe"),
        wgpu::RenderPassColorAttachment {
            view: &view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                store: wgpu::StoreOp::Store,
            },
        },
        &snapshot.depth_pipeline,
        &[&group],
    );
    context.queue().submit([encoder.finish()]);
    context
        .device()
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    let bytes = read(context, &texture);
    let offset = (16 * 32 + 16) * 4;
    let depth = f32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
    assert!(
        (4.0..6.0).contains(&depth),
        "surface must replace the far depth: {depth}"
    );
}

fn fixture_view() -> SceneView {
    SceneView::new(
        CameraPose {
            eye_millimeters: [0, -3000, 4000],
            target_millimeters: [0, 0, 0],
        },
        SceneCamera {
            vertical_fov_millidegrees: 60000,
            near_plane_millimeters: 100,
            far_plane_millimeters: 100_000,
        },
        Viewport {
            width: 32,
            height: 32,
        },
    )
    .unwrap()
}

fn upload_snapshot(context: &GpuContext, texture: &wgpu::Texture, bytes: &[u8], pixel_bytes: u32) {
    context.queue().write_texture(
        texture.as_image_copy(),
        bytes,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(32 * pixel_bytes),
            rows_per_image: Some(32),
        },
        texture.size(),
    );
}
fn assert_foreground_rejection(context: &GpuContext, renderer: &crate::compositor::FrameRenderer) {
    let water = renderer.water.as_ref().unwrap();
    let snapshot = &water.snapshots[1];
    let center = 16 * 32 + 16;
    let colors: Vec<u8> = (0..32 * 32)
        .flat_map(|pixel| {
            if pixel == center {
                [0x3c00u16, 0, 0, 0x3c00]
            } else {
                [0, 0, 0x3c00, 0x3c00]
            }
        })
        .flat_map(u16::to_le_bytes)
        .collect();
    upload_snapshot(context, &snapshot.color, &colors, 8);
    for foreground in [false, true] {
        let depths: Vec<u8> = (0..32 * 32)
            .flat_map(|pixel| {
                if foreground && pixel != center {
                    1.0f32
                } else {
                    10.0f32
                }
                .to_le_bytes()
            })
            .collect();
        upload_snapshot(context, &snapshot.depth, &depths, 4);
        draw_water_only(context, renderer);
        let bytes = read(context, renderer.targets().texture());
        let pixel = &bytes[center * 8..center * 8 + 8];
        let red = u16::from_le_bytes(pixel[0..2].try_into().unwrap());
        let blue = u16::from_le_bytes(pixel[4..6].try_into().unwrap());
        if foreground {
            assert!(
                red > 0 && blue == 0,
                "foreground must use the red undistorted pixel"
            );
        } else {
            assert!(
                blue > 0 && red == 0,
                "background must use the blue refracted pixel"
            );
        }
    }
}
fn draw_water_only(context: &GpuContext, renderer: &crate::compositor::FrameRenderer) {
    draw_water_slot(context, renderer, 1);
}
fn draw_water_slot(context: &GpuContext, renderer: &crate::compositor::FrameRenderer, slot: usize) {
    let water = renderer.water.as_ref().unwrap();
    let feature = water.feature(
        context.device(),
        FrameUniform::new(&fixture_view(), None).unwrap(),
        (2.0, slot),
        None,
    );
    let mut encoder = context
        .device()
        .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("water refraction probe"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: renderer.targets().scene(),
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: renderer.targets().depth(),
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            ..wgpu::RenderPassDescriptor::default()
        });
        feature.draw(
            &worldrunner_render_post::DrawContext {
                slot: PassSlot::TransparentWater,
            },
            &mut pass,
        );
    }
    renderer.targets().encode_resolve(&mut encoder);
    context.queue().submit([encoder.finish()]);
    context
        .device()
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
}
