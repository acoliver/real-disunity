use super::{SceneFxaaKernel, SceneFxaaLuminance, SceneFxaaQuality, SceneHdrPost};

fn recipe() -> SceneHdrPost {
    SceneHdrPost {
        tone_map: None,
        grading: None,
        adaptive_exposure: None,
        bloom: None,
        fxaa_subpixel_micro: Some(750_000),
        fxaa_quality: None,
    }
}

#[test]
fn omitted_quality_preserves_existing_serialized_recipe() {
    let json = r#"{"tone_map":null,"grading":null,"adaptive_exposure":null,"bloom":null,"fxaa_subpixel_micro":750000}"#;
    let value: SceneHdrPost = serde_json::from_str(json).unwrap();
    assert_eq!(value, recipe());
    assert_eq!(serde_json::to_string(&value).unwrap(), json);
}

#[test]
fn complete_quality_round_trips_for_all_kernels_and_luminance_inputs() {
    for kernel in [SceneFxaaKernel::Sobel, SceneFxaaKernel::SecondDifference] {
        for luminance in [
            SceneFxaaLuminance::Linear,
            SceneFxaaLuminance::Srgb,
            SceneFxaaLuminance::Green,
        ] {
            let value = SceneHdrPost {
                fxaa_quality: Some(SceneFxaaQuality {
                    edge_threshold_micro: 125_000,
                    minimum_threshold_micro: 31_250,
                    subpixel_blend_micro: 500_000,
                    kernel,
                    luminance,
                }),
                ..recipe()
            };
            value.validate().unwrap();
            let bytes = serde_json::to_vec(&value).unwrap();
            let decoded: SceneHdrPost = serde_json::from_slice(&bytes).unwrap();
            decoded.validate().unwrap();
            assert_eq!(decoded, value);
        }
    }
}

#[test]
fn quality_rejects_out_of_range_units_and_malformed_inputs() {
    for value in [0, 1_000_000, 1_000_001, u32::MAX] {
        for q in [
            SceneFxaaQuality {
                edge_threshold_micro: value,
                ..Default::default()
            },
            SceneFxaaQuality {
                minimum_threshold_micro: value,
                ..Default::default()
            },
            SceneFxaaQuality {
                subpixel_blend_micro: value,
                ..Default::default()
            },
        ] {
            let post = SceneHdrPost {
                fxaa_quality: Some(q),
                ..recipe()
            };
            assert_eq!(post.validate().is_ok(), value <= 1_000_000);
        }
    }
    let mut json = serde_json::to_value(SceneFxaaQuality::default()).unwrap();
    for invalid in [
        serde_json::json!(-1),
        serde_json::json!(0.5),
        serde_json::json!(4_294_967_296_u64),
    ] {
        json["edge_threshold_micro"] = invalid;
        assert!(serde_json::from_value::<SceneFxaaQuality>(json.clone()).is_err());
    }
    json["edge_threshold_micro"] = serde_json::json!(0);
    json["kernel"] = serde_json::json!("unknown");
    assert!(serde_json::from_value::<SceneFxaaQuality>(json).is_err());
}
