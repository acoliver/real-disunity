//! Portable generated yaw: integer Q56 Taylor evaluation, then nearest Q40
//! components (ties away from zero). No host transcendental or normalization.

use worldrunner_world_schema::EnvironmentOrientation;

const FIXED_ONE: i128 = 1_i128 << 56;
const OUTPUT_QUANTUM: i128 = 1_i128 << 16;
const OUTPUT_ONE: f64 = 1_099_511_627_776.0;

pub(super) fn from_draw(random: u64) -> EnvironmentOrientation {
    // Preserve the original hash draw and microradian range, including its wrap.
    let yaw = random % 6_283_185;
    let angle = i128::from(yaw) * FIXED_ONE / 2_000_000;
    let square = angle * angle / FIXED_ONE;
    let (mut sine_term, mut cosine_term) = (angle, FIXED_ONE);
    let (mut sine, mut cosine) = (sine_term, cosine_term);
    // |angle| < pi: 17 terms bound truncation below 4e-22; Q56 arithmetic
    // error is below the Q40 quantum. Products fit i128 (less than 2^120).
    for term in 1..=16 {
        let twice = 2 * term;
        sine_term = -sine_term * square / FIXED_ONE / (twice * (twice + 1));
        cosine_term = -cosine_term * square / FIXED_ONE / ((twice - 1) * twice);
        sine += sine_term;
        cosine += cosine_term;
    }
    EnvironmentOrientation {
        quaternion_xyzw: [0.0, component(sine), 0.0, component(cosine)],
    }
}

#[allow(
    clippy::cast_precision_loss,
    reason = "Q40 unit components have at most 41 significant bits, exactly representable in f64"
)]
fn component(value: i128) -> f64 {
    let rounded = (value + value.signum() * (OUTPUT_QUANTUM / 2)) / OUTPUT_QUANTUM;
    let numerator = i64::try_from(rounded).expect("Q40 unit component fits in i64");
    numerator as f64 / OUTPUT_ONE
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn yaw_has_platform_independent_numeric_vectors() {
        // Independent 85-digit decimal Taylor evaluation, rounded to Q40.
        for (yaw, sine, cosine) in [
            (0, 0.0, 1_099_511_627_776.0),
            (1, 549_756.0, 1_099_511_627_776.0),
            (1_000_000, 527_133_954_348.0, 964_912_231_132.0),
            (1_570_796, 777_472_000_957.0, 777_472_255_031.0),
            (3_141_592, 1_099_511_627_776.0, 359_315.0),
            (4_712_389, 777_472_120_369.0, -777_472_135_619.0),
            (6_283_184, 718_630.0, -1_099_511_627_776.0),
        ] {
            let expected = [
                0.0,
                sine / 1_099_511_627_776.0,
                0.0,
                cosine / 1_099_511_627_776.0,
            ];
            assert_eq!(
                from_draw(yaw).quaternion_xyzw.map(f64::to_bits),
                expected.map(f64::to_bits),
                "yaw {yaw} microradians"
            );
        }
    }

    #[test]
    fn quantized_rotations_remain_unit_yaw_rotations_across_the_turn() {
        for yaw in (0..6_283_185).step_by(997).chain([6_283_184]) {
            let orientation = from_draw(yaw);
            assert_eq!(orientation.validate(), Ok(()), "yaw {yaw}");
            let expected = EnvironmentOrientation::from_yaw_radians(
                f64::from(u32::try_from(yaw).unwrap()) / 1_000_000.0,
            );
            for (actual, expected) in orientation
                .quaternion_xyzw
                .into_iter()
                .zip(expected.quaternion_xyzw)
            {
                assert!((actual - expected).abs() < 1e-12, "yaw {yaw}");
            }
        }
    }

    #[test]
    fn draw_reduction_wraps_and_handles_all_hash_bits() {
        assert_eq!(from_draw(6_283_185), EnvironmentOrientation::IDENTITY);
        assert_eq!(from_draw(u64::MAX), from_draw(u64::MAX % 6_283_185));
    }
}
