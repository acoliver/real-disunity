//! The depth range of a player run: the one setting that the GPU side and the managed side share.
//!
//! A [`DepthRange`] says whether depth is reversed, with near at 1 and far at 0. One value is
//! chosen for a run and travels with it:
//!
//! * [`PlayerLaunch::depth`](crate::PlayerLaunch) holds it. `prepare_player` writes it into the
//!   bridge's request as `reversed_z_buffer`, so the managed `PlayerDevice` declares a reversed
//!   depth buffer exactly when the host draws with one, and keeps it in
//!   [`PreparedPlayer::depth`](crate::PreparedPlayer).
//! * [`Settings::depth`](super::Settings) holds it on the GPU side. `PlayerGpu` puts the run's
//!   value there, [`OffscreenHost`](super::OffscreenHost) tells its render-program state
//!   ([`set_reversed_depth`](worldrunner_render_program::unity::offscreen::OffscreenState::set_reversed_depth))
//!   to clear depth to the far value and to reverse depth tests and the shadow comparison, and
//!   the adapter builds every camera's clip space with [`clip_from_gl`](super::clip_from_gl) for
//!   the same value, so depth written through the host's matrices follows the state's convention.
//!
//! A built player run by `play` is [`DepthRange::PLAYER`], reversed. The evidence is an Old World
//! RomeTurn42 run, frame 16 (board 133, posts 3214, 3220 and 3238). With forward depth (clear 1,
//! `ZTest LEqual` as `LessEqual`) every camera texture held one value across the screen: the
//! normals were exactly (0, 0, -1, 0) and the depth 0.0076. With reversed depth (clear 0, depth
//! tests and the shadow comparison reversed, the host projection mapping near to w and far to 0)
//! and the request field declaring it to the managed side, the normals varied across the screen
//! (mean z -0.99) and real geometry won the depth test.
//!
//! `play` takes an explicit choice from `--depth reversed|forward` and from the environment
//! variable [`REVERSED_DEPTH_ENV`] (`0` or `false` for forward, `1` or `true` for reversed); the
//! command line wins. Only `play`'s `main` reads the environment, once, and [`DepthRange::choose`]
//! turns the two inputs into the run's value. Everything below receives the value it is given: a
//! library caller that builds [`Settings`](super::Settings) or an [`OffscreenState`] directly
//! stays forward, which is [`DepthRange::default`].
//!
//! [`OffscreenState`]: worldrunner_render_program::unity::offscreen::OffscreenState

/// The environment variable that overrides the depth range of a player run.
pub const REVERSED_DEPTH_ENV: &str = "RD_REVERSED_DEPTH";

/// Whether the depth of a run is reversed: near is 1 and far is 0.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DepthRange {
    /// Near is 0 and far is 1: depth clears to 1 and `ZTest LEqual` is `LessEqual`. This is what
    /// the render-program state and [`Settings::default`](super::Settings) do unless told
    /// otherwise.
    #[default]
    Forward,
    /// Near is 1 and far is 0: depth clears to 0 and `ZTest LEqual` is `GreaterEqual`.
    Reversed,
}

impl DepthRange {
    /// The depth range of a built player run by `play`, when the run does not choose one.
    pub const PLAYER: Self = Self::Reversed;

    /// Whether near is 1 and far is 0.
    #[must_use]
    pub const fn reversed(self) -> bool {
        matches!(self, Self::Reversed)
    }

    /// The range that `reversed` stands for.
    #[must_use]
    pub const fn from_reversed(reversed: bool) -> Self {
        if reversed {
            Self::Reversed
        } else {
            Self::Forward
        }
    }

    /// The range an environment variable's contents name: `1` or `true` is reversed, `0` or
    /// `false` is forward, and anything else names none.
    #[must_use]
    pub fn from_flag(text: &str) -> Option<Self> {
        match text.trim() {
            "1" | "true" => Some(Self::Reversed),
            "0" | "false" => Some(Self::Forward),
            _ => None,
        }
    }

    /// The range the command line option `--depth` names: `reversed` or `forward`.
    #[must_use]
    pub fn from_name(text: &str) -> Option<Self> {
        match text {
            "reversed" => Some(Self::Reversed),
            "forward" => Some(Self::Forward),
            _ => None,
        }
    }

    /// The depth range of a run: `option` (the command line) when it names one, else the
    /// environment variable's contents `environment` when it is set, else [`DepthRange::PLAYER`].
    ///
    /// # Errors
    /// Fails with a message when the environment variable is set to something that is neither a
    /// `0`, `false`, `1` nor `true` and the command line names none.
    pub fn choose(option: Option<Self>, environment: Option<&str>) -> Result<Self, String> {
        if let Some(chosen) = option {
            return Ok(chosen);
        }
        match environment {
            None => Ok(Self::PLAYER),
            Some(text) => Self::from_flag(text)
                .ok_or_else(|| format!("{REVERSED_DEPTH_ENV} `{text}` is 0, false, 1 or true")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::DepthRange;

    #[test]
    fn a_built_player_is_reversed_and_a_library_default_is_forward() {
        assert_eq!(DepthRange::PLAYER, DepthRange::Reversed);
        assert_eq!(DepthRange::default(), DepthRange::Forward);
    }

    #[test]
    fn the_environment_names_a_range_by_one_zero_true_or_false() {
        assert_eq!(DepthRange::from_flag("1"), Some(DepthRange::Reversed));
        assert_eq!(DepthRange::from_flag("true"), Some(DepthRange::Reversed));
        assert_eq!(DepthRange::from_flag(" 0\n"), Some(DepthRange::Forward));
        assert_eq!(DepthRange::from_flag("false"), Some(DepthRange::Forward));
        assert_eq!(DepthRange::from_flag(""), None);
        assert_eq!(DepthRange::from_flag("yes"), None);
    }

    #[test]
    fn the_option_names_a_range_by_its_name() {
        assert_eq!(
            DepthRange::from_name("reversed"),
            Some(DepthRange::Reversed)
        );
        assert_eq!(DepthRange::from_name("forward"), Some(DepthRange::Forward));
        assert_eq!(DepthRange::from_name("1"), None);
    }

    #[test]
    fn a_run_is_reversed_unless_the_option_or_the_environment_says_forward() {
        assert_eq!(DepthRange::choose(None, None), Ok(DepthRange::Reversed));
        assert_eq!(DepthRange::choose(None, Some("0")), Ok(DepthRange::Forward));
        assert_eq!(
            DepthRange::choose(None, Some("1")),
            Ok(DepthRange::Reversed)
        );
        assert!(DepthRange::choose(None, Some("sideways")).is_err());
    }

    #[test]
    fn the_option_wins_over_the_environment() {
        let forward = Some(DepthRange::Forward);
        let reversed = Some(DepthRange::Reversed);
        assert_eq!(
            DepthRange::choose(forward, Some("1")),
            Ok(DepthRange::Forward)
        );
        assert_eq!(
            DepthRange::choose(reversed, Some("0")),
            Ok(DepthRange::Reversed)
        );
        // The option decides, so an environment value that would be an error is not read.
        assert_eq!(
            DepthRange::choose(forward, Some("sideways")),
            Ok(DepthRange::Forward)
        );
    }

    #[test]
    fn the_flag_round_trips() {
        for range in [DepthRange::Forward, DepthRange::Reversed] {
            assert_eq!(DepthRange::from_reversed(range.reversed()), range);
        }
    }
}
