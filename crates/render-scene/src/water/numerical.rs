use super::*;
use worldrunner_render_post::{BorrowedFeature, DrawContext, PrepareContext};

struct PerSample(wgpu::RenderPipeline);
impl PreparedFeature for PerSample {
    fn prepare(&mut self, _: &mut PrepareContext<'_>) -> Result<(), FrameError> {
        Ok(())
    }
    fn draw(&self, _: &DrawContext, pass: &mut wgpu::RenderPass<'_>) {
        pass.set_pipeline(&self.0);
        pass.draw(0..3, 0..1);
    }
}
fn per_sample(context: &GpuContext) -> PerSample {
    let shader = context
        .device()
        .create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("distinct covered samples"),
            source: wgpu::ShaderSource::Wgsl(
                r"
@vertex fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let p = array<vec2<f32>, 3>(vec2<f32>(-1.0, -1.0), vec2<f32>(3.0, -1.0), vec2<f32>(-1.0, 3.0));
    return vec4<f32>(p[i], 0.0, 1.0);
}
struct Output { @location(0) color: vec4<f32>, @builtin(frag_depth) depth: f32, }
@fragment fn fs(@builtin(sample_index) sample: u32) -> Output {
    let reds = array<f32, 4>(0.125, 0.25, 0.5, 0.625);
    let depths = array<f32, 4>(0.9, 0.75, 0.5, 0.875);
    return Output(vec4<f32>(reds[sample], 0.125, 0.0625, 1.0), depths[sample]);
}"
                .into(),
            ),
        });
    PerSample(
        context
            .device()
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("distinct covered samples"),
                layout: None,
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    buffers: &[],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: wgpu::TextureFormat::Rgba16Float,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: wgpu::TextureFormat::Depth32Float,
                    depth_write_enabled: Some(true),
                    depth_compare: Some(wgpu::CompareFunction::Always),
                    stencil: wgpu::StencilState::default(),
                    bias: wgpu::DepthBiasState::default(),
                }),
                multisample: wgpu::MultisampleState {
                    count: 4,
                    ..Default::default()
                },
                multiview_mask: None,
                cache: None,
            }),
    )
}
#[test]
fn production_snapshots_average_distinct_samples_and_select_nearest_depth() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    let mut renderer = regressions::renderer(&context, 4);
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
    let source = regressions::opaque(Some(&color), Some(&depth), 0.0, 1.0);
    let (output, declaration) = regressions::consumer(
        &source,
        &[
            "opaque snapshot color",
            "opaque snapshot depth",
            "post-water depth",
        ],
        4,
    );
    let mut geometry = per_sample(&context);
    let mut probe = regressions::Probe(0);
    let mut geometry_declaration = FeatureDeclaration::new(PassSlot::Opaque);
    geometry_declaration.passes[0].samples = 4;
    geometry_declaration.passes[0].color_format = Some(wgpu::TextureFormat::Rgba16Float);
    geometry_declaration.passes[0].depth_format = Some(wgpu::TextureFormat::Depth32Float);
    let frame = FrameUniform::new(&fixture_view(), None).unwrap();
    let mut encoder = context.submission().begin(context.device()).unwrap();
    renderer
        .encode_frame(
            context.device(),
            &mut encoder,
            (&[source, output], &[color, depth]),
            &mut [
                BorrowedFeature {
                    declaration: geometry_declaration,
                    prepared: &mut geometry,
                },
                BorrowedFeature {
                    declaration,
                    prepared: &mut probe,
                },
            ],
            (frame, None),
        )
        .unwrap();
    encoder.finish().submit().unwrap();
    assert_sample_reduction(&context, &renderer, &frame);
}
fn assert_sample_reduction(
    context: &GpuContext,
    renderer: &crate::compositor::FrameRenderer,
    frame: &FrameUniform,
) {
    let (color, depth) = renderer.opaque_snapshot().unwrap();
    assert_eq!(
        read(context, color.texture()),
        contracts::bytes_for_color(0x3600)
    );
    let bytes = read(context, depth.texture());
    let actual = f32::from_le_bytes(bytes[0..4].try_into().unwrap());
    let world = frame.view_projection.inverse() * glam::Vec4::new(-0.96875, 0.96875, 0.5, 1.0);
    let expected = -(frame.view * (world / world.w)).z;
    assert!((actual - expected).abs() < 0.0001);
    assert_eq!(
        read(context, renderer.post_water_depth().unwrap().texture()),
        bytes
    );
}

pub(super) fn positive_half(bytes: &[u8]) -> f32 {
    let bits = u16::from_le_bytes(bytes.try_into().unwrap());
    assert_eq!(bits & 0x8000, 0);
    let exponent = i32::from((bits >> 10) & 31);
    let fraction = f32::from(bits & 1023) / 1024.0;
    if exponent == 0 {
        fraction * 2.0f32.powi(-14)
    } else {
        (1.0 + fraction) * 2.0f32.powi(exponent - 15)
    }
}

