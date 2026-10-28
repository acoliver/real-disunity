//! Authored camera profile: `real-disunity.camera-profile` version 1.
//!
//! The profile is the one place a game states how its cameras behave: the view convention
//! (handedness), the clip planes and lens of each view mode, the free-fly and RTS tuning, and the
//! input scales. The camera stores a flattened `CameraTuning` copy (the profile's Vec cannot
//! live inside the `Copy` camera), derived once at construction with parse-don't-validate
//! semantics: invalid profiles are rejected, never clamped or defaulted.
//!
//! Three clip authorities exist. Interactive cameras read [`InteractiveView`]; a fixed capture
//! without a pose reads [`FixedCaptureView`]; a pose-bound capture takes its clip planes from the
//! pose, which admission binds by hash, so the profile states none for it.

use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::ViewHandedness;

/// The schema name every camera profile carries.
pub const CAMERA_PROFILE_SCHEMA: &str = "real-disunity.camera-profile";
/// The one schema version this engine reads. Versions are matched exactly.
pub const CAMERA_PROFILE_SCHEMA_VERSION: u32 = 1;

/// Near and far clip planes, meters.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClipPlanes {
    pub near_m: f64,
    pub far_m: f64,
}

/// A render-target size in pixels.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Viewport {
    pub width: u32,
    pub height: u32,
}

/// The view convention the game's cameras present with.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ViewConvention {
    /// Which way screen-right points. A game with a source camera claim authors the source's
    /// handedness; scenes without one author `RightHanded`.
    pub handedness: ViewHandedness,
}

impl ViewConvention {
    /// The scene renderer's existing right-handed raster convention.
    pub const RIGHT_HANDED: Self = Self {
        handedness: ViewHandedness::RightHanded,
    };
}

/// The view block of the interactive RTS and free-fly cameras.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InteractiveView {
    pub clip: ClipPlanes,
}

/// The view block of a fixed capture that no parity pose binds.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FixedCaptureView {
    pub clip: ClipPlanes,
    /// The calibrated capture viewport.
    pub viewport: Viewport,
    /// The calibrated horizontal field of view at the viewport's aspect, degrees.
    pub horizontal_fov_deg: f64,
}

/// Free-flight tuning.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FreeFlyTuning {
    /// Movement speed a free-flight camera starts with, meters per second.
    pub default_speed_meters_per_second: f64,
    /// Clearance kept between the free-flight camera and the ground beneath it, meters.
    pub minimum_ground_clearance_m: f64,
}

/// Authored RTS camera tuning.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RtsTuning {
    /// `MoveSpeedNormal`, meters per second at close zoom.
    pub move_speed_normal: f64,
    /// `MoveSpeedFast`, meters per second at close zoom with the fast key held.
    pub move_speed_fast: f64,
    /// Pan speed fully zoomed out, meters per second.
    pub zoomed_pan_speed: f64,
    /// Fully zoomed-in camera distance, meters.
    pub zoom_min_m: f64,
    /// Fully zoomed-out camera distance, meters.
    pub zoom_max_m: f64,
    /// Maximum pitch depression, degrees below horizon (negative).
    pub pitch_min_deg: f64,
    /// Minimum pitch depression, degrees below horizon (negative).
    pub pitch_max_deg: f64,
    /// Keyboard rotation and tilt speed, radians per second.
    pub rotation_speed_radians_per_second: f64,
    /// Zoom fraction one wheel notch moves.
    pub zoom_step: f64,
    /// Time a zoom transition takes, seconds.
    pub zoom_time_seconds: f64,
    /// Height of the look-at point above the ground target fully zoomed in, meters.
    pub look_at_height_min_m: f64,
    /// Height of the look-at point above the ground target fully zoomed out, meters.
    pub look_at_height_max_m: f64,
    /// Horizontal FOV breakpoints as `(aspect ratio, degrees)` pairs ordered
    /// narrow to wide; the camera lerps between the first and last entries.
    pub hfov_by_aspect: Vec<(f64, f64)>,
}

/// Input scales shared by the free-fly and RTS cameras.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputTuning {
    /// Radians of rotation per unit of raw mouse motion when the run gives no sensitivity.
    pub mouse_sensitivity_radians_per_unit: f64,
    /// Speed multiplier while the slow key is held in free flight.
    pub free_fly_slow_multiplier: f64,
    /// Speed multiplier while the fast key is held in free flight.
    pub free_fly_fast_multiplier: f64,
    /// Pan speed multiplier while the fast key is held in the RTS camera.
    pub rts_fast_pan_multiplier: f64,
    /// Per-notch factor of free-flight movement-speed scaling (greater than 1).
    pub free_fly_wheel_speed_factor: f64,
    /// Seconds of travel one wheel notch is worth when dollying a free-flight camera.
    pub free_fly_wheel_zoom_seconds: f64,
}

