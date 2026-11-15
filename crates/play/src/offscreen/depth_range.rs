//! The one switch for reversed offscreen depth: near is 1 and far is 0.
//!
//! This is an experiment switch. It is off unless the environment variable
//! [`REVERSED_DEPTH_ENV`] is `1` or `true`, and it is read once, at the first use. When it is on,
//! [`OffscreenHost`](super::OffscreenHost) tells its render-program state to clear depth to the
//! far value, to reverse depth tests and the shadow comparison, and [`clip_from_gl`](super::clip_from_gl)
//! builds the clip space to match, so depth written through the host's matrices follows the same
//! convention.

use std::sync::OnceLock;

/// The environment variable that turns reversed depth on.
pub const REVERSED_DEPTH_ENV: &str = "RD_REVERSED_DEPTH";

/// Whether `value`, the variable's contents, turns reversed depth on.
fn parse(value: Option<&str>) -> bool {
    matches!(value.map(str::trim), Some("1" | "true"))
}

/// Whether offscreen depth is reversed in this process. The environment is read on the first call.
#[must_use]
pub fn reversed_depth() -> bool {
    static REVERSED: OnceLock<bool> = OnceLock::new();
    *REVERSED.get_or_init(|| parse(std::env::var(REVERSED_DEPTH_ENV).ok().as_deref()))
}

#[cfg(test)]
mod tests {
    use super::parse;

    #[test]
    fn only_one_and_true_turn_it_on() {
        assert!(!parse(None));
        assert!(!parse(Some("")));
        assert!(!parse(Some("0")));
        assert!(!parse(Some("false")));
        assert!(parse(Some("1")));
        assert!(parse(Some("true")));
        assert!(parse(Some(" 1\n")));
    }
}
