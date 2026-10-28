use super::*;

fn configuration(dimension: ReceiverDimension, layers: u32) -> DirectionalShadowConfig {
    DirectionalShadowConfig {
        coverage: if layers == 1 {
            DirectionalCoverage::Single
        } else {
            DirectionalCoverage::Cascades { count: layers }
        },
        dimension,
        resolution: 17,
        caster_compare: wgpu::CompareFunction::Less,
        receiver_compare: wgpu::CompareFunction::LessEqual,
        caster_bias: wgpu::DepthBiasState::default(),
        receiver_depth_bias: 0.001,
        receiver_normal_bias: 0.01,
        filter: ShadowFilter::Grid {
            radius: 1,
            receiver_plane: true,
        },
    }
}

#[test]
fn configuration_rejects_invalid_topology_bias_and_footprints() {
    let limits = wgpu::Limits::default();
    let config = configuration(ReceiverDimension::D2, 1);
    config.validate(&limits).unwrap();
    for invalid in [
        DirectionalShadowConfig {
            coverage: DirectionalCoverage::Cascades { count: 0 },
            ..config
        },
        DirectionalShadowConfig {
            coverage: DirectionalCoverage::Cascades { count: 4 },
            ..config
        },
        DirectionalShadowConfig {
            receiver_depth_bias: f32::NAN,
            ..config
        },
        DirectionalShadowConfig {
            receiver_normal_bias: -0.1,
            ..config
        },
        DirectionalShadowConfig {
            filter: ShadowFilter::FourTap {
                half_extent_texels: f32::INFINITY,
            },
            ..config
        },
        DirectionalShadowConfig {
            filter: ShadowFilter::Grid {
                radius: 5,
                receiver_plane: false,
            },
            ..config
        },
    ] {
        assert!(invalid.validate(&limits).is_err(), "{invalid:?}");
    }
    configuration(ReceiverDimension::D2Array, 4)
        .validate(&limits)
        .unwrap();
}

#[test]
fn generated_backends_agree_on_center_taps_grids_plane_correction_and_coverage() {
    let (device, queue) = device();
    for (abi, compare, layers) in super::receiver::variants() {
        let map = DepthMap::new(&device, descriptor(layers, abi.dimension)).unwrap();
        let output = DepthMap::new(&device, descriptor(1, ReceiverDimension::D2)).unwrap();
        let config = DirectionalShadowConfig {
            receiver_compare: compare,
            ..configuration(abi.dimension, layers)
        };
        config.validate(&device.limits()).unwrap();
        let group =
            super::receiver::receiver_group(&device, &queue, &map, abi, config.receiver_compare);
        for (call, expected) in sample_cases(abi.dimension, compare) {
            let (texture, slots) = if abi.dimension == ReceiverDimension::D2 {
                ("texture_depth_2d", 5)
            } else {
                ("texture_depth_2d_array", 20)
            };
            let pipeline =
                super::receiver::receiver_pipeline_with_sample(&device, abi, texture, &call, slots);
            let mut encoder =
                device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
            clear_layers(&map, &mut encoder);
            clear_depth(&mut encoder, map.layer_view(layers - 1).unwrap(), 0.25);
            {
                let mut pass = output.begin_layer_pass(&mut encoder, 0, None).unwrap();
                pass.set_pipeline(&pipeline);
                pass.set_bind_group(0, &group, &[]);
                pass.draw(0..3, 0..1);
            }
            let copy = output
                .copy_layer_to_readback(&device, &mut encoder, 0)
                .unwrap();
            queue.submit([encoder.finish()]);
            let actual = depths(&device, &copy)[8 * 17 + 8];
            assert!(
                (actual - expected).abs() < 1e-5,
                "{call}: {actual} != {expected}"
            );
        }
    }
}

fn sample_cases(
    dimension: ReceiverDimension,
    compare: wgpu::CompareFunction,
) -> Vec<(String, f32)> {
    let (prefix, layer) = if dimension == ReceiverDimension::D2 {
        ("rd_shadow_2d", "")
    } else {
        ("rd_shadow_array", ", 3")
    };
    let equal = if compare == wgpu::CompareFunction::LessEqual {
        1.0
    } else {
        0.0
    };
    vec![
        (
            format!(
                "{prefix}_bilinear(map, vec2(8.75 / 17.0, 8.5 / 17.0){layer}, 0.25, vec2(17.0, 0.0))"
            ),
            0.75,
        ),
        (
            format!("{prefix}_bilinear(map, vec2(0.5){layer}, 0.25, vec2(0.0))"),
            0.0,
        ),
        (
            format!("{prefix}_bilinear(map, vec2(0.5){layer}, 0.24, vec2(0.0))"),
            1.0,
        ),
        (
            format!("{prefix}_compare(map, cmp, vec2(0.5){layer}, 0.25)"),
            equal,
        ),
        (
            format!("dot({prefix}_taps(map, cmp, vec2(0.5){layer}, 0.5, 0.5), vec4(0.25))"),
            0.0,
        ),
        (
            format!("dot({prefix}_taps(map, cmp, vec2(0.5){layer}, 0.1, 1.5), vec4(0.25))"),
            1.0,
        ),
        (
            format!("{prefix}_grid(map, cmp, vec2(0.0){layer}, 0.5, vec2(0.0), 1, true)"),
            5.0 / 9.0,
        ),
        (
            format!("{prefix}_grid(map, cmp, vec2(0.0){layer}, 0.5, vec2(0.0), 2, true)"),
            16.0 / 25.0,
        ),
        (
            format!("{prefix}_grid(map, cmp, vec2(0.0){layer}, 0.5, vec2(0.0), 4, true)"),
            56.0 / 81.0,
        ),
        (
            format!("{prefix}_grid(map, cmp, vec2(0.0){layer}, 0.5, vec2(0.0), 4, false)"),
            0.0,
        ),
        (
            format!("{prefix}_grid(map, cmp, vec2(0.5){layer}, 0.25, vec2(17.0, 0.0), 1, false)"),
            (1.0 + equal) / 3.0,
        ),
    ]
}