/// Authored camera profile.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CameraProfile {
    /// Schema name, [`CAMERA_PROFILE_SCHEMA`].
    pub schema: String,
    /// Schema version, [`CAMERA_PROFILE_SCHEMA_VERSION`].
    pub schema_version: u32,
    /// Stable profile identity, e.g. `rts-v1`.
    pub profile_id: String,
    /// Provenance citation for the values.
    pub source: String,
    pub view_convention: ViewConvention,
    pub interactive: InteractiveView,
    pub fixed_capture: FixedCaptureView,
    pub free_fly: FreeFlyTuning,
    pub rts: RtsTuning,
    pub input: InputTuning,
}

/// Flattened f64 tuning carried inside [`crate::RtsCamera`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct CameraTuning {
    pub move_speed_normal: f64,
    pub move_speed_fast: f64,
    pub zoomed_pan_speed: f64,
    pub zoom_min_m: f64,
    pub zoom_max_m: f64,
    pub pitch_min_radians: f64,
    pub pitch_max_radians: f64,
    pub rotation_speed_radians_per_second: f64,
    pub zoom_step: f64,
    pub zoom_time_seconds: f64,
    pub look_at_height_min_m: f64,
    pub look_at_height_max_m: f64,
    pub narrow_aspect: f64,
    pub wide_aspect: f64,
    pub narrow_hfov_radians: f64,
    pub wide_hfov_radians: f64,
}

/// Profile validation failures.
#[derive(Clone, Debug, PartialEq)]
pub enum CameraProfileError {
    /// `schema` is not [`CAMERA_PROFILE_SCHEMA`].
    WrongSchema {
        /// The schema the document names.
        found: String,
    },
    /// `schema_version` is not [`CAMERA_PROFILE_SCHEMA_VERSION`].
    WrongSchemaVersion {
        /// The version the document names.
        found: u32,
    },
    /// `hfov_by_aspect` must contain at least one breakpoint.
    EmptyFovTable,
    /// Aspect ratios and FOV degrees must be finite and positive.
    InvalidFovBreakpoint {
        /// Index of the offending `(aspect, degrees)` pair.
        index: usize,
    },
    /// Speeds and zoom bounds must be finite and positive.
    InvalidBounds,
    /// Pitch range must be ordered (min below max).
    InvertedPitchRange,
    /// A clip block needs a finite positive near plane below its far plane.
    InvalidClip {
        /// The block that holds the clip planes.
        block: &'static str,
    },
    /// The fixed-capture viewport needs a positive width and height.
    InvalidViewport,
    /// The fixed-capture horizontal field of view must be finite, positive and below a half turn.
    InvalidHorizontalFov,
    /// A tuning value must be finite and positive (and ordered where the field says so).
    InvalidTuning {
        /// The field path in the document.
        field: &'static str,
    },
}

impl std::fmt::Display for CameraProfileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::WrongSchema { found } => write!(
                f,
                "camera profile schema must be {CAMERA_PROFILE_SCHEMA:?}, got {found:?}"
            ),
            Self::WrongSchemaVersion { found } => write!(
                f,
                "camera profile schema_version must be {CAMERA_PROFILE_SCHEMA_VERSION}, got {found}"
            ),
            Self::EmptyFovTable => {
                write!(
                    f,
                    "camera profile needs at least one hfov_by_aspect breakpoint"
                )
            }
            Self::InvalidFovBreakpoint { index } => write!(
                f,
                "camera profile hfov breakpoint {index} is not finite and positive"
            ),
            Self::InvalidBounds => write!(
                f,
                "camera profile speeds and zoom bounds must be finite and positive"
            ),
            Self::InvertedPitchRange => {
                write!(
                    f,
                    "camera profile pitch_min_deg must be below pitch_max_deg"
                )
            }
            Self::InvalidClip { block } => write!(
                f,
                "camera profile {block}.clip needs a finite positive near_m below far_m"
            ),
            Self::InvalidViewport => write!(
                f,
                "camera profile fixed_capture.viewport needs a positive width and height"
            ),
            Self::InvalidHorizontalFov => write!(
                f,
                "camera profile fixed_capture.horizontal_fov_deg must be finite, positive and below a half turn"
            ),
            Self::InvalidTuning { field } => {
                write!(f, "camera profile {field} is not a valid tuning value")
            }
        }
    }
}

