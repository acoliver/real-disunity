use super::*;
use worldrunner_render_plan::SceneSsrPassBehind;

fn evaluate(context: &GpuContext, shader: &str, width: u32) -> Vec<f32> {
    let pipeline = gpu::pipeline(context.device(), shader, 1, false);
    let target = crate::ssao::passes::texture(
        context.device(),
        [width, 1],
        wgpu::TextureFormat::Rgba16Float,
    );
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
        &pipeline,
        &[],
    );
    encoder.finish().submit().unwrap();
    read(context, &target)
        .chunks_exact(8)
        .map(|pixel| {
            let bits = u16::from_le_bytes(pixel[..2].try_into().unwrap());
            if bits == 0 {
                return 0.0;
            }
            let exponent = i32::from((bits >> 10) & 31) - 15;
            (1.0 + f32::from(bits & 1023) / 1024.0) * 2.0f32.powi(exponent)
        })
        .collect()
}

#[test]
fn gpu_first_tap_ao_matches_independent_double_precision_oracle() {
    let context = context();
    let depths = [1.0f64, 2.0, 3.0, 4.0];
    let weights = depths.map(|depth| (-(depth - depths[0]).abs()).exp());
    let expected = weights[3] / weights.iter().sum::<f64>();
    assert!((expected - 0.032_058_603_3).abs() < 1e-10);
    let receiver_relative = 1.0
        / depths
            .map(|depth| (-(depth - 4.0).abs()).exp())
            .iter()
            .sum::<f64>();
    assert!((receiver_relative - expected - 0.611_855_656_6).abs() < 1e-10);
    let shader = format!(
        "{}\n@fragment fn fs_screen() -> @location(0) vec4<f32> {{ return vec4<f32>(reflection_ao(vec4<f32>(1.0, 2.0, 3.0, 4.0), vec4<f32>(0.0, 0.0, 0.0, 1.0)), 0.0, 0.0, 1.0); }}",
        include_str!("ao_math.wgsl")
    );
    let actual = evaluate(&context, &shader, 1)[0];
    let (_, arithmetic) = super::ao_accuracy_tests::raw_oracle();
    let bound = arithmetic + super::ao_accuracy_tests::storage_error(expected + arithmetic);
    assert!((f64::from(actual) - expected).abs() <= bound);
}

#[test]
fn gpu_roughness_count_endpoints_clamping_rounding_and_constant_override() {
    let context = context();
    let curve = SceneSsrPassBehind {
        minimum_steps: 2,
        maximum_steps: 10,
        roughness_start_micro: 250_000,
        roughness_end_micro: 750_000,
    };
    let policy = SceneSsr {
        roughness_pass_behind: Some(curve),
        ..settings()
    };
    assert!(policy.validate().is_ok());
    let controls = gpu::controls(&policy);
    assert_eq!(&controls[8..], &[2.0, 10.0, 0.25, 0.75]);
    let shader = format!(
        "{}\n@fragment fn fs_screen(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {{
        let roughness = array<f32, 7>(0.0, 0.25, 0.40625, 0.5, 0.75, 1.0, 0.0)[u32(p.x)];
        var curve = vec4<f32>(2.0, 10.0, 0.25, 0.75);
        if p.x > 6.0 {{ curve.x = -1.0; }}
        return vec4<f32>(f32(reflection_pass_count(roughness, 7.0, curve)), 0.0, 0.0, 1.0);
    }}",
        include_str!("controls.wgsl")
    );
    assert_eq!(
        evaluate(&context, &shader, 7),
        [2.0, 2.0, 4.0, 6.0, 10.0, 10.0, 7.0]
    );
}

#[test]
fn application_ao_compiles_for_opaque_water_and_multisampling() {
    let context = context();
    for samples in [1, 4] {
        let mut screen = SceneMetadata::new(context.device(), [3, 3], neutral_ao()).unwrap();
        Ssr::new(
            context.device(),
            FrameConfig::new([3, 3], wgpu::TextureFormat::Rgba16Float, samples),
            SceneSsr {
                depth_relative_ao: true,
                ..settings()
            },
            &mut screen,
        )
        .unwrap();
    }
}

#[test]
fn invalid_roughness_counts_fail_while_disabled() {
    let valid = SceneSsrPassBehind {
        minimum_steps: 2,
        maximum_steps: 10,
        roughness_start_micro: 250_000,
        roughness_end_micro: 750_000,
    };
    for curve in [
        SceneSsrPassBehind {
            minimum_steps: 11,
            ..valid
        },
        SceneSsrPassBehind {
            maximum_steps: 257,
            ..valid
        },
        SceneSsrPassBehind {
            roughness_start_micro: 750_000,
            ..valid
        },
        SceneSsrPassBehind {
            roughness_end_micro: 0,
            ..valid
        },
        SceneSsrPassBehind {
            roughness_end_micro: 1_000_001,
            ..valid
        },
    ] {
        assert!(
            SceneSsr {
                enabled: false,
                roughness_pass_behind: Some(curve),
                ..settings()
            }
            .validate()
            .is_err()
        );
    }
}

