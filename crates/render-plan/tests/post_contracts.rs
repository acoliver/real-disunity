use worldrunner_render_plan::post::{
    MAX_GAUSSIAN_RADIUS, PostContractError, RenderScale, UpscaleMode, UpscaleSettings,
};

#[test]
fn cpu_consumer_uses_scale_rounding_and_radius() {
    assert_eq!(MAX_GAUSSIAN_RADIUS, 64);
    for (scale, output, expected) in [
        (500_000, [1, 1], [1, 1]),
        (770_000, [50, 26], [39, 20]),
        (1_000_000, [u32::MAX, u32::MAX], [u32::MAX, u32::MAX]),
    ] {
        assert_eq!(
            RenderScale::new(scale).unwrap().extent(output),
            Ok(expected)
        );
    }
    for output in [[0, 1], [1, 0], [0, 0]] {
        let error = RenderScale::new(500_000)
            .unwrap()
            .extent(output)
            .unwrap_err();
        assert_eq!(error, PostContractError::EmptyExtent);
        assert_eq!(error.to_string(), "post target extent is empty");
    }
}

#[test]
fn cpu_consumer_preserves_admission_errors() {
    for scale in [0, 499_999, 1_000_001, u32::MAX] {
        let error = RenderScale::new(scale).unwrap_err();
        assert_eq!(
            error,
            PostContractError::UnsupportedProfile(
                "render scale must be 500000..=1000000 millionths",
            )
        );
        assert_eq!(
            error.to_string(),
            "unsupported frame profile: render scale must be 500000..=1000000 millionths"
        );
    }
    for sharpness in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -0.01, 2.01] {
        let error = UpscaleSettings {
            mode: UpscaleMode::Easu,
            sharpness: Some(sharpness),
        }
        .validate()
        .unwrap_err();
        assert_eq!(
            error,
            PostContractError::UnsupportedProfile(
                "RCAS sharpness must be finite and within 0..=2 stops",
            )
        );
        assert_eq!(
            error.to_string(),
            "unsupported frame profile: RCAS sharpness must be finite and within 0..=2 stops"
        );
    }
    for mode in [
        UpscaleMode::Bilinear,
        UpscaleMode::Easu,
        UpscaleMode::Temporal,
    ] {
        for sharpness in [None, Some(0.0), Some(2.0)] {
            assert_eq!(UpscaleSettings { mode, sharpness }.validate(), Ok(()));
        }
    }
}
