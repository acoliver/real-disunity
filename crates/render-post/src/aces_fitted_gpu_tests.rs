use super::*;

fn assert_readback(actual: &[u8], expected: [f64; 3]) {
    for pixel in actual.chunks_exact(4) {
        for (&channel, linear) in pixel[..3].iter().zip(expected) {
            let byte = encoded(linear);
            assert!(
                (byte - (byte.floor() + 0.5)).abs() > 0.1,
                "rounding boundary {byte}"
            );
            assert_eq!(f64::from(channel).to_bits(), byte.round().to_bits());
        }
        assert_eq!(pixel[3], 64);
    }
}

#[test]
fn legacy_and_graded_gpu_bloom_composition_match_reference() {
    let (device, queue) = crate::gpu_tests::gpu();
    for curve in [ToneMap::Reinhard, ToneMap::Filmic] {
        for scale in [1.0, 2.0] {
            let grading = ColorGrading {
                linear_scale: scale,
                ..Default::default()
            };
            let settings = HdrPostSettings {
                grading,
                bloom: Some(Bloom {
                    policies: BloomPolicies::default(),
                    threshold: 0.0,
                    knee: 0.0,
                    intensity: 0.5,
                    levels: 1,
                }),
                ..Default::default()
            };
            let (actual, _) = render(
                (&device, &queue),
                FrameConfig::new([5, 3], wgpu::TextureFormat::Rgba16Float, 1),
                (curve, 0.5),
                &settings,
                "return vec4(0.25,0.25,0.25,0.25);",
            );
            let expected = grading.evaluate(curve, [0.25; 3], 0.5, ([0.25; 3], 0.5), [0.5; 2]);
            assert_readback(&actual, expected);
        }
    }
}

#[test]
fn aces_matrix_gpu_primaries_ramp_and_default_identity() {
    let (device, queue) = crate::gpu_tests::gpu();
    let values = [
        [0.0; 3],
        [0.09375; 3],
        [0.75; 3],
        [4.5; 3],
        [5.0, 0.0, 0.0],
        [0.0, 5.0, 0.0],
        [0.0, 0.0, 5.0],
    ];
    let body = "let v=array<vec3<f32>,7>(vec3(0.0),vec3(0.09375),vec3(0.75),vec3(4.5),vec3(5.0,0.0,0.0),vec3(0.0,5.0,0.0),vec3(0.0,0.0,5.0))[u32(p.x)];return vec4(v,0.25);";
    let config = FrameConfig::new([7, 3], wgpu::TextureFormat::Rgba16Float, 1);
    let (fixed, _) = render(
        (&device, &queue),
        config,
        (ToneMap::AcesFitted, 1.0),
        &HdrPostSettings::default(),
        body,
    );
    let (post, _) = render(
        (&device, &queue),
        config,
        (ToneMap::Reinhard, 1.0),
        &HdrPostSettings {
            tone_map: Some(ToneMap::AcesFitted),
            ..Default::default()
        },
        body,
    );
    assert_eq!(fixed, post);
    for (i, pixel) in fixed.chunks_exact(4).enumerate() {
        let expected = ToneMap::AcesFitted.evaluate_rgb(values[i % 7], 1.0);
        for (&actual, linear) in pixel[..3].iter().zip(expected) {
            let v = encoded(linear);
            assert!((v - (v.floor() + 0.5)).abs() > 0.1, "rounding boundary {v}");
            assert_eq!(f64::from(actual).to_bits(), v.round().to_bits());
        }
        assert_eq!(pixel[3], 64);
    }
}

#[test]
fn aces_gpu_composition_power_and_radial_grade_at_odd_size() {
    let (device, queue) = crate::gpu_tests::gpu();
    let grading = ColorGrading {
        linear_scale: 2.0,
        display_power: [1.1, 0.9, 1.5],
        vignette: Some(Vignette {
            axis_scale: [1.0, 0.8],
            tint_range: [0.0, 1.0],
            tint: [0.25, 0.5, 0.75, 0.3],
            darken_range: [1.0, 0.0],
            darken_opacity: 0.6,
        }),
    };
    let settings = HdrPostSettings {
        grading,
        tone_map: Some(ToneMap::AcesFitted),
        bloom: Some(Bloom {
            policies: BloomPolicies::default(),
            threshold: 0.0,
            knee: 0.0,
            intensity: 0.5,
            levels: 1,
        }),
        ..Default::default()
    };
    let config = FrameConfig::new([5, 5], wgpu::TextureFormat::Rgba16Float, 1);
    let (pixels, _) = render(
        (&device, &queue),
        config,
        (ToneMap::Reinhard, 0.5),
        &settings,
        "return vec4(0.5,0.5,0.5,0.25);",
    );
    for i in [0_usize, 2, 12] {
        let pixel = &pixels[i * 4..i * 4 + 4];
        let x = u32::try_from(i % 5).unwrap();
        let y = u32::try_from(i / 5).unwrap();
        let expected = grading.evaluate(
            ToneMap::AcesFitted,
            [0.5; 3],
            0.5,
            ([0.5; 3], 0.5),
            [(f64::from(x) + 0.5) / 5.0, (f64::from(y) + 0.5) / 5.0],
        );
        for (&actual, linear) in pixel[..3].iter().zip(expected) {
            let v = encoded(linear);
            assert!(
                (v - (v.floor() + 0.5)).abs() > 0.15,
                "rounding boundary {v}"
            );
            assert_eq!(
                f64::from(actual).to_bits(),
                v.round().to_bits(),
                "pixel {i}: observed {actual}, expected {v}"
            );
        }
        assert_eq!(pixel[3], 64);
    }
}

#[test]
fn aces_gpu_special_values() {
    let (device, queue) = crate::gpu_tests::gpu();
    for (value, reference) in [
        ("bitcast<f32>(0x7f800000u)", f64::INFINITY),
        ("bitcast<f32>(0xff800000u)", f64::NEG_INFINITY),
        ("bitcast<f32>(0x7fc00000u)", f64::NAN),
    ] {
        for exposure in [0.0, 1e-18, 1.0] {
            for settings in [
                HdrPostSettings::default(),
                HdrPostSettings {
                    resolve: crate::PostResolve::fxaa(0.0),
                    ..Default::default()
                },
                HdrPostSettings {
                    grading: ColorGrading {
                        linear_scale: 1e-18,
                        ..Default::default()
                    },
                    ..Default::default()
                },
                HdrPostSettings {
                    grading: ColorGrading {
                        linear_scale: 0.0,
                        ..Default::default()
                    },
                    ..Default::default()
                },
            ] {
                let body = format!("return vec4(vec3({value}),0.25);");
                let (actual, _) = render(
                    (&device, &queue),
                    FrameConfig::new([3, 1], wgpu::TextureFormat::Rgba16Float, 1),
                    (ToneMap::AcesFitted, exposure),
                    &settings,
                    &body,
                );
                let expected = settings.grading.evaluate(
                    ToneMap::AcesFitted,
                    [reference; 3],
                    exposure,
                    ([0.0; 3], 0.0),
                    [0.5; 2],
                );
                assert_readback(&actual, expected);
            }
        }
    }
}