fn half_ulp(value: f32) -> f32 {
    let exponent = i32::try_from(value.abs().to_bits() >> 23).unwrap() - 127;
    2.0f32.powi((exponent - 10).max(-24))
}

#[test]
fn half_ulp_matches_adjacent_output_values() {
    for bits in [0u16, 1, 0x0400, 0x3400, 0x361d, 0x3800, 0x3c00] {
        let value = positive_half(&bits.to_le_bytes());
        let next = positive_half(&(bits + 1).to_le_bytes());
        assert_eq!(half_ulp(value).to_bits(), (next - value).to_bits());
    }
    assert!(half_ulp(0.382_075_43) > 0.0002);
}

fn absorption_error_bound(expected: f32, background: f32, scattering: f32) -> f32 {
    // Allow one output ULP (including truncating conversions), plus rounding
    // through depth, exp, scattering, composition and the multisample resolve.
    let rounding = 16.0 * f32::EPSILON;
    half_ulp(expected) + rounding / (1.0 - rounding) * (background.abs() + scattering.abs())
}

#[test]
fn production_absorption_matches_cpu_with_nonzero_scattering_at_one_and_four_samples() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    for samples in [1, 4] {
        let mut renderer = regressions::renderer(&context, samples);
        let mut settings = settings();
        settings.roughness_micro = 1_000_000;
        settings.absorption_micro = [3000, 1000, 500];
        renderer
            .install_water(context.device(), settings.clone())
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
        let mut frame = FrameUniform::new(&fixture_view(), None).unwrap();
        frame.ambient_sky = glam::Vec3::new(0.5, 0.25, 0.125);
        // Isolate absorption from the procedural normal's hemisphere weighting.
        frame.ambient_ground = frame.ambient_sky;
        frame.sun_irradiance = glam::Vec3::ZERO;
        let encoder = encode_fixture(&context, &mut renderer, (&color, &depth), &frame, 0.5);
        encoder.finish().submit().unwrap();
        let center = 16 * 32 + 16;
        let surface = read(&context, renderer.post_water_depth().unwrap().texture());
        let surface_depth =
            f32::from_le_bytes(surface[center * 4..center * 4 + 4].try_into().unwrap());
        let snapshots = renderer.snapshots.as_ref().unwrap();
        let opaque = read(&context, &snapshots[0].depth);
        let opaque_depth =
            f32::from_le_bytes(opaque[center * 4..center * 4 + 4].try_into().unwrap());
        let output = read(&context, renderer.targets().texture());
        for (channel, (background, ambient)) in [(0.5, 0.5), (0.125, 0.25), (0.0625, 0.125)]
            .into_iter()
            .enumerate()
        {
            let transmission = transmittance(
                crate::view::meters_scalar(settings.absorption_micro[channel]) / 1000.0,
                opaque_depth - surface_depth,
            );
            let scattering =
                crate::view::meters_scalar(settings.color_micro[channel]) / 1000.0 * ambient;
            let expected = background * transmission + scattering * (1.0 - transmission);
            let offset = center * 8 + channel * 2;
            let actual = positive_half(&output[offset..offset + 2]);
            let bound = absorption_error_bound(expected, background, scattering);
            assert!(
                (actual - expected).abs() <= bound,
                "channel {channel} at {samples}x: {actual} versus {expected}, bound {bound}"
            );
        }
    }
}

#[test]
fn animation_changes_refraction_with_identical_scene_and_frame_slot() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    for samples in [1, 4] {
        let mut renderer = regressions::renderer(&context, samples);
        let mut settings = settings();
        settings.absorption_micro = [0; 3];
        settings.roughness_micro = 0;
        renderer
            .install_water(context.device(), settings.clone())
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
        let frame = FrameUniform::new(&fixture_view(), None).unwrap();
        let colors: Vec<u8> = (0..32u16 * 32)
            .flat_map(|pixel| {
                [
                    0x3000 + (pixel % 32) * 16,
                    0x3000 + (pixel / 32) * 16,
                    0,
                    0x3c00,
                ]
            })
            .flat_map(u16::to_le_bytes)
            .collect();
        let depths: Vec<u8> = (0..32 * 32).flat_map(|_| 10.0f32.to_le_bytes()).collect();
        let mut outputs = Vec::new();
        for time in [0.0, 2.0] {
            renderer.select_frame_slot(0);
            renderer.set_frame_time(time).unwrap();
            let encoder = encode_fixture(&context, &mut renderer, (&color, &depth), &frame, 0.5);
            encoder.finish().submit().unwrap();
            let snapshot = &renderer.snapshots.as_ref().unwrap()[0];
            upload_snapshot(&context, &snapshot.color, &colors, 8);
            upload_snapshot(&context, &snapshot.depth, &depths, 4);
            draw_water_slot(&context, &renderer, 0);
            outputs.push(read(&context, renderer.targets().texture()));
        }
        assert_ne!(outputs[0], outputs[1]);
    }
}
