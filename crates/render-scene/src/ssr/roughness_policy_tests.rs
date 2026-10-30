use super::*;
use worldrunner_render_plan::{SceneSsrPassBehind, SceneSsrTraversal};

#[test]
fn decreasing_curves_match_independent_integer_oracle() {
    let context = context();
    // Binary endpoints exercise exact ties; decimal endpoints use samples away from f32-sensitive ties.
    for (start, end, offset) in [
        (750_000, 250_000, 0),
        (1_000_000, 0, 0),
        (900_000, 100_000, 15_625),
    ] {
        let policy = SceneSsr {
            roughness_pass_behind: Some(SceneSsrPassBehind {
                minimum_steps: 2,
                maximum_steps: 10,
                roughness_start_micro: start,
                roughness_end_micro: end,
            }),
            ..settings()
        };
        policy.validate().unwrap();
        let curve = &gpu::controls(&policy)[8..];
        let shader = format!(
            "{}\n@fragment fn fs_screen(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {{
            return vec4<f32>(f32(reflection_pass_count(f32(u32(p.x)) / 16.0 + {}, 0.0,
                vec4<f32>({}, {}, {}, {}))), 0.0, 0.0, 1.0);
            }}",
            include_str!("controls.wgsl"), f64::from(offset) / 1_000_000.0, curve[0], curve[1], curve[2], curve[3]
        );
        let actual = super::fidelity_tests::evaluate(&context, &shader, 17);
        for (index, actual) in actual.into_iter().enumerate() {
            let roughness = u32::try_from(index).unwrap() * 62_500 + offset;
            let numerator = start.saturating_sub(roughness).min(start - end) * 8;
            let denominator = start - end;
            let quotient = numerator / denominator;
            let remainder = numerator % denominator;
            let round_up =
                remainder * 2 > denominator || (remainder * 2 == denominator && quotient % 2 != 0);
            assert_eq!(
                actual.to_bits(),
                gpu::scalar(2 + quotient + u32::from(round_up)).to_bits()
            );
        }
    }
}

#[test]
fn disabled_cutoff_accepts_roughness_one_without_changing_pass_budget() {
    let context = context();
    for traversal in [SceneSsrTraversal::Conservative, SceneSsrTraversal::Grid] {
        let disabled = SceneSsr {
            roughness_cutoff_disabled: true,
            traversal,
            ..settings()
        };
        assert_eq!(
            gpu::controls(&disabled).map(f32::to_bits),
            gpu::controls(&settings_with(traversal)).map(f32::to_bits)
        );
        for (policy, expected) in [(settings_with(traversal), false), (disabled, true)] {
            let bytes =
                super::controls_tests::capture_material(&context, policy, 1.0, false, None, 1.0);
            assert_eq!(super::controls_tests::center_red(&bytes) > 0, expected);
        }
        let bytes =
            super::controls_tests::capture_material(&context, disabled, 1.0, true, None, 1.0);
        let baseline = super::controls_tests::capture_material(
            &context,
            settings_with(traversal),
            1.0,
            true,
            None,
            0.0,
        );
        assert_eq!(bytes, baseline);
    }
}

fn settings_with(traversal: SceneSsrTraversal) -> SceneSsr {
    SceneSsr {
        traversal,
        ..settings()
    }
}

#[test]
fn disabled_cutoff_resolves_rough_surfaces_without_fading() {
    let context = context();
    for roughness in [0.0, 0.5, 1.0] {
        let bytes = super::numerical::resolve_capture_settings(
            &context,
            1.0,
            roughness,
            false,
            SceneSsr {
                roughness_cutoff_disabled: true,
                ..settings()
            },
        );
        // The resolved red is 0.75 for scene colour up to 1, so the f32 arithmetic range is 1;
        // `assert_half_value` adds the half store bound.
        crate::tolerance_tests::assert_half_value(
            f64::from(::half::f16::from_le_bytes([bytes[32], bytes[33]]).to_f32()),
            0.75,
            1.0,
            &format!("resolved red at roughness {roughness}"),
        );
    }
}

#[test]
fn default_trace_matches_prior_shader_bytes() {
    let context = context();
    let source = gpu::trace_source(1, false).replace(
        "(camera.settings.z > 0.0 && roughness >= camera.settings.z)",
        "roughness >= camera.settings.z",
    );
    assert_eq!(
        super::controls_tests::capture_material(&context, settings(), 1.0, false, None, 0.5),
        super::controls_tests::capture_material(
            &context,
            settings(),
            1.0,
            false,
            Some(&source),
            0.5
        ),
    );
    assert_eq!(
        gpu::roughness_cutoff(&settings()).to_bits(),
        0.6f32.to_bits()
    );
    let expected: Vec<_> = [0.75, 0.625, 0.5625, 1.0]
        .repeat(9)
        .into_iter()
        .flat_map(|value| half(value).to_le_bytes())
        .collect();
    assert_eq!(
        super::numerical::resolve_capture(&context, 1.0, 0.0, false),
        expected
    );
}
