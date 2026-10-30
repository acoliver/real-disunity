use super::*;

#[test]
fn forward_metadata_declares_the_replaceable_lobe() {
    let context = context();
    let mut screen = SceneMetadata::new(context.device(), [3, 3], neutral_ao()).unwrap();
    let ssr = Ssr::new(
        context.device(),
        FrameConfig::new([3, 3], wgpu::TextureFormat::Rgba16Float, 4),
        settings(),
        &mut screen,
    )
    .unwrap();
    assert_eq!(ssr.forward_products[0].sample_count(), 4);
    assert!(screen.geometry.forward.is_some());
}

#[test]
fn gpu_replacement_preserves_diffuse_and_interpolates_confidence() {
    let context = context();
    let source = format!(
        "{}\n@fragment fn fs_screen(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {{
        let confidence = array<f32, 3>(0.0, 0.5, 1.0)[u32(p.x)];
        let shaded = vec3<f32>(0.5, 0.375, 0.25);
        let environment = vec3<f32>(0.25, 0.125, 0.0625);
        let incident = vec3<f32>(1.0) * confidence;
        return vec4<f32>(shaded + reflection_delta(incident, confidence, vec3<f32>(0.5), environment, true), 1.0);
        }}",
        concat!(worldrunner_render_lighting::lighting_wgsl!(), include_str!("replacement.wgsl"))
    );
    let pipeline = gpu::pipeline(context.device(), &source, 1, false);
    let target =
        crate::ssao::passes::texture(context.device(), [3, 1], wgpu::TextureFormat::Rgba16Float);
    let mut encoder = context.submission().begin(context.device()).unwrap();
    worldrunner_render_post::fullscreen_pass(
        &mut encoder,
        Some("linear replacement energy"),
        wgpu::RenderPassColorAttachment {
            view: &target.create_view(&wgpu::TextureViewDescriptor::default()),
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                store: wgpu::StoreOp::Store,
            },
        },
        &pipeline,
        &[],
    );
    encoder.finish().submit().unwrap();
    // The shaded colour is stored unchanged; the delta is a few f32 multiplies and adds of
    // dyadic values, so each word is within the shared half store bound of the exact value
    // (range 1).
    let bytes = read(&context, &target);
    let expected = [
        0.5, 0.375, 0.25, 1.0, 0.625, 0.5625, 0.46875, 1.0, 0.75, 0.75, 0.6875, 1.0,
    ];
    for (index, want) in expected.into_iter().enumerate() {
        crate::tolerance_tests::assert_half_value(
            f64::from(
                ::half::f16::from_le_bytes([bytes[2 * index], bytes[2 * index + 1]]).to_f32(),
            ),
            want,
            1.0,
            &format!("replacement word {index}"),
        );
    }
}

#[test]
fn generated_resolve_source_calls_composition_helpers() {
    let source = gpu::resolve_source(3);
    assert!(source.contains("reflection_delta("));
    assert!(source.contains("reflection_factor("));
}
