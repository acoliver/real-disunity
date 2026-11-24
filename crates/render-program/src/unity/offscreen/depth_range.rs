//! The depth range convention of offscreen draws, in one place.
//!
//! Depth is reversed: near is 1 and far is 0. A depth clear value `d` is stored as `1 - d`, a
//! depth test compares the other way round from the order the script states, and a shadow
//! lookup's comparison flips with it. The host builds its projections to map near to 1 and far
//! to 0 as well.

/// The function a depth test compares by when a pass asks for `test`.
pub(super) fn depth_test(test: wgpu::CompareFunction) -> wgpu::CompareFunction {
    use wgpu::CompareFunction as F;
    match test {
        F::Less => F::Greater,
        F::LessEqual => F::GreaterEqual,
        F::Greater => F::Less,
        F::GreaterEqual => F::LessEqual,
        other => other,
    }
}

/// The value a depth clear to `depth` stores.
pub(super) fn depth_clear(depth: f32) -> f32 {
    1.0 - depth
}

#[cfg(test)]
mod tests {
    use super::*;
    use wgpu::CompareFunction as F;

    #[test]
    fn the_ordered_comparisons_swap_and_the_clear_is_the_far_value() {
        assert_eq!(depth_test(F::Less), F::Greater);
        assert_eq!(depth_test(F::LessEqual), F::GreaterEqual);
        assert_eq!(depth_test(F::Greater), F::Less);
        assert_eq!(depth_test(F::GreaterEqual), F::LessEqual);
        for same in [F::Never, F::Equal, F::NotEqual, F::Always] {
            assert_eq!(depth_test(same), same);
        }
        assert!(depth_clear(1.0).abs() < f32::EPSILON);
        assert!((depth_clear(0.25) - 0.75).abs() < f32::EPSILON);
    }
}
