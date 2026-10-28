use super::*;
use worldrunner_render_plan::TextureHandle;

#[test]
fn screen_refraction_range_and_coordinate_system_validate_before_allocation() {
    let mut body = settings();
    for amplitude in [0, 50_000, 1_000_000] {
        body.material.screen_refraction_micro = Some(amplitude);
        assert!(body.validate().is_ok());
    }
    body.material.screen_refraction_micro = Some(1_000_001);
    assert!(body.validate().is_err());
    body.material.screen_refraction_micro = Some(50_000);
    body.material.refraction_millimeters = Some(0);
    assert!(body.validate().is_err());
    body.material.refraction_millimeters = None;
    let encoded = optics::uniforms(&body);
    assert_eq!(&encoded[14..16], &[0.05, 1.0]);
    body.material.screen_refraction_micro = None;
    assert_eq!(optics::uniforms(&body).map(f32::to_bits), [0; 28]);
}

fn install_normal(
    context: &GpuContext,
    renderer: &mut crate::compositor::FrameRenderer,
    encoded: [u8; 2],
) {
    let texture = context.device().create_texture(&wgpu::TextureDescriptor {
        label: Some("screen displacement normal"),
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
        texture.as_image_copy(),
        &encoded,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(2),
            rows_per_image: Some(1),
        },
        texture.size(),
    );
    renderer.water.as_mut().unwrap().surface_inputs[0][0] =
        Some(texture.create_view(&wgpu::TextureViewDescriptor::default()));
}

fn screen_renderer(
    context: &GpuContext,
    samples: u32,
    amplitude: u32,
    encoded: [u8; 2],
) -> crate::compositor::FrameRenderer {
    let mut body = settings();
    body.color_micro = [0; 3];
    body.absorption_micro = [0; 3];
    body.material.normal = Some(TextureHandle::from_raw(0));
    body.material.screen_refraction_micro = Some(amplitude);
    let mut renderer = regressions::renderer(context, samples);
    renderer.install_water(context.device(), body).unwrap();
    install_normal(context, &mut renderer, encoded);
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
    encode_fixture(context, &mut renderer, (&color, &depth), &frame, 0.0)
        .finish()
        .submit()
        .unwrap();
    renderer
}

fn patterned_snapshot() -> Vec<u8> {
    (0..1024u16)
        .flat_map(|pixel| {
            [
                0x3000 + pixel % 32 * 32,
                0x3000 + pixel / 32 * 32,
                0,
                0x3c00,
            ]
        })
        .flat_map(u16::to_le_bytes)
        .collect()
}

fn expected_color(point: [u16; 2], encoded: [u8; 2], amplitude: u32, colors: &[u8]) -> [f64; 3] {
    let signed = encoded.map(|v| f64::from(v) / 255.0 * 2.0 - 1.0);
    let vertical = (1.0 - signed[0] * signed[0] - signed[1] * signed[1]).sqrt();
    let length = (signed[0] * signed[0] + signed[1] * signed[1] + vertical * vertical).sqrt();
    let coordinate = std::array::from_fn::<_, 2, _>(|axis| {
        (f64::from(point[axis]) + signed[axis] / length * f64::from(amplitude) / 1_000_000.0 * 32.0)
            .clamp(0.0, 31.0)
    });
    let base = coordinate.map(|v| v.floor().to_string().parse::<usize>().unwrap());
    let fraction = coordinate.map(f64::fract);
    std::array::from_fn(|channel| {
        let texel = |x: usize, y: usize| {
            let index = (y.min(31) * 32 + x.min(31)) * 8 + channel * 2;
            f64::from(numerical::positive_half(&colors[index..index + 2]))
        };
        let first = texel(base[0], base[1]) * (1.0 - fraction[0])
            + texel(base[0] + 1, base[1]) * fraction[0];
        let second = texel(base[0], base[1] + 1) * (1.0 - fraction[0])
            + texel(base[0] + 1, base[1] + 1) * fraction[0];
        first * (1.0 - fraction[1]) + second * fraction[1]
    })
}

#[test]
fn screen_refraction_matches_cpu_pixels_and_zero_identity_at_every_supported_sample_count() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    let colors = patterned_snapshot();
    for samples in crate::msaa_tests::supported_samples(&context) {
        for encoded in [[204, 128], [51, 128], [128, 204], [128, 51]] {
            for amplitude in [0, 50_000, 250_000] {
                assert_displacement(&context, samples, encoded, amplitude, &colors);
            }
        }
    }
}

