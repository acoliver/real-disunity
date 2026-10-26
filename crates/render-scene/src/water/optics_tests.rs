use super::*;
use worldrunner_render_plan::{SceneWaterOptics, SceneWaterShoreline};

#[test]
fn optical_and_shoreline_controls_validate_before_allocation() {
    let mut body = settings();
    body.material.optics = Some(SceneWaterOptics {
        fresnel_micro: 20_370,
        specular_micro: 1_000_000,
        scattering_micro: [20_000, 30_000, 30_000],
        tint: None,
    });
    body.material.shoreline = Some(SceneWaterShoreline {
        depth_millimeters: 1000,
        normal_gain_micro: -1_000_000,
        opacity_micro: 1_000_000,
    });
    assert!(validate_settings(&body).is_ok());
    let valid = body.clone();
    body.material.optics.as_mut().unwrap().fresnel_micro = 1_000_001;
    assert!(validate_settings(&body).is_err());
    body = valid.clone();
    body.material.optics.as_mut().unwrap().scattering_micro[0] = 10_000_001;
    assert!(validate_settings(&body).is_err());
    body = valid.clone();
    body.material.optics.as_mut().unwrap().specular_micro = 10_000_001;
    assert!(validate_settings(&body).is_err());
    body = valid.clone();
    body.material.shoreline.as_mut().unwrap().depth_millimeters = 0;
    assert!(validate_settings(&body).is_err());
    body = valid.clone();
    body.material.shoreline.as_mut().unwrap().normal_gain_micro = i32::MIN;
    assert!(validate_settings(&body).is_err());
    body = valid;
    body.material.shoreline.as_mut().unwrap().opacity_micro = 1_000_001;
    assert!(validate_settings(&body).is_err());
    assert_eq!(optics::uniforms(&settings()).map(f32::to_bits), [0; 28]);
}

#[test]
fn shader_optical_equations_match_depth_endpoints_and_bounds() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    let source = format!(
        "{}\n{}",
        include_str!("optics.wgsl"),
        r"
@group(0) @binding(0) var<storage, read_write> output: array<vec4<f32>>;
@compute @workgroup_size(1) fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let depths = array<f32, 5>(0.0, 0.001, 0.025, 0.5, 2.0);
    let d = depths[id.x];
    let offset = water_refraction_offset(vec2<f32>(0.6, -0.8), d, 0.25);
    output[id.x * 2u] = vec4<f32>(offset, water_flow_at_depth(d), water_opacity_at_depth(d));
    output[id.x * 2u + 1u] = vec4<f32>(water_normal_at_depth(vec3<f32>(-0.3, 0.4, 0.8), d).xy,
        exp(-0.3 * d), water_view_scattering(vec3<f32>(0.0, 0.0, 1.0), vec3<f32>(0.03)).x);
}"
    );
    let result = compute(&context, &source);
    for (index, depth) in [0.0f32, 0.001, 0.025, 0.5, 2.0].into_iter().enumerate() {
        let shape = (depth.powf(0.4) * 2.0).clamp(0.0, 1.0);
        let expected = [
            0.15 * depth.min(1.0),
            -0.2 * depth.min(1.0),
            depth.min(1.0).powf(0.2) * 0.2,
            (depth * 20.0).min(1.0),
            0.3 * shape,
            0.4 * shape,
            transmittance(0.3, depth),
            0.03,
        ];
        for (actual, expected) in result[index * 8..index * 8 + 8].iter().zip(expected) {
            assert!(
                (actual - expected).abs() < 0.00001,
                "{actual} versus {expected}"
            );
        }
        assert!(glam::Vec2::new(result[index * 8], result[index * 8 + 1]).length() <= 0.250_001);
    }
}

#[test]
fn tint_validation_and_uniforms_preserve_disabled_bytes() {
    use worldrunner_render_plan::SceneWaterTint;
    let mut body = settings();
    body.material.optics = Some(SceneWaterOptics {
        fresnel_micro: 20_370,
        specular_micro: 1_000_000,
        scattering_micro: [0; 3],
        tint: None,
    });
    let disabled = optics::uniforms(&body);
    assert_eq!(
        disabled[16..]
            .iter()
            .map(|v| v.to_bits())
            .collect::<Vec<_>>(),
        vec![0; 12]
    );
    let tint = SceneWaterTint {
        colors_micro: [[250_000, 500_000, 750_000], [750_000, 500_000, 250_000]],
        decay_micro: [1_000_000, 2_000_000],
        offsets_micro: [250_000, -125_000],
    };
    body.material.optics.as_mut().unwrap().tint = Some(tint);
    assert!(validate_settings(&body).is_ok());
    let encoded = optics::uniforms(&body);
    assert_eq!(encoded[..16], disabled[..16]);
    assert_eq!(
        &encoded[16..],
        &[
            0.25, 0.5, 0.75, 1.0, 0.75, 0.5, 0.25, 0.0, 1.0, 2.0, 0.25, -0.125
        ]
    );
    for invalid in [
        SceneWaterTint {
            colors_micro: [[1_000_001; 3]; 2],
            ..tint
        },
        SceneWaterTint {
            decay_micro: [100_000_001, 0],
            ..tint
        },
        SceneWaterTint {
            decay_micro: [0, 100_000_001],
            ..tint
        },
        SceneWaterTint {
            offsets_micro: [i32::MIN, 0],
            ..tint
        },
        SceneWaterTint {
            offsets_micro: [0, 1_000_001],
            ..tint
        },
    ] {
        body.material.optics.as_mut().unwrap().tint = Some(invalid);
        assert!(validate_settings(&body).is_err());
    }
}

