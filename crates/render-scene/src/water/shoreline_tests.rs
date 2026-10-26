use super::*;

#[test]
fn oblique_candidate_ray_classifies_both_depth_orderings() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    for samples in [1, 4] {
        for (green, foreground) in [(64u8, true), (192, false)] {
            let mut renderer = regressions::renderer(&context, samples);
            let mut body = settings();
            body.color_micro = [0; 3];
            body.absorption_micro = [0; 3];
            body.roughness_micro = 0;
            body.material.normal = Some(worldrunner_render_plan::TextureHandle::from_raw(0));
            renderer.install_water(context.device(), body).unwrap();
            install_normal(&context, &mut renderer, green);
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
            let frame = FrameUniform::new(&fixture_view(), None).unwrap();
            encode_fixture(&context, &mut renderer, (&color, &depth), &frame, 0.0)
                .finish()
                .submit()
                .unwrap();
            let snapshot = &renderer.water.as_ref().unwrap().snapshots[0];
            let center = 16 * 32 + 16;
            let colors: Vec<_> = (0..1024)
                .flat_map(|i| {
                    if i == center {
                        [0x3c00u16, 0, 0, 0x3c00]
                    } else {
                        [0, 0, 0x3c00, 0x3c00]
                    }
                })
                .flat_map(u16::to_le_bytes)
                .collect();
            upload_snapshot(&context, &snapshot.color, &colors, 8);
            let depths = shoreline_depths(&frame, foreground, center);
            upload_snapshot(&context, &snapshot.depth, &depths, 4);
            draw_water_slot(&context, &renderer, 0);
            let bytes = read(&context, renderer.targets().texture());
            let red = numerical::positive_half(&bytes[center * 8..center * 8 + 2]);
            let blue = numerical::positive_half(&bytes[center * 8 + 4..center * 8 + 6]);
            if foreground {
                assert!(
                    red > 0.9 && blue == 0.0,
                    "dry foreground leaked at {samples}x: {red} {blue}"
                );
            } else {
                assert!(
                    blue > 0.9 && red == 0.0,
                    "submerged candidate rejected at {samples}x: {red} {blue}"
                );
            }
        }
    }
}

fn install_normal(
    context: &GpuContext,
    renderer: &mut crate::compositor::FrameRenderer,
    green: u8,
) {
    let map = context.device().create_texture(&wgpu::TextureDescriptor {
        label: Some("shore normal"),
        size: wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rg8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    context.queue().write_texture(
        map.as_image_copy(),
        &[128, green],
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(2),
            rows_per_image: Some(1),
        },
        map.size(),
    );
    renderer.water.as_mut().unwrap().surface_inputs[0][0] =
        Some(map.create_view(&wgpu::TextureViewDescriptor::default()));
}

fn surface_depth(frame: &FrameUniform, inverse: glam::Mat4, x: f32, y: f32) -> f32 {
    let unproject = |z| {
        let point = inverse * glam::Vec4::new(x / 16.0 - 1.0, 1.0 - y / 16.0, z, 1.0);
        point.truncate() / point.w
    };
    let near_point = unproject(0.0);
    let far_point = unproject(1.0);
    let point =
        near_point + (far_point - near_point) * (-near_point.z / (far_point.z - near_point.z));
    -(frame.view * point.extend(1.0)).z
}

fn shoreline_depths(frame: &FrameUniform, foreground: bool, center: usize) -> Vec<u8> {
    let inverse = frame.view_projection.inverse();
    (0..1024)
        .flat_map(|i| {
            let x = f32::from(u16::try_from(i % 32).unwrap());
            let y = f32::from(u16::try_from(i / 32).unwrap());
            // Bound the entire pixel footprint, not just its centre: the oblique
            // plane varies by more than 0.03 between MSAA sample positions.
            let corners = [(x, y), (x + 1.0, y), (x, y + 1.0), (x + 1.0, y + 1.0)]
                .map(|(x, y)| surface_depth(frame, inverse, x, y));
            let depth = if i == center {
                50.0
            } else if foreground {
                corners.into_iter().fold(f32::INFINITY, f32::min) - 0.03
            } else {
                corners.into_iter().fold(f32::NEG_INFINITY, f32::max) + 0.03
            };
            depth.to_le_bytes()
        })
        .collect()
}

#[test]
fn shoreline_depth_ordering_covers_the_candidate_pixel_footprint() {
    let frame = FrameUniform::new(&fixture_view(), None).unwrap();
    let inverse = frame.view_projection.inverse();
    let original = surface_depth(&frame, inverse, 16.5, 16.5);
    for (foreground, y) in [(true, 5u16), (false, 28)] {
        let depths = shoreline_depths(&frame, foreground, 16 * 32 + 16);
        let pixel = usize::from(y) * 32 + 16;
        let depth = f32::from_le_bytes(depths[pixel * 4..pixel * 4 + 4].try_into().unwrap());
        assert_eq!(depth > original, foreground);
        for dy in [0.0, 0.125, 0.375, 0.5, 0.625, 0.875, 1.0] {
            let surface = surface_depth(&frame, inverse, 16.5, f32::from(y) + dy);
            assert_eq!(
                depth < surface,
                foreground,
                "candidate row {y}, offset {dy}"
            );
        }
    }
}
