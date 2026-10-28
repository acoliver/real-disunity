#[test]
fn full_colour_chain_matches_division_oracle_and_legacy_prefix() {
    use super::*;
    for size in [[257, 5], [897, 493], [1, 493], [493, 1], [1, 1]] {
        for rounding in [
            SceneDownsampleRounding::Ceil,
            SceneDownsampleRounding::Floor,
        ] {
            for scale in [500_000, 1_000_000] {
                let legacy = SceneSsr {
                    radiance_rounding: rounding,
                    resolution_scale_micro: scale,
                    ..Default::default()
                };
                let full = SceneSsr {
                    radiance_pyramid: SceneSsrPyramid::Full,
                    ..legacy
                };
                let actual = full.radiance_extents(size);
                let mut divisor = if scale == 500_000 { 2 } else { 1 };
                let mut expected = Vec::new();
                loop {
                    let extent = size.map(|n| match rounding {
                        SceneDownsampleRounding::Ceil => ((n - 1) / divisor + 1).max(1),
                        SceneDownsampleRounding::Floor => (n / divisor).max(1),
                    });
                    expected.push(extent);
                    if extent == [1, 1] {
                        break;
                    }
                    divisor *= 2;
                }
                assert_eq!(actual, expected);
                assert_eq!(
                    &actual[..actual.len().min(6)],
                    legacy.radiance_extents(size)
                );
            }
        }
    }
}

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
        radiance_pyramid: SceneSsrPyramid::Full,
        ..Default::default()
    };
    let value = serde_json::to_value(policy).unwrap();
    assert_eq!(
        serde_json::from_value::<SceneSsr>(value.clone()).unwrap(),
        policy
    );
    for field in ["application_lod", "radiance_rounding", "radiance_pyramid"] {
        let mut invalid = value.clone();
        invalid[field] = serde_json::json!("Unknown");
        assert!(serde_json::from_value::<SceneSsr>(invalid).is_err());
    }
}
