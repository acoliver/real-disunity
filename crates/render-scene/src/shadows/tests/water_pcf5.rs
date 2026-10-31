use super::*;

const PHASES: [f64; 16] = [
    0.0, 0.001, 0.0625, 0.125, 0.2, 0.25, 0.333, 0.499, 0.5, 0.501, 0.666, 0.75, 0.8, 0.875, 0.999,
    1.0,
];

#[test]
fn gpu_water_pcf5_matches_binary_map_oracle_across_phases_and_clamped_edges() {
    let context = context();
    let source = scene(&[], 600_000, None);
    let renderer = ShadowedScene::new(
        &context,
        &source,
        DirectionalShadowSettings {
            resolution: 256,
            ..settings()
        },
    )
    .unwrap();
    let depth_pipeline = binary_pipeline(context.device(), false);
    for origin in [0.5, 128.5, 254.5] {
        let image = binary_probe(&context, &renderer, &depth_pipeline, origin);
        for (index, pixel) in image.pixels.chunks_exact(4).enumerate() {
            let expected = oracle(origin + PHASES[index % 16], origin + PHASES[index / 16]);
            let actual = crate::lit_model_tests::srgb_decode(pixel[0]);
            // Two 8-bit hardware interpolation fractions contribute at most 2/256;
            // the sRGB8 readback contributes less than 0.004 in this fixture.
            assert!(
                (actual - expected).abs() < 0.012,
                "origin={origin}, pixel={index}: {actual} vs {expected}"
            );
        }
    }
}

fn coefficients(q: f64) -> (i32, [f64; 6]) {
    let anchor = (q + 0.5).floor();
    let phase = q - anchor + 0.5;
    let index = anchor.to_string().parse::<i32>().unwrap() - 3;
    (
        index,
        [
            1.0 - phase,
            3.0 - 2.0 * phase,
            4.0 - phase,
            3.0 + phase,
            1.0 + 2.0 * phase,
            phase,
        ],
    )
}

fn binary(x: i32, y: i32) -> f64 {
    let x = x.clamp(0, 255);
    let y = y.clamp(0, 255);
    f64::from(u8::from((x + 3 * y + x * y) % 7 >= 3))
}

pub(super) fn oracle(x: f64, y: f64) -> f64 {
    let (start_x, weights_x) = coefficients(x);
    let (start_y, weights_y) = coefficients(y);
    weights_y
        .into_iter()
        .zip(0..6)
        .map(|(wy, y)| {
            weights_x
                .into_iter()
                .zip(0..6)
                .map(|(wx, x)| wx * wy * binary(start_x + x, start_y + y))
                .sum::<f64>()
        })
        .sum::<f64>()
        / 144.0
}

fn binary_source(origin: f64) -> String {
    let phases = PHASES.map(|value| format!("{value:.6}")).join(",");
    let vertex = super::selection::PROBE.split("@fragment").next().unwrap();
    format!(
        "{}\n{}\n{vertex}\n{}",
        include_str!("../../shader/shadows.wgsl"),
        worldrunner_render_shadow::shadow_wgsl!(),
        format_args!(
            r"
{}
@fragment fn fs_probe(@builtin(position) pixel: vec4<f32>) -> @location(0) vec4<f32> {{
 let phases = array<f32, 16>({phases});
 let uv = (vec2<f32>({origin:.6}) + vec2<f32>(phases[u32(pixel.x)], phases[u32(pixel.y)])) / 256.0;
 let visibility = rd_shadow_array_weighted_pcf5(shadow_depth, shadow_sampler, uv, 0, 0.5);
 return vec4<f32>(vec3<f32>(visibility), 1.0);
}}",
            worldrunner_render_shadow::WEIGHTED_PCF5_WGSL
        )
    )
}