fn assert_displacement(
    context: &GpuContext,
    samples: u32,
    encoded: [u8; 2],
    amplitude: u32,
    colors: &[u8],
) {
    let renderer = screen_renderer(context, samples, amplitude, encoded);
    let snapshot = &renderer.water.as_ref().unwrap().snapshots[0];
    upload_snapshot(context, &snapshot.color, colors, 8);
    let depths: Vec<_> = (0..1024).flat_map(|_| 50.0f32.to_le_bytes()).collect();
    upload_snapshot(context, &snapshot.depth, &depths, 4);
    draw_water_slot(context, &renderer, 0);
    let output = read(context, renderer.targets().texture());
    for x in [1u16, 16, 30] {
        let actual = (16 * 32 + usize::from(x)) * 8;
        let expected = expected_color([x, 16], encoded, amplitude, colors);
        assert_color(&output[actual..actual + 8], expected);
        if amplitude == 0 {
            assert_eq!(&output[actual..actual + 8], &colors[actual..actual + 8]);
        }
    }
}

#[test]
fn screen_refraction_retains_foreground_rejection_at_every_supported_sample_count() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    for samples in crate::msaa_tests::supported_samples(&context) {
        let renderer = screen_renderer(&context, samples, 250_000, [204, 128]);
        let snapshot = &renderer.water.as_ref().unwrap().snapshots[0];
        let mut colors = patterned_snapshot();
        colors[(16 * 32 + 16) * 8..(16 * 32 + 16) * 8 + 8]
            .copy_from_slice(&[0, 0x3c, 0, 0, 0, 0, 0, 0x3c]);
        upload_snapshot(&context, &snapshot.color, &colors, 8);
        for foreground in [None, Some(17 * 32 + 21)] {
            let depths = foreground_depths(foreground);
            upload_snapshot(&context, &snapshot.depth, &depths, 4);
            draw_water_slot(&context, &renderer, 0);
            let output = read(&context, renderer.targets().texture());
            let center = (16 * 32 + 16) * 8;
            assert_eq!(&output[center..center + 8], &colors[center..center + 8]);
        }
    }
}

#[test]
fn screen_refraction_preserves_each_sample_receiver_ownership() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    for samples in crate::msaa_tests::supported_samples(&context) {
        let unchanged = screen_renderer(&context, samples, 0, [204, 128]);
        let displaced = screen_renderer(&context, samples, 250_000, [204, 128]);
        let before = &unchanged.water.as_ref().unwrap().receivers[0];
        let after = &displaced.water.as_ref().unwrap().receivers[0];
        for sample in 0..samples {
            for index in 0..3 {
                assert_eq!(
                    lighting_tests::environment_tests::sample_read(
                        &context,
                        &before[index],
                        sample
                    ),
                    lighting_tests::environment_tests::sample_read(&context, &after[index], sample),
                    "receiver {index}, sample {sample} at {samples}x"
                );
            }
        }
    }
}

fn assert_color(actual: &[u8], expected: [f64; 3]) {
    for (channel, expected) in expected.into_iter().enumerate() {
        let bytes = &actual[channel * 2..channel * 2 + 2];
        let bits = u16::from_le_bytes(bytes.try_into().unwrap());
        let value = numerical::positive_half(bytes);
        let next = numerical::positive_half(&(bits + 1).to_le_bytes());
        // One half-float ULP permits render-target conversion; float arithmetic
        // contributes the usual gamma_n bound for the interpolation operations.
        let rounding = 32.0 * f64::from(f32::EPSILON);
        let bound = f64::from(next - value) + rounding / (1.0 - rounding) * expected;
        assert!(
            (f64::from(value) - expected).abs() <= bound,
            "{value} vs {expected}, bound {bound}"
        );
    }
    assert_eq!(&actual[6..8], &0x3c00u16.to_le_bytes());
}

fn foreground_depths(single_tap: Option<usize>) -> Vec<u8> {
    (0..1024)
        .flat_map(|pixel| {
            let foreground = single_tap.map_or(pixel != 16 * 32 + 16, |tap| pixel == tap);
            if foreground { 1.0f32 } else { 50.0f32 }.to_le_bytes()
        })
        .collect()
}
