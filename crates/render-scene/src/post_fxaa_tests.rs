use worldrunner_render_plan as s;
use worldrunner_render_post as p;

pub(crate) fn effects(quality: Option<s::SceneFxaaQuality>) -> s::SceneEffects {
    crate::post_bloom_tests::effects(crate::post_bloom_tests::bloom())
        .with_hdr_post(s::SceneHdrPost {
            tone_map: None,
            grading: None,
            adaptive_exposure: None,
            bloom: None,
            fxaa_subpixel_micro: Some(750_000),
            fxaa_quality: quality,
        })
        .unwrap()
}

#[test]
fn omitted_and_explicit_default_quality_translate_identically() {
    let omitted = crate::post_settings(&effects(None)).unwrap();
    let explicit = crate::post_settings(&effects(Some(s::SceneFxaaQuality::default()))).unwrap();
    assert_eq!(omitted.resolve, p::PostResolve::fxaa(0.75));
    assert_eq!(explicit.resolve, omitted.resolve);
}

#[test]
fn scene_quality_reaches_existing_post_executor_settings() {
    for (kernel, expected_kernel) in [
        (s::SceneFxaaKernel::Sobel, p::FxaaKernel::Sobel),
        (
            s::SceneFxaaKernel::SecondDifference,
            p::FxaaKernel::SecondDifference,
        ),
        (
            s::SceneFxaaKernel::Quality {
                preset: 12,
                sample_scale_micro: [500_000, 1_500_000],
            },
            p::FxaaKernel::Quality {
                preset: 12,
                sample_scale: [0.5, 1.5],
            },
        ),
    ] {
        for (luminance, expected_luminance) in [
            (s::SceneFxaaLuminance::Linear, p::FxaaLuminance::Linear),
            (s::SceneFxaaLuminance::Srgb, p::FxaaLuminance::Srgb),
            (s::SceneFxaaLuminance::Green, p::FxaaLuminance::Green),
        ] {
            let settings = crate::post_settings(&effects(Some(s::SceneFxaaQuality {
                edge_threshold_micro: 125_000,
                minimum_threshold_micro: 31_250,
                subpixel_blend_micro: 500_000,
                luminance,
                kernel,
            })))
            .unwrap();
            assert_eq!(
                settings.resolve,
                p::PostResolve::fxaa_with(
                    0.75,
                    p::FxaaQuality {
                        edge_threshold: 0.125,
                        minimum_threshold: 0.03125,
                        subpixel_blend: 0.5,
                        luminance: expected_luminance,
                        kernel: expected_kernel,
                    }
                )
            );
        }
    }
}

#[test]
fn scene_anti_alias_strength_uses_hdr_quality_kernel() {
    let authored = s::SceneFxaaQuality {
        kernel: s::SceneFxaaKernel::Quality {
            preset: 12,
            sample_scale_micro: [500_000; 2],
        },
        ..Default::default()
    };
    let effects = crate::post_taa_tests::effects(
        s::SceneAntiAlias::Fxaa {
            subpixel_micro: 750_000,
        },
        None,
    )
    .with_hdr_post(s::SceneHdrPost {
        tone_map: None,
        grading: None,
        adaptive_exposure: None,
        bloom: None,
        fxaa_subpixel_micro: None,
        fxaa_quality: Some(authored),
    })
    .unwrap();
    let actual = crate::post_settings(&effects).unwrap();
    assert_eq!(
        actual.resolve,
        p::PostResolve::fxaa_with(
            0.75,
            p::FxaaQuality {
                kernel: p::FxaaKernel::Quality {
                    preset: 12,
                    sample_scale: [0.5; 2]
                },
                ..Default::default()
            }
        )
    );
}