impl std::error::Error for CameraProfileError {}

/// Errors produced while loading a [`CameraProfile`] document.
#[derive(Debug)]
pub enum CameraProfileLoadError {
    /// The profile document could not be read from disk.
    Io(std::io::Error),
    /// The profile document is not valid profile JSON.
    Parse(serde_json::Error),
    /// The profile document parsed but breaks a validation rule.
    Invalid(CameraProfileError),
}

impl std::fmt::Display for CameraProfileLoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(f, "could not read camera profile: {error}"),
            Self::Parse(error) => write!(f, "could not parse camera profile: {error}"),
            Self::Invalid(error) => write!(f, "invalid camera profile: {error}"),
        }
    }
}

impl std::error::Error for CameraProfileLoadError {}

fn is_positive(value: f64) -> bool {
    value.is_finite() && value > 0.0
}

fn positive(field: &'static str, value: f64) -> Result<(), CameraProfileError> {
    if is_positive(value) {
        Ok(())
    } else {
        Err(CameraProfileError::InvalidTuning { field })
    }
}

impl ClipPlanes {
    fn validate(self, block: &'static str) -> Result<(), CameraProfileError> {
        if is_positive(self.near_m) && self.far_m.is_finite() && self.near_m < self.far_m {
            Ok(())
        } else {
            Err(CameraProfileError::InvalidClip { block })
        }
    }
}

impl Viewport {
    /// The viewport as `(width, height)`.
    #[must_use]
    pub const fn as_pair(self) -> (u32, u32) {
        (self.width, self.height)
    }
}

impl FixedCaptureView {
    /// The vertical field of view that frames the calibrated horizontal field of view at the
    /// calibrated viewport's aspect, so free flight frames the world like the reference
    /// captures.
    #[must_use]
    pub fn vertical_fov_radians(&self) -> f64 {
        let aspect = f64::from(self.viewport.width) / f64::from(self.viewport.height);
        2.0 * ((self.horizontal_fov_deg.to_radians() * 0.5).tan() / aspect).atan()
    }

    fn validate(self) -> Result<(), CameraProfileError> {
        self.clip.validate("fixed_capture")?;
        if self.viewport.width == 0 || self.viewport.height == 0 {
            return Err(CameraProfileError::InvalidViewport);
        }
        if !is_positive(self.horizontal_fov_deg)
            || self.horizontal_fov_deg.to_radians() >= std::f64::consts::PI
        {
            return Err(CameraProfileError::InvalidHorizontalFov);
        }
        Ok(())
    }
}

impl FreeFlyTuning {
    fn validate(self) -> Result<(), CameraProfileError> {
        positive(
            "free_fly.default_speed_meters_per_second",
            self.default_speed_meters_per_second,
        )?;
        positive(
            "free_fly.minimum_ground_clearance_m",
            self.minimum_ground_clearance_m,
        )
    }
}

impl InputTuning {
    fn validate(self) -> Result<(), CameraProfileError> {
        positive(
            "input.mouse_sensitivity_radians_per_unit",
            self.mouse_sensitivity_radians_per_unit,
        )?;
        positive(
            "input.free_fly_slow_multiplier",
            self.free_fly_slow_multiplier,
        )?;
        positive(
            "input.free_fly_fast_multiplier",
            self.free_fly_fast_multiplier,
        )?;
        positive(
            "input.rts_fast_pan_multiplier",
            self.rts_fast_pan_multiplier,
        )?;
        positive(
            "input.free_fly_wheel_zoom_seconds",
            self.free_fly_wheel_zoom_seconds,
        )?;
        if !self.free_fly_wheel_speed_factor.is_finite() || self.free_fly_wheel_speed_factor <= 1.0
        {
            return Err(CameraProfileError::InvalidTuning {
                field: "input.free_fly_wheel_speed_factor",
            });
        }
        Ok(())
    }
}

impl CameraProfile {
    /// Loads a profile from a JSON document. Unknown or missing fields, a different schema or
    /// version, and any invalid value are errors; the camera never substitutes defaults.
    ///
    /// # Errors
    /// Returns the matching [`CameraProfileLoadError`] variant on failure.
    pub fn load(path: &Path) -> Result<Self, CameraProfileLoadError> {
        let text = std::fs::read_to_string(path).map_err(CameraProfileLoadError::Io)?;
        Self::from_text(&text)
    }

