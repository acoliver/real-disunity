use super::*;

fn renderer(
    context: &GpuContext,
    samples: u32,
    body: SceneWaterBody,
) -> crate::compositor::FrameRenderer {
    let mut renderer = crate::compositor::FrameRenderer::hdr(
        context,
        [32, 32],
        wgpu::TextureFormat::Rgba8UnormSrgb,
        samples,
        ToneMap::Reinhard,
        None,
    )
    .unwrap();
    renderer.install_water(context.device(), body).unwrap();
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
    renderer.select_frame_slot(1);
    let frame = FrameUniform::new(&fixture_view(), None).unwrap();
    let encoder = encode_fixture(context, &mut renderer, (&color, &depth), &frame, 0.25);
    encoder.finish().submit().unwrap();
    context
        .device()
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    renderer
}

#[test]
fn authored_refraction_keeps_foreground_rejection_at_1x_and_4x() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    for samples in [1, 4] {
        let mut body = settings();
        body.material.refraction_millimeters = Some(4000);
        let renderer = renderer(&context, samples, body);
        assert_foreground_rejection(&context, &renderer);
    }
}

#[test]
fn zero_bound_and_zero_opacity_preserve_original_snapshot_at_1x_and_4x() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    for samples in [1, 4] {
        for transparent in [false, true] {
            let mut body = settings();
            body.color_micro = [0; 3];
            body.absorption_micro = [0; 3];
            body.material.refraction_millimeters = Some(if transparent { 4000 } else { 0 });
            if transparent {
                body.material.shoreline = Some(worldrunner_render_plan::SceneWaterShoreline {
                    depth_millimeters: 1000,
                    normal_gain_micro: 1_000_000,
                    opacity_micro: 0,
                });
            }
            let renderer = renderer(&context, samples, body);
            let snapshot = &renderer.water.as_ref().unwrap().snapshots[1];
            let colors: Vec<_> = (0..32 * 32)
                .flat_map(|p| {
                    [
                        if p % 2 == 0 { 0x3400u16 } else { 0x3800 },
                        0x3000,
                        0x2c00,
                        0x3c00,
                    ]
                })
                .flat_map(u16::to_le_bytes)
                .collect();
            upload_snapshot(&context, &snapshot.color, &colors, 8);
            let depths: Vec<_> = (0..32 * 32).flat_map(|_| 10.0f32.to_le_bytes()).collect();
            upload_snapshot(&context, &snapshot.depth, &depths, 4);
            draw_water_only(&context, &renderer);
            let actual = read(&context, renderer.targets().texture());
            let center = (16 * 32 + 16) * 8;
            assert_eq!(&actual[center..center + 8], &colors[center..center + 8]);
        }
    }
}

#[test]
fn authored_optics_publish_fresnel_and_squared_specular_for_ssr() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    for samples in [1, 4] {
        let mut body = settings();
        body.material.optics = Some(worldrunner_render_plan::SceneWaterOptics {
            fresnel_micro: 250_000,
            specular_micro: 500_000,
            scattering_micro: [20_000, 30_000, 30_000],
            tint: None,
        });
        let renderer = renderer(&context, samples, body);
        let water = renderer.water.as_ref().unwrap();
        for sample in 0..samples {
            let material = lighting_tests::environment_tests::sample_read(
                &context,
                &water.receivers[1][1],
                sample,
            );
            let environment = lighting_tests::environment_tests::sample_read(
                &context,
                &water.receivers[1][2],
                sample,
            );
            let base = (16 * 32 + 16) * 8;
            assert_eq!(
                numerical::positive_half(&material[base + 4..base + 6]).to_bits(),
                0.25f32.to_bits()
            );
            assert_eq!(
                numerical::positive_half(&environment[base + 6..base + 8]).to_bits(),
                0.25f32.to_bits()
            );
        }
    }
}

#[test]
fn view_scattering_is_additive_even_without_absorption() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    for samples in [1, 4] {
        let mut body = settings();
        body.color_micro = [0; 3];
        body.absorption_micro = [0; 3];
        body.material.refraction_millimeters = Some(0);
        body.material.optics = Some(worldrunner_render_plan::SceneWaterOptics {
            fresnel_micro: 20_370,
            specular_micro: 1_000_000,
            scattering_micro: [20_000, 30_000, 30_000],
            tint: None,
        });
        let renderer = renderer(&context, samples, body);
        let bytes = read(&context, renderer.targets().texture());
        let base = (16 * 32 + 16) * 8;
        let red = numerical::positive_half(&bytes[base..base + 2]);
        assert!((0.259..0.271).contains(&red), "additive scattering: {red}");
    }
}

#[test]
fn disabled_optical_controls_are_byte_identical_to_stock_shader_at_1x_and_4x() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    for samples in [1, 4] {
        let mut renderer = renderer(&context, samples, settings());
        let expected = read(&context, renderer.targets().texture());
        let source = water_source(false).replace(
            include_str!("water.wgsl"),
            include_str!("stock_reference.wgsl"),
        );
        let water = renderer.water.as_mut().unwrap();
        water.pipeline = pipeline_with_source(
            context.device(),
            samples,
            &water.layout,
            false,
            [0; 3],
            &source,
        );
        draw_water_only(&context, &renderer);
        assert_eq!(read(&context, renderer.targets().texture()), expected);
    }
}