pub(super) fn prepare_application_ao(context: &GpuContext, ssr: &Ssr, final_depth: &wgpu::Texture) {
    if ssr.settings.depth_relative_ao {
        upload(
            context,
            &ssr.application_ao,
            &[1.0, 0.0, 0.0, 1.0, 1.0, 0.0, 1.0, 0.0, 0.0],
        );
        upload(
            context,
            &ssr.hierarchy[0],
            &[1.0, 1.0, 1.0, 1.0, 4.0, 3.0, 1.0, 1.0, 2.0],
        );
        upload(
            context,
            final_depth,
            &[1.0, 1.0, 1.0, 1.0, 4.0, 3.0, 1.0, 1.0, 2.0],
        );
    }
}

#[test]
fn gpu_production_resolve_applies_first_tap_ao_without_changing_disabled_bytes() {
    let context = context();
    let policy = SceneSsr {
        depth_relative_ao: true,
        roughness_fade_start_micro: Some(550_000),
        ..settings()
    };
    let bytes = super::numerical::resolve_capture_settings(&context, 1.0, 0.5, false, policy);
    super::ao_accuracy_tests::assert_resolve(
        super::ao_accuracy_tests::half_value(&bytes[32..34]),
        0.25,
        0.0,
        1.0,
    );
    let disabled = SceneSsr {
        depth_relative_ao: false,
        ..settings()
    };
    assert_eq!(
        super::numerical::resolve_capture_settings(&context, 1.0, 0.0, false, disabled),
        super::numerical::resolve_capture(&context, 1.0, 0.0, false)
    );
}

#[test]
fn gpu_roughness_count_controls_production_pass_behind_at_endpoints() {
    let context = context();
    let policy = SceneSsr {
        roughness_pass_behind: Some(SceneSsrPassBehind {
            minimum_steps: 0,
            maximum_steps: 256,
            roughness_start_micro: 0,
            roughness_end_micro: 500_000,
        }),
        ..settings()
    };
    for (roughness, expected) in [(0.0, false), (0.5, true)] {
        let shader = gpu::trace_source(1, false).replace(
            "var roughness = textureLoad(materials, p, 0).a;",
            &format!("var roughness = textureLoad(materials, p, 0).a * 0.0 + {roughness};"),
        );
        let bytes =
            super::controls_tests::capture_with_shader(&context, policy, 1.0, true, Some(&shader));
        assert_eq!(super::controls_tests::center_red(&bytes) > 0, expected);
    }
}

#[test]
fn gpu_replacement_ao_retains_the_published_lobe_and_confidence() {
    let context = context();
    let policy = SceneSsr {
        depth_relative_ao: true,
        roughness_fade_start_micro: Some(550_000),
        composition: worldrunner_render_plan::SceneSsrComposition::ReplaceEnvironment,
        ..settings()
    };
    for confidence in [0.0, 0.5, 1.0] {
        let bytes =
            super::numerical::resolve_capture_settings(&context, confidence, 0.5, false, policy);
        super::ao_accuracy_tests::assert_resolve(
            super::ao_accuracy_tests::half_value(&bytes[32..34]),
            0.25,
            0.125,
            f64::from(confidence),
        );
    }
}

#[test]
fn application_ao_pairs_half_resolution_with_averaged_depth() {
    let context = context();
    let ao = SceneSsao {
        kernel: worldrunner_render_plan::SceneSsaoKernel::Scalable {
            turns_micro: 7_000_000,
            power_micro: 1_000_000,
        },
        filter: worldrunner_render_plan::SceneSsaoFilter::ScalableBilateral {
            radius: 4,
            step: 2,
            sigma_scale_micro: 1_500_000,
            sharpness_micro: 7_000_000,
        },
        sample_count: 10,
        ..neutral_ao()
    };
    let mut screen = SceneMetadata::new(context.device(), [7, 5], ao).unwrap();
    let ssr = Ssr::new(
        context.device(),
        FrameConfig::new([7, 5], wgpu::TextureFormat::Rgba16Float, 1),
        SceneSsr {
            depth_relative_ao: true,
            ..settings()
        },
        &mut screen,
    )
    .unwrap();
    assert_eq!(
        [ssr.application_ao.width(), ssr.application_ao.height()],
        [4, 3]
    );
    assert_eq!(ssr.application_ao.format(), wgpu::TextureFormat::Rg16Float);
    let source = gpu::configured_resolve_source(6, 4, true, true, true);
    assert!(source.contains("tap.g * reflection_far_depth()"));
    assert_eq!(source.matches("@binding(").count(), 17);
}