    /// Parses and validates a profile from JSON text.
    ///
    /// # Errors
    /// Returns [`CameraProfileLoadError::Parse`] when the text does not describe a profile and
    /// [`CameraProfileLoadError::Invalid`] when the profile breaks a validation rule.
    pub fn from_text(text: &str) -> Result<Self, CameraProfileLoadError> {
        let profile: Self = serde_json::from_str(text).map_err(CameraProfileLoadError::Parse)?;
        profile
            .validate()
            .map_err(CameraProfileLoadError::Invalid)?;
        Ok(profile)
    }

    /// Checks every rule a profile must satisfy before a camera is built from it.
    ///
    /// # Errors
    /// Returns the [`CameraProfileError`] for the first rule the profile breaks.
    pub fn validate(&self) -> Result<(), CameraProfileError> {
        if self.schema != CAMERA_PROFILE_SCHEMA {
            return Err(CameraProfileError::WrongSchema {
                found: self.schema.clone(),
            });
        }
        if self.schema_version != CAMERA_PROFILE_SCHEMA_VERSION {
            return Err(CameraProfileError::WrongSchemaVersion {
                found: self.schema_version,
            });
        }
        self.interactive.clip.validate("interactive")?;
        self.fixed_capture.validate()?;
        self.free_fly.validate()?;
        self.input.validate()?;
        CameraTuning::from_profile(self)?;
        Ok(())
    }
}

impl CameraTuning {
    /// Derives tuning from an authored profile, rejecting invalid data.
    ///
    /// # Errors
    /// Returns the matching [`CameraProfileError`] variant for invalid input.
    pub(crate) fn from_profile(profile: &CameraProfile) -> Result<Self, CameraProfileError> {
        let rts = &profile.rts;
        validate_rts_ranges(rts)?;
        let ((narrow_aspect, narrow_deg), (wide_aspect, wide_deg)) = fov_endpoints(rts)?;
        Ok(Self {
            move_speed_normal: rts.move_speed_normal,
            move_speed_fast: rts.move_speed_fast,
            zoomed_pan_speed: rts.zoomed_pan_speed,
            zoom_min_m: rts.zoom_min_m,
            zoom_max_m: rts.zoom_max_m,
            pitch_min_radians: rts.pitch_min_deg.to_radians(),
            pitch_max_radians: rts.pitch_max_deg.to_radians(),
            rotation_speed_radians_per_second: rts.rotation_speed_radians_per_second,
            zoom_step: rts.zoom_step,
            zoom_time_seconds: rts.zoom_time_seconds,
            look_at_height_min_m: rts.look_at_height_min_m,
            look_at_height_max_m: rts.look_at_height_max_m,
            narrow_aspect,
            wide_aspect,
            narrow_hfov_radians: narrow_deg.to_radians(),
            wide_hfov_radians: wide_deg.to_radians(),
        })
    }
}

/// Rejects RTS speeds, zoom, pitch and look-at ranges that are not finite, positive and ordered.
fn validate_rts_ranges(rts: &RtsTuning) -> Result<(), CameraProfileError> {
    let bounds = [
        rts.move_speed_normal,
        rts.move_speed_fast,
        rts.zoom_min_m,
        rts.zoom_max_m,
    ];
    if bounds
        .iter()
        .any(|value| !value.is_finite() || *value <= 0.0)
        || rts.zoom_min_m >= rts.zoom_max_m
    {
        return Err(CameraProfileError::InvalidBounds);
    }
    if !rts.pitch_min_deg.is_finite()
        || !rts.pitch_max_deg.is_finite()
        || rts.pitch_min_deg >= rts.pitch_max_deg
    {
        return Err(CameraProfileError::InvertedPitchRange);
    }
    positive("rts.zoomed_pan_speed", rts.zoomed_pan_speed)?;
    positive(
        "rts.rotation_speed_radians_per_second",
        rts.rotation_speed_radians_per_second,
    )?;
    positive("rts.zoom_time_seconds", rts.zoom_time_seconds)?;
    positive("rts.look_at_height_min_m", rts.look_at_height_min_m)?;
    if !is_positive(rts.zoom_step) || rts.zoom_step > 1.0 {
        return Err(CameraProfileError::InvalidTuning {
            field: "rts.zoom_step",
        });
    }
    if !rts.look_at_height_max_m.is_finite() || rts.look_at_height_min_m >= rts.look_at_height_max_m
    {
        return Err(CameraProfileError::InvalidTuning {
            field: "rts.look_at_height_max_m",
        });
    }
    Ok(())
}

