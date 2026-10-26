use crate::{FxaaLuminance, FxaaQuality, HdrPostSettings, ToneMap};

#[test]
fn filmic_reference_vectors_and_special_values() {
    for (x, expected) in [
        (0.0, 0.0),
        (0.125, 0.170_701_427_684_667_92),
        (0.5, 0.616_306_954_436_450_8),
        (1.0, 0.803_797_468_354_430_2),
        (4.0, 0.973_417_109_714_837_9),
        (65_504.0, 1.0),
    ] {
        assert!((ToneMap::Filmic.evaluate(x, 1.0) - expected).abs() < 1e-12);
    }
    for x in [-1.0, f64::NEG_INFINITY, f64::NAN] {
        assert_eq!(
            ToneMap::Filmic.evaluate(x, 1.0).to_bits(),
            0.0_f64.to_bits()
        );
    }
    assert_eq!(
        ToneMap::Filmic.evaluate(f64::INFINITY, 1.0).to_bits(),
        1.0_f64.to_bits()
    );
    assert_eq!(
        ToneMap::Filmic.evaluate(f64::MAX, 4.0).to_bits(),
        1.0_f64.to_bits()
    );
    assert_eq!(
        ToneMap::Filmic.evaluate(f64::INFINITY, 0.0).to_bits(),
        0.0_f64.to_bits()
    );
    assert_eq!(
        ToneMap::Filmic.evaluate(2.0, 0.25).to_bits(),
        ToneMap::Filmic.evaluate(0.5, 1.0).to_bits()
    );
}

#[test]
fn fxaa_quality_defaults_and_validation() {
    let q = FxaaQuality::default();
    assert_eq!(q.edge_threshold.to_bits(), 0.1_f32.to_bits());
    assert_eq!(q.minimum_threshold.to_bits(), (1.0_f32 / 64.0).to_bits());
    assert_eq!(q.subpixel_blend.to_bits(), 1.0_f32.to_bits());
    assert_eq!(q.luminance, FxaaLuminance::Linear);
    for value in [-0.1, 1.1, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        for quality in [
            FxaaQuality {
                edge_threshold: value,
                ..q
            },
            FxaaQuality {
                minimum_threshold: value,
                ..q
            },
            FxaaQuality {
                subpixel_blend: value,
                ..q
            },
        ] {
            assert!(
                HdrPostSettings {
                    fxaa_quality: quality,
                    ..Default::default()
                }
                .validate()
                .is_err()
            );
        }
    }
    for value in [0.0, 1.0] {
        assert!(
            HdrPostSettings {
                fxaa_quality: FxaaQuality {
                    edge_threshold: value,
                    minimum_threshold: value,
                    subpixel_blend: value,
                    luminance: FxaaLuminance::Srgb,
                },
                ..Default::default()
            }
            .validate()
            .is_ok()
        );
    }
}
