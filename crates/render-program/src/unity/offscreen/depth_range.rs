//! The depth range convention of offscreen draws, in one place.
//!
//! By default depth is not reversed: a depth clear to 1 is the far value and `ZTest LEqual` is
//! `LessEqual`. With the host's `reversed_depth` switch on (see
//! [`OffscreenState::set_reversed_depth`](super::OffscreenState::set_reversed_depth)) near is 1
//! and far is 0, so a depth clear value `d` is stored as `1 - d`, a depth test compares the
//! other way round and a shadow lookup's comparison flips with it. The host must then build
//! its projections to map near to 1 and far to 0 as well.

/// The function a depth test compares by when a pass asks for `test`.
pub(super) fn depth_test(test: wgpu::CompareFunction, reversed: bool) -> wgpu::CompareFunction {
    use wgpu::CompareFunction as F;
    if !reversed {
        return test;
    }
    match test {
        F::Less => F::Greater,
        F::LessEqual => F::GreaterEqual,
        F::Greater => F::Less,
        F::GreaterEqual => F::LessEqual,
        other => other,
    }
}

/// The value a depth clear to `depth` stores.
pub(super) fn depth_clear(depth: f32, reversed: bool) -> f32 {
    if reversed { 1.0 - depth } else { depth }
}

/// The function a comparison sampler compares a reference depth with the stored depth by.
pub(super) fn shadow_compare(reversed: bool) -> wgpu::CompareFunction {
    depth_test(super::sampling::SHADOW_COMPARE, reversed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use wgpu::CompareFunction as F;

    #[test]
    fn off_leaves_everything_as_it_is() {
        for test in [
            F::Never,
            F::Less,
            F::Equal,
            F::LessEqual,
            F::Greater,
            F::NotEqual,
            F::GreaterEqual,
            F::Always,
        ] {
            assert_eq!(depth_test(test, false), test);
        }
        assert!((depth_clear(1.0, false) - 1.0).abs() < f32::EPSILON);
        assert_eq!(shadow_compare(false), F::LessEqual);
    }

    #[test]
    fn on_swaps_the_ordered_comparisons_and_clears_to_the_far_value() {
        assert_eq!(depth_test(F::Less, true), F::Greater);
        assert_eq!(depth_test(F::LessEqual, true), F::GreaterEqual);
        assert_eq!(depth_test(F::Greater, true), F::Less);
        assert_eq!(depth_test(F::GreaterEqual, true), F::LessEqual);
        for same in [F::Never, F::Equal, F::NotEqual, F::Always] {
            assert_eq!(depth_test(same, true), same);
        }
        assert!(depth_clear(1.0, true).abs() < f32::EPSILON);
        assert_eq!(shadow_compare(true), F::GreaterEqual);
    }
}
