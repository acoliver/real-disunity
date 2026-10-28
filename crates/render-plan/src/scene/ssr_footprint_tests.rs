use super::*;
use crate::SceneDownsampleRounding;

#[test]
fn colour_extents_follow_independent_integer_division_oracle() {
    for size in [[1, 1], [1, 493], [897, 493], [1164, 640]] {
        for rounding in [
            SceneDownsampleRounding::Ceil,
            SceneDownsampleRounding::Floor,
        ] {
            for scale in [500_000, 1_000_000] {
                let policy = SceneSsr {
                    radiance_rounding: rounding,
                    resolution_scale_micro: scale,
                    ..Default::default()
                };
                let extents = policy.radiance_extents(size);
                for (index, actual) in extents.iter().enumerate() {
                    let divisor = (1u32 << index) * if scale == 500_000 { 2 } else { 1 };
                    let expected = size.map(|side| match rounding {
                        SceneDownsampleRounding::Ceil => ((side - 1) / divisor + 1).max(1),
                        SceneDownsampleRounding::Floor => (side / divisor).max(1),
                    });
                    assert_eq!(*actual, expected);
                }
                assert!(extents.len() <= 6);
            }
        }
    }
}

#[test]
fn colour_policies_roundtrip_and_unknown_variants_fail() {
    let policy = SceneSsr {
        application_lod: SceneSsrLodMapping::RoughnessDownsample,
        radiance_rounding: SceneDownsampleRounding::Floor,
        ..Default::default()
    };
    let value = serde_json::to_value(policy).unwrap();
    assert_eq!(
        serde_json::from_value::<SceneSsr>(value.clone()).unwrap(),
        policy
    );
    for field in ["application_lod", "radiance_rounding"] {
        let mut invalid = value.clone();
        invalid[field] = serde_json::json!("Unknown");
        assert!(serde_json::from_value::<SceneSsr>(invalid).is_err());
    }
}