#[test]
fn tint_ramp_endpoints_midpoints_and_offsets_match_cpu() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    for offsets in [
        [0.0f32, 0.0],
        [0.25, 0.125],
        [-0.25, -0.125],
        [1.0, -1.0],
        [-1.0, 1.0],
    ] {
        let source = format!(
            "{}\n{}",
            include_str!("optics.wgsl"),
            format_args!(
                r"
@group(0) @binding(0) var<storage, read_write> output: array<vec4<f32>>;
@compute @workgroup_size(1) fn main(@builtin(global_invocation_id) id: vec3<u32>) {{
    let depths = array<f32, 5>(0.0, 0.0752575, 0.150515, 100.0, 1.0);
    let d = depths[id.x];
    let controls = vec4<f32>(1.0, 2.0, {0}, {1});
    let weights = water_tint_weights(d, controls);
    let color = water_tint_color(vec3<f32>(0.2, 0.4, 0.8), d,
        vec3<f32>(0.25, 0.5, 0.75), vec3<f32>(0.75, 0.5, 0.25), controls);
    output[id.x * 2u] = vec4<f32>(weights, color.xy);
    output[id.x * 2u + 1u] = vec4<f32>(color.z, 0.0, 0.0, 0.0);
}}",
                offsets[0], offsets[1]
            )
        );
        let actual = compute(&context, &source);
        for (index, depth) in [0.0f32, 0.075_257_5, 0.150_515, 100.0, 1.0]
            .into_iter()
            .enumerate()
        {
            let first =
                ((100.0f32.powf(-depth) - offsets[0]).clamp(0.0, 1.0) + offsets[1]).clamp(0.0, 1.0);
            let second = 100.0f32.powf(-2.0 * depth).clamp(0.0, 1.0);
            let mut expected = vec![first, second];
            for ((deep, shallow), background) in [0.25f32, 0.5, 0.75]
                .into_iter()
                .zip([0.75f32, 0.5, 0.25])
                .zip([0.2, 0.4, 0.8])
            {
                let a = deep.powf(2.2) * (1.0 - first) + 4.0 * background * deep * first;
                let b = shallow.powf(2.2) * (1.0 - first) + 4.0 * background * shallow * first;
                expected.push((a * (1.0 - second) + b * second) * 0.25);
            }
            for (actual, expected) in actual[index * 8..index * 8 + 5].iter().zip(expected) {
                assert!(
                    (actual - expected).abs() < 0.00001,
                    "depth {depth}, offsets {offsets:?}: {actual} vs {expected}"
                );
            }
        }
        if offsets == [0.0, 0.0] {
            assert_eq!(&actual[..2], &[1.0, 1.0]);
            assert!((actual[9] - 0.5).abs() < 0.00001);
            assert!((actual[16] - 0.5).abs() < 0.00001);
            assert_eq!(&actual[24..26], &[0.0, 0.0]);
        }
    }
}

fn compute(context: &GpuContext, source: &str) -> Vec<f32> {
    let device = context.device();
    let output = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("water optical equation results"),
        size: 160,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("water optical equation readback"),
        size: 160,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("water optical equations"),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("water optical equations"),
        layout: None,
        module: &module,
        entry_point: Some("main"),
        compilation_options: wgpu::PipelineCompilationOptions::default(),
        cache: None,
    });
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: output.as_entire_binding(),
        }],
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor::default());
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.dispatch_workgroups(5, 1, 1);
    }
    encoder.copy_buffer_to_buffer(&output, 0, &readback, 0, 160);
    context.queue().submit([encoder.finish()]);
    let (tx, rx) = std::sync::mpsc::channel();
    readback
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| tx.send(result).unwrap());
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    rx.recv().unwrap().unwrap();
    let result = readback
        .slice(..)
        .get_mapped_range()
        .unwrap()
        .chunks_exact(4)
        .map(|bytes| f32::from_le_bytes(bytes.try_into().unwrap()))
        .collect();
    readback.unmap();
    result
}
