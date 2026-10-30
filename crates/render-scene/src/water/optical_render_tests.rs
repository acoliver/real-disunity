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
    wait(context.device());
    renderer
}

#[test]
fn authored_refraction_keeps_foreground_rejection_at_every_supported_sample_count() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    for samples in crate::msaa_tests::supported_samples(&context) {
        let mut body = settings();
        body.material.refraction_millimeters = Some(4000);
        let renderer = renderer(&context, samples, body);
        assert_foreground_rejection(&context, &renderer);
    }
}

#[test]
fn zero_bound_and_zero_opacity_preserve_original_snapshot_at_every_supported_sample_count() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    for samples in crate::msaa_tests::supported_samples(&context) {
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
            let colors = upload_checker_snapshot(&context, &renderer);
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
    for samples in crate::msaa_tests::supported_samples(&context) {
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
            // The fresnel f0 and the squared specular 0.5 * 0.5 are f32 values stored as half,
            // so each is within the shared half store bound of 0.25 (range 1).
            crate::tolerance_tests::assert_half_value(
                f64::from(numerical::positive_half(&material[base + 4..base + 6])),
                0.25,
                1.0,
                "fresnel",
            );
            crate::tolerance_tests::assert_half_value(
                f64::from(numerical::positive_half(&environment[base + 6..base + 8])),
                0.25,
                1.0,
                "squared specular",
            );
        }
    }
}

#[test]
fn view_scattering_is_additive_even_without_absorption() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    for samples in crate::msaa_tests::supported_samples(&context) {
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
fn disabled_optical_controls_are_byte_identical_to_stock_shader_at_every_supported_sample_count() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    for samples in crate::msaa_tests::supported_samples(&context) {
        let mut renderer = renderer(&context, samples, settings());
        let expected = read(&context, renderer.targets().texture());
        let receivers = receiver_bytes(&context, &renderer, samples);
        assert_default_uniform_bytes();
        use_stock_shader(&context, &mut renderer, samples);
        draw_water_only(&context, &renderer);
        assert_images_agree(
            &read(&context, renderer.targets().texture()),
            &expected,
            "colour",
        );
        for (index, (actual, want)) in receiver_bytes(&context, &renderer, samples)
            .iter()
            .zip(&receivers)
            .enumerate()
        {
            assert_images_agree(actual, want, &format!("receiver {index}"));
        }
        assert_transparent_water_passes_snapshot(&context, samples);
    }
}

/// With colour 0 and absorption 0 the transmission is 1 and the scattering 0, so both shaders
/// must output the snapshot pixel itself. This absolute value does not depend on either shader
/// text agreeing with the other, so a change to the shared compose term cannot hide.
fn assert_transparent_water_passes_snapshot(context: &GpuContext, samples: u32) {
    let mut body = settings();
    body.color_micro = [0; 3];
    body.absorption_micro = [0; 3];
    body.material.refraction_millimeters = Some(0);
    let mut renderer = renderer(context, samples, body);
    upload_checker_snapshot(context, &renderer);
    let center = (16 * 32 + 16) * 8;
    for stock in [false, true] {
        if stock {
            use_stock_shader(context, &mut renderer, samples);
        }
        draw_water_only(context, &renderer);
        let actual = read(context, renderer.targets().texture());
        crate::tolerance_tests::assert_half_pixel(
            &actual[center..center + 8],
            [0.25, 0.125, 0.0625, 1.0],
            if stock {
                "stock shader"
            } else {
                "current shader"
            },
        );
    }
}

/// Fills the snapshot with a constant-depth checker whose even pixels (including the centre)
/// are (0.25, 0.125, 0.0625, 1). Returns the colour bytes.
fn upload_checker_snapshot(
    context: &GpuContext,
    renderer: &crate::compositor::FrameRenderer,
) -> Vec<u8> {
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
    upload_snapshot(context, &snapshot.color, &colors, 8);
    let depths: Vec<_> = (0..32 * 32).flat_map(|_| 10.0f32.to_le_bytes()).collect();
    upload_snapshot(context, &snapshot.depth, &depths, 4);
    colors
}

fn use_stock_shader(
    context: &GpuContext,
    renderer: &mut crate::compositor::FrameRenderer,
    samples: u32,
) {
    let source = water_source(false)
        .replace(
            include_str!("water.wgsl"),
            include_str!("stock_reference.wgsl"),
        )
        .replace(include_str!("terms.wgsl"), "");
    let water = renderer.water.as_mut().unwrap();
    water.pipeline = pipeline_with_source(
        context.device(),
        samples,
        &water.layout,
        false,
        [0; 4],
        &source,
    );
}

/// The two shaders are different texts that are equal for default controls, so their float16
/// outputs differ only by the arithmetic before the store and the store itself: every word of
/// both is within twice the derived half bound (one store each) of the other, for values up to
/// the largest word of the image.
fn assert_images_agree(actual: &[u8], expected: &[u8], context: &str) {
    assert_eq!(actual.len(), expected.len(), "{context}: lengths");
    let word = |bytes: &[u8], i: usize| {
        f64::from(::half::f16::from_le_bytes([bytes[2 * i], bytes[2 * i + 1]]).to_f32())
    };
    let words = expected.len() / 2;
    let range = (0..words)
        .map(|i| word(expected, i).abs())
        .fold(1.0, f64::max);
    for i in 0..words {
        let (a, e) = (word(actual, i), word(expected, i));
        let bound = 2.0 * worldrunner_render_gpu::tolerance_test_support::half_bound(e, range);
        assert!(
            (a - e).abs() <= bound,
            "{context}: word {i}, got {a}, want {e}, bound {bound}"
        );
    }
}

fn receiver_bytes(
    context: &GpuContext,
    renderer: &crate::compositor::FrameRenderer,
    samples: u32,
) -> Vec<Vec<u8>> {
    (0..samples)
        .flat_map(|sample| {
            renderer.water.as_ref().unwrap().receivers[1]
                .iter()
                .map(move |receiver| {
                    lighting_tests::environment_tests::sample_read(context, receiver, sample)
                })
        })
        .collect()
}

fn assert_default_uniform_bytes() {
    let frame = FrameUniform::new(&fixture_view(), None).unwrap();
    let data = uniform_data(&frame, 0.0, &settings());
    let bytes: Vec<_> = data.into_iter().flat_map(f32::to_le_bytes).collect();
    let mut stock = Vec::new();
    for matrix in [
        frame.view_projection,
        frame.view,
        frame.view_projection.inverse(),
    ] {
        stock.extend(matrix.to_cols_array());
    }
    stock.extend([
        -10.0, -10.0, 0.0, 0.0, 20.0, 20.0, 0.0, 0.0, 0.05, 0.2, 0.25, 0.2, 0.3, 0.1, 0.05, 0.0,
    ]);
    stock.extend([0.0; 12]);
    stock.extend([1.0, 1.0, 0.0, 0.0]);
    stock.extend([0.0; 28]);
    let stock: Vec<_> = stock.into_iter().flat_map(f32::to_le_bytes).collect();
    assert_eq!(stock.len(), 432);
    assert_eq!(&bytes[..432], stock);
    assert_eq!(&bytes[432..], &[0; 128]);
}