/// An `(aspect ratio, degrees)` breakpoint of the horizontal-FOV table.
type FovBreakpoint = (f64, f64);

/// The narrowest and widest breakpoints of the horizontal-FOV table, ordered by aspect.
fn fov_endpoints(rts: &RtsTuning) -> Result<(FovBreakpoint, FovBreakpoint), CameraProfileError> {
    if rts.hfov_by_aspect.is_empty() {
        return Err(CameraProfileError::EmptyFovTable);
    }
    for (index, (aspect, degrees)) in rts.hfov_by_aspect.iter().enumerate() {
        if !aspect.is_finite() || *aspect <= 0.0 || !degrees.is_finite() || *degrees <= 0.0 {
            return Err(CameraProfileError::InvalidFovBreakpoint { index });
        }
    }
    let mut ordered = rts.hfov_by_aspect.clone();
    ordered.sort_by(|a, b| a.0.total_cmp(&b.0));
    Ok((ordered[0], ordered[ordered.len() - 1]))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Original synthetic fixture, read through the supported profile loader.
    fn authored_text() -> String {
        std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/camera-profile-rts-v1.json"),
        )
        .expect("authored camera profile document")
    }

    fn authored_profile() -> CameraProfile {
        CameraProfile::from_text(&authored_text()).expect("authored camera profile loads")
    }

    /// Authored decimal values must survive parsing without adjustment.
    fn same(actual: f64, expected: f64) {
        assert_eq!(
            actual.to_bits(),
            expected.to_bits(),
            "{actual} != {expected}"
        );
    }

    #[test]
    fn original_synthetic_profile_loads_without_defaults() {
        let profile = authored_profile();
        assert_eq!(profile.schema, CAMERA_PROFILE_SCHEMA);
        assert_eq!(profile.schema_version, CAMERA_PROFILE_SCHEMA_VERSION);
        assert_eq!(profile.profile_id, "synthetic-orbit-v1");
        assert_eq!(
            profile.view_convention.handedness,
            ViewHandedness::RightHanded
        );
        same(profile.interactive.clip.near_m, 0.25);
        same(profile.fixed_capture.horizontal_fov_deg, 76.0);
        assert_eq!(profile.fixed_capture.viewport.as_pair(), (960, 720));
        same(profile.rts.move_speed_normal, 6.0);
        same(profile.rts.move_speed_fast, 18.0);
        same(profile.rts.zoom_min_m, 6.0);
        same(profile.rts.zoom_max_m, 72.0);
        assert_eq!(
            profile.rts.hfov_by_aspect,
            vec![(1.0, 60.0), (1.5, 80.0), (2.0, 100.0)]
        );
    }

    #[test]
    fn calibrated_free_fly_fov_frames_the_calibrated_horizontal_fov() {
        let fixed = authored_profile().fixed_capture;
        let aspect = f64::from(fixed.viewport.width) / f64::from(fixed.viewport.height);
        let horizontal = 2.0 * ((fixed.vertical_fov_radians() * 0.5).tan() * aspect).atan();
        assert!((horizontal - fixed.horizontal_fov_deg.to_radians()).abs() < 1.0e-12);
    }

    #[test]
    fn unknown_fields_are_errors_not_defaults() {
        let mutated = authored_text().replace("\"profile_id\"", "\"typo_id\"");
        let error = CameraProfile::from_text(&mutated).expect_err("renamed field must not parse");
        assert!(error.to_string().contains("typo_id"), "{error}");
    }

    #[test]
    fn a_missing_block_is_an_error_not_a_default() {
        let mutated = authored_text().replace("\"view_convention\"", "\"view_convention_x\"");
        assert!(matches!(
            CameraProfile::from_text(&mutated),
            Err(CameraProfileLoadError::Parse(_))
        ));
    }

    #[test]
    fn a_handedness_outside_the_enum_is_a_parse_error() {
        let mutated = authored_text().replace("RightHanded", "Default");
        assert!(matches!(
            CameraProfile::from_text(&mutated),
            Err(CameraProfileLoadError::Parse(_))
        ));
    }

    #[test]
    fn a_different_schema_or_version_is_rejected() {
        let mut profile = authored_profile();
        profile.schema = "other.camera-profile".to_owned();
        assert_eq!(
            profile.validate(),
            Err(CameraProfileError::WrongSchema {
                found: "other.camera-profile".to_owned()
            })
        );
        let mut profile = authored_profile();
        profile.schema_version = 0;
        assert_eq!(
            profile.validate(),
            Err(CameraProfileError::WrongSchemaVersion { found: 0 })
        );
    }

    #[test]
    fn invalid_profiles_are_rejected_not_clamped() {
        let base = authored_profile();

        let mut empty = base.clone();
        empty.rts.hfov_by_aspect = Vec::new();
        assert_eq!(empty.validate(), Err(CameraProfileError::EmptyFovTable));

        let mut bad_fov = base.clone();
        bad_fov.rts.hfov_by_aspect = vec![(1.777_777_777_777_777_7, -90.0)];
        assert_eq!(
            bad_fov.validate(),
            Err(CameraProfileError::InvalidFovBreakpoint { index: 0 })
        );

        let mut inverted = base.clone();
        inverted.rts.pitch_min_deg = -10.0;
        inverted.rts.pitch_max_deg = -60.0;
        assert_eq!(
            inverted.validate(),
            Err(CameraProfileError::InvertedPitchRange)
        );

        let mut bounds = base.clone();
        bounds.rts.zoom_min_m = bounds.rts.zoom_max_m;
        assert_eq!(bounds.validate(), Err(CameraProfileError::InvalidBounds));
    }

    #[test]
    fn clip_planes_must_be_ordered_and_positive_in_every_block() {
        let mut interactive = authored_profile();
        interactive.interactive.clip.near_m = interactive.interactive.clip.far_m;
        assert_eq!(
            interactive.validate(),
            Err(CameraProfileError::InvalidClip {
                block: "interactive"
            })
        );
        let mut fixed = authored_profile();
        fixed.fixed_capture.clip.near_m = 0.0;
        assert_eq!(
            fixed.validate(),
            Err(CameraProfileError::InvalidClip {
                block: "fixed_capture"
            })
        );
    }

    #[test]
    fn the_fixed_capture_lens_must_be_realisable() {
        let mut viewport = authored_profile();
        viewport.fixed_capture.viewport.height = 0;
        assert_eq!(
            viewport.validate(),
            Err(CameraProfileError::InvalidViewport)
        );
        let mut fov = authored_profile();
        fov.fixed_capture.horizontal_fov_deg = 180.0;
        assert_eq!(
            fov.validate(),
            Err(CameraProfileError::InvalidHorizontalFov)
        );
    }

    #[test]
    fn tuning_values_must_be_finite_and_positive() {
        let mut speed = authored_profile();
        speed.free_fly.default_speed_meters_per_second = 0.0;
        assert_eq!(
            speed.validate(),
            Err(CameraProfileError::InvalidTuning {
                field: "free_fly.default_speed_meters_per_second"
            })
        );
        let mut wheel = authored_profile();
        wheel.input.free_fly_wheel_speed_factor = 1.0;
        assert_eq!(
            wheel.validate(),
            Err(CameraProfileError::InvalidTuning {
                field: "input.free_fly_wheel_speed_factor"
            })
        );
        let mut heights = authored_profile();
        heights.rts.look_at_height_max_m = heights.rts.look_at_height_min_m;
        assert_eq!(
            heights.validate(),
            Err(CameraProfileError::InvalidTuning {
                field: "rts.look_at_height_max_m"
            })
        );
        let mut step = authored_profile();
        step.rts.zoom_step = 1.5;
        assert_eq!(
            step.validate(),
            Err(CameraProfileError::InvalidTuning {
                field: "rts.zoom_step"
            })
        );
    }

    #[test]
    fn reaches_consumer_schema() {
        let mut profile = authored_profile();
        profile.schema = "other".to_owned();
        let error = serde_json::to_string(&profile)
            .map(|text| CameraProfile::from_text(&text))
            .expect("profile serializes")
            .expect_err("a different schema must not load");
        assert!(error.to_string().contains("schema"), "{error}");
    }

    #[test]
    fn reaches_consumer_schema_version() {
        let mut profile = authored_profile();
        profile.schema_version = CAMERA_PROFILE_SCHEMA_VERSION + 1;
        let error = serde_json::to_string(&profile)
            .map(|text| CameraProfile::from_text(&text))
            .expect("profile serializes")
            .expect_err("a different schema_version must not load");
        assert!(error.to_string().contains("schema_version"), "{error}");
    }
}
