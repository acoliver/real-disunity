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