pub(super) fn binary_pipeline(device: &wgpu::Device, equality: bool) -> wgpu::RenderPipeline {
    let vertex = super::selection::PROBE.split("@fragment").next().unwrap();
    let depth = if equality {
        "0.5"
    } else {
        "select(0.25, 0.75, (x + 3u * y + x * y) % 7u >= 3u)"
    };
    let source = format!(
        r"{vertex}
@fragment fn fs_depth(@builtin(position) p: vec4<f32>) -> @builtin(frag_depth) f32 {{
 let x = u32(p.x); let y = u32(p.y);
 return {depth};
}}"
    );
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("binary comparison oracle"),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: None,
        layout: None,
        vertex: wgpu::VertexState {
            module: &module,
            entry_point: Some("vs_probe"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module: &module,
            entry_point: Some("fs_depth"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::Always),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}

pub(super) fn binary_probe(
    context: &GpuContext,
    renderer: &ShadowedScene,
    depth_pipeline: &wgpu::RenderPipeline,
    origin: f64,
) -> RgbaImage {
    let pipeline = super::selection::probe_pipeline(
        context,
        &renderer.shadow_layout,
        &binary_source(origin),
        None,
    );
    let mut target = OffscreenTarget::new(
        context.device(),
        crate::Viewport {
            width: 16,
            height: 16,
        },
    )
    .unwrap();
    let mut encoder = context.submission().begin(context.device()).unwrap();
    {
        let mut pass = worldrunner_render_shadow::begin_depth_pass(
            &mut encoder,
            renderer.map.depth.layer_view(0).unwrap(),
            Some("binary shadow map"),
        );
        pass.set_pipeline(depth_pipeline);
        pass.draw(0..3, 0..1);
    }
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
        pass.set_bind_group(2, &renderer.map.group, &[]);
        pass.draw(0..3, 0..1);
    }
    target.encode_readback(&mut encoder).unwrap();
    let submission = encoder.finish().submit().unwrap();
    target.read_encoded(context, submission).unwrap()
}

#[test]
fn gpu_water_pcf5_preserves_cascade_selection_coverage_and_boundary_blends() {
    let context = context();
    let source = super::cascades::source_with(
        &scene(&[], 600_000, None),
        SceneShadowCascades::camera_splits(vec![2_000, 4_000, 6_000, 8_000], 250_000).unwrap(),
    );
    let renderer = ShadowedScene::new(&context, &source, settings()).unwrap();
    let probe = super::selection::PROBE.replace(
        "sunlight_visibility(",
        "water_filtered_raw_sunlight_visibility(",
    );
    let filtered = format!(
        "{}\n{}\n{probe}",
        resources::SHADOWED_WGSL,
        crate::water::shadow_filter::receiver_source()
    );
    let baseline = format!("{}{}", resources::SHADOWED_WGSL, super::selection::PROBE);
    for camera in [false, true] {
        for count in 1..=4 {
            for blend in [0.0, 0.25] {
                super::selection::upload_probe(&context, &renderer, camera, count, blend);
                // Match the clamped-edge reference policy. Grid PCF instead makes
                // out-of-map taps lit even while the receiver remains covered.
                context
                    .queue()
                    .write_buffer(&renderer.map.uniform, 264, &(-1.0f32).to_le_bytes());
                let expected = super::selection::render_probe(&context, &renderer, &baseline);
                let actual = super::selection::render_probe(&context, &renderer, &filtered);
                assert_eq!(
                    actual.pixels, expected.pixels,
                    "camera={camera}, count={count}, blend={blend}"
                );
            }
        }
    }
}

#[test]
fn gpu_water_pcf5_depth_equality_is_lit_with_less_equal() {
    let context = context();
    let source = scene(&[], 600_000, None);
    let renderer = ShadowedScene::new(
        &context,
        &source,
        DirectionalShadowSettings {
            resolution: 256,
            ..settings()
        },
    )
    .unwrap();
    let pipeline = binary_pipeline(context.device(), true);
    let image = binary_probe(&context, &renderer, &pipeline, 128.5);
    assert!(image.pixels.chunks_exact(4).all(|pixel| pixel == [255; 4]));
}