fn clear_depth(encoder: &mut wgpu::CommandEncoder, view: &wgpu::TextureView, depth: f32) {
    let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        color_attachments: &[],
        depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
            view,
            depth_ops: Some(wgpu::Operations {
                load: wgpu::LoadOp::Clear(depth),
                store: wgpu::StoreOp::Store,
            }),
            stencil_ops: None,
        }),
        ..Default::default()
    });
}

#[test]
fn bilinear_edge_uses_only_adjacent_depth_texels_with_fractional_weights() {
    let (device, queue) = device();
    for (abi, compare, layers) in super::receiver::variants() {
        let map = DepthMap::new(&device, descriptor(layers, abi.dimension)).unwrap();
        let output = DepthMap::new(&device, descriptor(1, ReceiverDimension::D2)).unwrap();
        let caster = triangle_pipeline(&device);
        let group = super::receiver::receiver_group(&device, &queue, &map, abi, compare);
        let (texture, call, slots) = if abi.dimension == ReceiverDimension::D2 {
            (
                "texture_depth_2d",
                "rd_shadow_2d_bilinear(map, vec2(8.25 / 17.0, 8.5 / 17.0), 0.5, vec2(0.0))",
                5,
            )
        } else {
            (
                "texture_depth_2d_array",
                "rd_shadow_array_bilinear(map, vec2(8.25 / 17.0, 8.5 / 17.0), 3, 0.5, vec2(0.0))",
                20,
            )
        };
        let pipeline =
            super::receiver::receiver_pipeline_with_sample(&device, abi, texture, call, slots);
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        clear_layers(&map, &mut encoder);
        {
            let mut pass = map
                .begin_layer_pass(&mut encoder, layers - 1, None)
                .unwrap();
            pass.set_pipeline(&caster);
            pass.set_scissor_rect(0, 0, 8, 17);
            pass.draw(0..3, 0..1);
        }
        {
            let mut pass = output.begin_layer_pass(&mut encoder, 0, None).unwrap();
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &group, &[]);
            pass.draw(0..3, 0..1);
        }
        let copy = output
            .copy_layer_to_readback(&device, &mut encoder, 0)
            .unwrap();
        queue.submit([encoder.finish()]);
        let actual = depths(&device, &copy)[8 * 17 + 8];
        // Receiver x=8.25 lies three quarters from the dark texel centre at 7.5
        // to the lit centre at 8.5. No outer texel contributes.
        assert!((actual - 0.75).abs() < 1e-5, "edge visibility {actual}");
    }
}

#[test]
fn caster_slope_bias_matches_raster_depth_derivative() {
    let (device, queue) = device();
    let map = DepthMap::new(&device, descriptor(1, ReceiverDimension::D2)).unwrap();
    let mut samples = Vec::new();
    for slope in [0.0, 0.003] {
        let pipeline = sloped_caster(&device, slope);
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        {
            let mut pass = map.begin_layer_pass(&mut encoder, 0, None).unwrap();
            pass.set_pipeline(&pipeline);
            pass.draw(0..3, 0..1);
        }
        let copy = map
            .copy_layer_to_readback(&device, &mut encoder, 0)
            .unwrap();
        queue.submit([encoder.finish()]);
        samples.push(depths(&device, &copy)[8 * 17 + 8]);
    }
    // NDC x spans two units across seventeen pixels; z increases by 0.1 per x.
    let expected_offset = 0.003 * 0.2 / 17.0;
    assert!((samples[1] - samples[0] - expected_offset).abs() < 1e-7);
}

fn sloped_caster(device: &wgpu::Device, slope: f32) -> wgpu::RenderPipeline {
    let source = TRIANGLE.replace("0.25, 1.0", "0.25 + positions[vertex].x * 0.1, 1.0");
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("slope derivative oracle"),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[],
        immediate_size: 0,
    });
    create_depth_pipeline(
        device,
        DepthPipelineDescriptor {
            label: None,
            layout: &layout,
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            fragment: None,
            compare: wgpu::CompareFunction::Less,
            bias: wgpu::DepthBiasState {
                slope_scale: slope,
                ..Default::default()
            },
        },
    )
}
