use super::*;

fn draw(
    context: &GpuContext,
    target: &wgpu::Texture,
    pipeline: &wgpu::RenderPipeline,
    groups: &[&wgpu::BindGroup],
) {
    let mut encoder = context.submission().begin(context.device()).unwrap();
    worldrunner_render_post::fullscreen_pass(
        &mut encoder,
        None,
        wgpu::RenderPassColorAttachment {
            view: &target.create_view(&wgpu::TextureViewDescriptor::default()),
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                store: wgpu::StoreOp::Store,
            },
        },
        pipeline,
        groups,
    );
    encoder.finish().submit().unwrap();
}
fn ownership_source() -> String {
    let source = gpu::trace_source(4, true);
    let function = source
        .split("fn matches_receiver_depth(")
        .nth(1)
        .unwrap()
        .split("@fragment")
        .next()
        .unwrap();
    format!("struct ReflectionCamera {{ projection: mat4x4<f32>, inverse_projection: mat4x4<f32>, settings: vec4<f32>, size: vec4<f32> }}
        @group(0) @binding(0) var<uniform> camera: ReflectionCamera;
        @group(0) @binding(1) var opaque_forward_depth: texture_2d<f32>;
        @group(0) @binding(2) var water_normals: texture_multisampled_2d<f32>;
        fn matches_receiver_depth({function}
        @fragment fn fs_screen(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {{
            let p = vec2<i32>(position.xy);
            let surface = select(4.5, 4.25, p.x == 0);
            let normal = normalize(vec3<f32>(camera.settings.x, 0.0, 1.0));
            let owned = matches_receiver_depth(surface, 5.0, normal, p);
            return vec4<f32>(select(0.0, 1.0, owned), 0.0, 0.0, 1.0);
        }}")
}
#[test]
fn gpu_forward_ownership_rejects_inside_interval_foreground_and_partial_water_independent_of_shading_normal()
 {
    let context = context();
    let config = FrameConfig::new([3, 1], wgpu::TextureFormat::Rgba16Float, 4);
    let mut screen = SceneMetadata::new(context.device(), config.size, neutral_ao()).unwrap();
    let ssr = Ssr::new(context.device(), config, settings(), &mut screen).unwrap();
    let water = water_tests::water_fixture(&context, config);
    let mask = gpu::pipeline(
        context.device(),
        "@fragment fn fs_screen(@builtin(position) p: vec4<f32>, @builtin(sample_index) sample: u32) -> @location(0) vec4<f32> { return vec4<f32>(0.0,0.0,1.0,select(0.0,1.0,p.x >= 2.0 && sample == 3u)); }",
        4,
        false,
    );
    draw(&context, &water.receivers[0][0], &mask, &[]);
    let actual_depth =
        crate::ssao::passes::texture(context.device(), [3, 1], wgpu::TextureFormat::R32Float);
    upload(&context, &actual_depth, &[4.5; 3]);
    let views = [&actual_depth, &water.receivers[0][0]]
        .map(|t| t.create_view(&wgpu::TextureViewDescriptor::default()));
    let pipeline = gpu::pipeline(context.device(), &ownership_source(), 1, false);
    let group = gpu::group(context.device(), &pipeline, Some(&ssr.uniform), &views);
    for normal_x in [0.0, 1.0] {
        let mut values: Vec<_> = [frame().view_projection, frame().view_projection.inverse()]
            .into_iter()
            .flat_map(|m| m.to_cols_array())
            .collect();
        values.extend([normal_x, 0.0, 0.0, 0.0, 3.0, 1.0, 1.0, 0.0]);
        let mut encoder = context.submission().begin(context.device()).unwrap();
        let mut prepare = PrepareContext::new(context.device(), &mut encoder).unwrap();
        prepare.upload_buffer(
            &ssr.uniform,
            0,
            &values
                .into_iter()
                .flat_map(f32::to_le_bytes)
                .collect::<Vec<_>>(),
        );
        prepare.finish();
        encoder.finish().submit().unwrap();
        draw(&context, &ssr.material, &pipeline, &[&group]);
        let bytes = read(&context, &ssr.material);
        for (pixel, expected) in [0, half(1.0), 0].into_iter().enumerate() {
            assert_eq!(
                u16::from_le_bytes(bytes[pixel * 8..pixel * 8 + 2].try_into().unwrap()),
                expected
            );
        }
    }
}
