use glam::{DQuat, DVec3, Mat4, Vec3};
use std::f64::consts::{FRAC_PI_2, PI};
use std::time::Duration;

mod controller_view;
mod handedness;
mod resolved;
pub use resolved::{
    CameraLens, CameraPose, CameraState, DepthConvention, ResolvedView, ScreenRay, ViewError,
    resolve,
};
pub mod profile;
pub use handedness::ViewHandedness;
pub use profile::{
    CAMERA_PROFILE_SCHEMA, CAMERA_PROFILE_SCHEMA_VERSION, CameraProfile, CameraProfileError,
    CameraProfileLoadError, ClipPlanes, FixedCaptureView, FreeFlyTuning, InputTuning,
    InteractiveView, RtsTuning, ViewConvention, Viewport,
};

use profile::CameraTuning;

const MIN_PITCH_MARGIN_RADIANS: f64 = 0.001;
const MIN_FOV_RADIANS: f64 = 20.0_f64.to_radians();
const MAX_FOV_RADIANS: f64 = 120.0_f64.to_radians();
const MIN_SPEED_METERS_PER_SECOND: f64 = 0.25;
const MAX_SPEED_METERS_PER_SECOND: f64 = 8_192.0;
const MIN_NEAR_PLANE_METERS: f64 = 0.01;
const MAX_ELAPSED_SECONDS: f64 = 0.25;
pub const RTS_TARGET_TERRAIN_CLEARANCE_METERS: f64 = 0.25;
pub const RTS_EYE_TERRAIN_CLEARANCE_METERS: f64 = 1.0;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FreeFlyInput {
    pub forward: f64,
    pub right: f64,
    pub up: f64,
    pub yaw_delta_radians: f64,
    pub pitch_delta_radians: f64,
    pub speed_multiplier: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FreeFlyCamera {
    pub position: DVec3,
    pub yaw_radians: f64,
    pub pitch_radians: f64,
    pub vertical_fov_radians: f64,
    pub movement_speed_meters_per_second: f64,
    pub handedness: ViewHandedness,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CameraView {
    pub view_projection: Mat4,
    pub position: Vec3,
}

impl FreeFlyCamera {
    /// A free-flight camera at `position` looking along `yaw_radians`/`pitch_radians`, presenting
    /// with the profile's handedness and starting at its default speed.
    #[must_use]
    pub fn new(
        profile: &CameraProfile,
        position: DVec3,
        yaw_radians: f64,
        pitch_radians: f64,
        vertical_fov_radians: f64,
    ) -> Self {
        Self {
            position,
            yaw_radians,
            pitch_radians,
            vertical_fov_radians,
            movement_speed_meters_per_second: profile.free_fly.default_speed_meters_per_second,
            handedness: profile.view_convention.handedness,
        }
    }

    pub fn update(&mut self, input: FreeFlyInput, elapsed: Duration) {
        let screen_sign = self.handedness.screen_sign();
        self.yaw_radians =
            wrap_angle(self.yaw_radians + screen_sign * finite_or_zero(input.yaw_delta_radians));
        self.pitch_radians = (self.pitch_radians + finite_or_zero(input.pitch_delta_radians))
            .clamp(
                -FRAC_PI_2 + MIN_PITCH_MARGIN_RADIANS,
                FRAC_PI_2 - MIN_PITCH_MARGIN_RADIANS,
            );

        let orientation = self.orientation();
        let forward = orientation * DVec3::NEG_Z;
        let right = orientation * DVec3::X * screen_sign;
        let up = DVec3::Y;
        let movement = forward * finite_or_zero(input.forward)
            + right * finite_or_zero(input.right)
            + up * finite_or_zero(input.up);
        let direction = movement.try_normalize().unwrap_or(DVec3::ZERO);
        let multiplier = if input.speed_multiplier.is_finite() && input.speed_multiplier > 0.0 {
            input.speed_multiplier
        } else {
            1.0
        };
        let elapsed_seconds = elapsed.as_secs_f64().min(MAX_ELAPSED_SECONDS);
        self.position +=
            direction * self.movement_speed_meters_per_second * multiplier * elapsed_seconds;
    }

    pub fn orientation(self) -> DQuat {
        DQuat::from_rotation_y(self.yaw_radians) * DQuat::from_rotation_x(self.pitch_radians)
    }

    pub fn forward(self) -> DVec3 {
        self.orientation() * DVec3::NEG_Z
    }

    pub fn set_vertical_fov_radians(&mut self, fov: f64) {
        if fov.is_finite() {
            self.vertical_fov_radians = fov.clamp(MIN_FOV_RADIANS, MAX_FOV_RADIANS);
        }
    }

    pub fn scale_movement_speed(&mut self, factor: f64) {
        if factor.is_finite() && factor > 0.0 {
            self.movement_speed_meters_per_second = (self.movement_speed_meters_per_second
                * factor)
                .clamp(MIN_SPEED_METERS_PER_SECOND, MAX_SPEED_METERS_PER_SECOND);
        }
    }

    /// Moves the eye along the view direction by `meters`, positive being forward.
    ///
    /// A free-fly camera has no orbit target whose distance could be shortened, so a wheel
    /// "zoom" has to be a dolly. The field of view is deliberately left alone: narrowing it
    /// would magnify without approaching, which misrepresents parallax and would make the
    /// world look wrong while inspecting it.
    pub fn dolly(&mut self, meters: f64) {
        let meters = finite_or_zero(meters);
        if meters != 0.0 {
            self.position += self.forward() * meters;
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RtsCameraInput {
    pub forward: f64,
    pub right: f64,
    pub rotate: f64,
    pub tilt: f64,
    pub yaw_delta_radians: f64,
    pub pitch_delta_radians: f64,
    pub speed_multiplier: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RtsCamera {
    pub ground_target: DVec3,
    pub yaw_radians: f64,
    pub pitch_radians: f64,
    pub zoom: f64,
    pub target_zoom: f64,
    pub handedness: ViewHandedness,
    tuning: CameraTuning,
    zoom_start: f64,
    zoom_elapsed_seconds: f64,
}

impl RtsCamera {
    /// Constructs a camera driven by an authored profile, presenting with the profile's
    /// handedness. Invalid profiles are rejected; the camera never silently clamps authored data.
    ///
    /// # Errors
    /// Returns the [`profile::CameraProfileError`] describing the rejected
    /// field when the profile cannot be validated.
    pub fn new(profile: &CameraProfile) -> Result<Self, profile::CameraProfileError> {
        let tuning = CameraTuning::from_profile(profile)?;
        let zoom = 0.5;
        Ok(Self {
            ground_target: DVec3::ZERO,
            yaw_radians: 0.0,
            pitch_radians: tuning.pitch_max_radians,
            zoom,
            target_zoom: zoom,
            handedness: profile.view_convention.handedness,
            tuning,
            zoom_start: zoom,
            zoom_elapsed_seconds: tuning.zoom_time_seconds,
        })
    }

    /// Returns this camera presenting with `handedness`. The pose is untouched; only the screen-right
    /// sign of the view and of the screen-relative inputs changes.
    #[must_use]
    pub fn with_handedness(mut self, handedness: ViewHandedness) -> Self {
        self.handedness = handedness;
        self
    }

    /// Derives an RTS camera pose from a free-flight eye position and yaw/pitch so the initial
    /// view matches as closely as the RTS pitch clamp permits, driven by an authored profile.
    ///
    /// The eye position is preserved exactly: `ground_target` is solved so that `eye()`
    /// returns the requested eye. The yaw is copied verbatim; the pitch is clamped to the
    /// RTS range. A mid-range zoom (0.5) is chosen as a reasonable starting distance, and
    /// `follow_terrain` subsequently grounds the look-at target.
    ///
    /// # Errors
    /// Returns the [`profile::CameraProfileError`] describing the rejected field when the
    /// profile cannot be validated.
    pub fn from_free_fly_pose(
        eye: DVec3,
        yaw_radians: f64,
        pitch_radians: f64,
        profile: &CameraProfile,
    ) -> Result<Self, profile::CameraProfileError> {
        let mut camera = Self::new(profile)?;
        camera.yaw_radians = wrap_angle(yaw_radians);
        camera.pitch_radians = pitch_radians.clamp(
            camera.tuning.pitch_min_radians,
            camera.tuning.pitch_max_radians,
        );
        // eye = look_at - forward * distance and look_at = ground_target + Y * look_at_height,
        // so ground_target = eye + forward * distance - Y * look_at_height.
        let distance = camera.distance_meters();
        let look_at_height = camera.look_at_height_meters();
        let forward = camera.forward();
        camera.ground_target = eye + forward * distance - DVec3::Y * look_at_height;
        Ok(camera)
    }

    pub fn update(&mut self, input: RtsCameraInput, elapsed: Duration) {
        let elapsed_seconds = elapsed.as_secs_f64().min(MAX_ELAPSED_SECONDS);
        let screen_sign = self.handedness.screen_sign();
        self.yaw_radians = wrap_angle(
            self.yaw_radians
                + screen_sign
                    * (finite_or_zero(input.yaw_delta_radians)
                        + finite_or_zero(input.rotate)
                            * self.tuning.rotation_speed_radians_per_second
                            * elapsed_seconds),
        );
        self.pitch_radians = (self.pitch_radians
            + finite_or_zero(input.pitch_delta_radians)
            + finite_or_zero(input.tilt)
                * self.tuning.rotation_speed_radians_per_second
                * elapsed_seconds)
            .clamp(self.tuning.pitch_min_radians, self.tuning.pitch_max_radians);

        self.advance_zoom(elapsed_seconds);
        let multiplier = if input.speed_multiplier.is_finite() && input.speed_multiplier > 0.0 {
            input.speed_multiplier
        } else {
            1.0
        };
        // Interpolate the authored near/far pan speeds so movement scales with
        // the distance covered by the visible ground plane.
        let base_speed = if input.speed_multiplier.is_finite() && input.speed_multiplier > 1.0 {
            self.tuning.move_speed_fast
        } else {
            self.tuning.move_speed_normal
        };
        let pan_speed = lerp(base_speed, self.tuning.zoomed_pan_speed, self.zoom) * multiplier;
        let forward = self.forward();
        let screen_forward = DVec3::new(forward.x, 0.0, forward.z)
            .try_normalize()
            .unwrap_or(DVec3::NEG_Z);
        let screen_right = screen_forward.cross(DVec3::Y) * screen_sign;
        let movement = screen_forward * finite_or_zero(input.forward)
            + screen_right * finite_or_zero(input.right);
        let direction = movement.try_normalize().unwrap_or(DVec3::ZERO);
        self.ground_target += direction * pan_speed * elapsed_seconds;
    }

    pub fn zoom_by_steps(&mut self, steps_toward_subject: f64) {
        if !steps_toward_subject.is_finite() || steps_toward_subject == 0.0 {
            return;
        }
        self.zoom_start = self.zoom;
        self.target_zoom =
            (self.target_zoom - steps_toward_subject * self.tuning.zoom_step).clamp(0.0, 1.0);
        self.zoom_elapsed_seconds = 0.0;
    }

    pub fn distance_meters(self) -> f64 {
        lerp(self.tuning.zoom_min_m, self.tuning.zoom_max_m, self.zoom)
    }

    pub fn look_at_height_meters(self) -> f64 {
        lerp(
            self.tuning.look_at_height_min_m,
            self.tuning.look_at_height_max_m,
            self.zoom,
        )
    }

    pub fn look_at(self) -> DVec3 {
        self.ground_target + DVec3::Y * self.look_at_height_meters()
    }

    pub fn eye(self) -> DVec3 {
        self.look_at() - self.forward() * self.distance_meters()
    }

    pub fn follow_terrain(
        &mut self,
        target_terrain_height_meters: f64,
        eye_terrain_height_meters: f64,
    ) {
        if !target_terrain_height_meters.is_finite() || !eye_terrain_height_meters.is_finite() {
            return;
        }
        self.ground_target.y = target_terrain_height_meters + RTS_TARGET_TERRAIN_CLEARANCE_METERS;
        let minimum_eye_height = eye_terrain_height_meters + RTS_EYE_TERRAIN_CLEARANCE_METERS;
        let eye_deficit = minimum_eye_height - self.eye().y;
        if eye_deficit > 0.0 {
            self.ground_target.y += eye_deficit;
        }
    }

    pub fn forward(self) -> DVec3 {
        DQuat::from_rotation_y(self.yaw_radians)
            * DQuat::from_rotation_x(self.pitch_radians)
            * DVec3::NEG_Z
    }

    pub fn horizontal_fov_radians_for_aspect(self, aspect: f64) -> f64 {
        let factor = ((aspect - self.tuning.narrow_aspect)
            / (self.tuning.wide_aspect - self.tuning.narrow_aspect))
            .clamp(0.0, 1.0);
        lerp(
            self.tuning.narrow_hfov_radians,
            self.tuning.wide_hfov_radians,
            factor,
        )
    }

    fn advance_zoom(&mut self, elapsed_seconds: f64) {
        if self.zoom_elapsed_seconds >= self.tuning.zoom_time_seconds {
            self.zoom = self.target_zoom;
            return;
        }
        self.zoom_elapsed_seconds =
            (self.zoom_elapsed_seconds + elapsed_seconds).min(self.tuning.zoom_time_seconds);
        let progress = self.zoom_elapsed_seconds / self.tuning.zoom_time_seconds;
        self.zoom = lerp(
            self.zoom_start,
            self.target_zoom,
            (progress * PI * 0.5).sin(),
        );
    }
}

fn finite_plane(value: f64, fallback: f64) -> f64 {
    if value.is_finite() { value } else { fallback }
}

fn lerp(start: f64, end: f64, factor: f64) -> f64 {
    start + (end - start) * factor.clamp(0.0, 1.0)
}

fn finite_or_zero(value: f64) -> f64 {
    if value.is_finite() { value } else { 0.0 }
}

fn wrap_angle(angle: f64) -> f64 {
    (angle + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU) - std::f64::consts::PI
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Original synthetic camera profile.
    fn profile() -> CameraProfile {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/camera-profile-rts-v1.json");
        CameraProfile::load(&path).expect("authored profile loads")
    }

    fn free_fly() -> FreeFlyCamera {
        FreeFlyCamera {
            movement_speed_meters_per_second: 40.0,
            ..FreeFlyCamera::new(
                &profile(),
                DVec3::new(0.0, 150.0, 0.0),
                0.0,
                -0.35,
                60.0_f64.to_radians(),
            )
        }
    }

    fn rts() -> RtsCamera {
        RtsCamera::new(&profile()).expect("authored profile validates")
    }

    #[test]
    fn cameras_present_with_the_profiles_handedness() {
        let mut profile = profile();
        assert_eq!(rts().handedness, ViewHandedness::RightHanded);
        assert_eq!(free_fly().handedness, ViewHandedness::RightHanded);
        profile.view_convention.handedness = ViewHandedness::RightHanded;
        assert_eq!(
            RtsCamera::new(&profile).unwrap().handedness,
            ViewHandedness::RightHanded
        );
        let flight = FreeFlyCamera::new(&profile, DVec3::ZERO, 0.0, 0.0, 1.0);
        assert_eq!(flight.handedness, ViewHandedness::RightHanded);
        assert_eq!(
            flight.movement_speed_meters_per_second.to_bits(),
            profile.free_fly.default_speed_meters_per_second.to_bits()
        );
    }

    #[test]
    fn movement_uses_elapsed_real_time() {
        let mut one_step = free_fly();
        let mut two_steps = one_step;
        let input = FreeFlyInput {
            forward: 1.0,
            ..FreeFlyInput::default()
        };
        one_step.update(input, Duration::from_millis(250));
        two_steps.update(input, Duration::from_millis(125));
        two_steps.update(input, Duration::from_millis(125));
        assert!(one_step.position.abs_diff_eq(two_steps.position, 1.0e-12));
    }

    #[test]
    fn elapsed_time_is_bounded_after_a_stall() {
        let mut camera = free_fly();
        camera.update(
            FreeFlyInput {
                forward: 1.0,
                ..FreeFlyInput::default()
            },
            Duration::from_secs(10),
        );
        let expected = camera.movement_speed_meters_per_second * MAX_ELAPSED_SECONDS;
        let actual = camera.position.distance(free_fly().position);
        assert!((actual - expected).abs() < 1.0e-12);
    }

    #[test]
    fn pitch_is_clamped_away_from_singularity() {
        let mut camera = free_fly();
        camera.update(
            FreeFlyInput {
                pitch_delta_radians: 10.0,
                ..FreeFlyInput::default()
            },
            Duration::ZERO,
        );
        assert!(camera.pitch_radians < FRAC_PI_2);
    }

    #[test]
    fn dolly_moves_along_the_view_direction_without_changing_orientation() {
        let mut camera = FreeFlyCamera {
            yaw_radians: 0.7,
            pitch_radians: -0.3,
            ..free_fly()
        };
        let start = camera.position;
        let forward = camera.forward();
        camera.dolly(10.0);
        assert!((camera.position - (start + forward * 10.0)).length() < 1e-9);
        // Zoom must not secretly re-aim the camera.
        assert!((camera.yaw_radians - 0.7).abs() < f64::EPSILON);
        assert!((camera.pitch_radians + 0.3).abs() < f64::EPSILON);
        // A dolly is a move, not a lens change; the field of view is untouched.
        assert!(
            (camera.vertical_fov_radians - free_fly().vertical_fov_radians).abs() < f64::EPSILON
        );
        camera.dolly(-10.0);
        assert!((camera.position - start).length() < 1e-9);
    }

    #[test]
    fn dolly_ignores_non_finite_distances() {
        let mut camera = free_fly();
        let start = camera.position;
        camera.dolly(f64::NAN);
        camera.dolly(f64::INFINITY);
        assert_eq!(camera.position, start);
    }

    #[test]
    fn diagonal_movement_has_the_same_speed_as_axial_movement() {
        let mut axial = free_fly();
        let mut diagonal = axial;
        axial.update(
            FreeFlyInput {
                forward: 1.0,
                ..FreeFlyInput::default()
            },
            Duration::from_millis(100),
        );
        diagonal.update(
            FreeFlyInput {
                forward: 1.0,
                right: 1.0,
                ..FreeFlyInput::default()
            },
            Duration::from_millis(100),
        );
        let origin = free_fly().position;
        let axial_distance = axial.position.distance(origin);
        let diagonal_distance = diagonal.position.distance(origin);
        assert!((axial_distance - diagonal_distance).abs() < 1.0e-12);
    }

    #[test]
    fn speed_and_fov_adjustments_are_bounded() {
        let mut camera = free_fly();
        camera.scale_movement_speed(1.0e12);
        assert!(
            (camera.movement_speed_meters_per_second - MAX_SPEED_METERS_PER_SECOND).abs()
                < f64::EPSILON
        );
        camera.scale_movement_speed(1.0e-12);
        assert!(
            (camera.movement_speed_meters_per_second - MIN_SPEED_METERS_PER_SECOND).abs()
                < f64::EPSILON
        );
        camera.set_vertical_fov_radians(180.0_f64.to_radians());
        assert!((camera.vertical_fov_radians - MAX_FOV_RADIANS).abs() < f64::EPSILON);
    }

    #[test]
    fn camera_view_is_finite_and_uses_camera_position() {
        let camera = free_fly();
        let view = camera.camera_view(16.0 / 9.0, 0.5, 5_000.0);
        assert_eq!(view.position, camera.position.as_vec3());
        assert!(view.view_projection.is_finite());
    }

    #[test]
    fn non_finite_input_cannot_poison_camera_state() {
        let mut camera = free_fly();
        camera.update(
            FreeFlyInput {
                forward: f64::NAN,
                yaw_delta_radians: f64::INFINITY,
                pitch_delta_radians: f64::NEG_INFINITY,
                speed_multiplier: f64::NAN,
                ..FreeFlyInput::default()
            },
            Duration::from_millis(16),
        );
        assert!(camera.position.is_finite());
        assert!(camera.yaw_radians.is_finite());
        assert!(camera.pitch_radians.is_finite());
    }

    #[test]
    fn rts_wheel_zoom_interpolates_distance_with_sine_out() {
        let mut camera = rts();
        let initial_distance = camera.distance_meters();
        camera.zoom_by_steps(1.0);
        camera.update(RtsCameraInput::default(), Duration::from_millis(100));
        assert!(camera.distance_meters() < initial_distance);
        assert!(camera.zoom > camera.target_zoom);
        camera.update(RtsCameraInput::default(), Duration::from_millis(100));
        assert!((camera.zoom - camera.target_zoom).abs() < f64::EPSILON);
    }

    #[test]
    fn rts_pan_follows_screen_axes_after_rotation() {
        let mut camera = rts();
        camera.update(
            RtsCameraInput {
                forward: 1.0,
                ..RtsCameraInput::default()
            },
            Duration::from_millis(250),
        );
        assert!(camera.ground_target.z < 0.0);
        assert!(camera.ground_target.y.abs() < f64::EPSILON);

        camera.ground_target = DVec3::ZERO;
        camera.yaw_radians = FRAC_PI_2;
        camera.update(
            RtsCameraInput {
                forward: 1.0,
                ..RtsCameraInput::default()
            },
            Duration::from_millis(250),
        );
        assert!(camera.ground_target.x < 0.0);
        assert!(camera.ground_target.z.abs() < 1.0e-12);
    }

    #[test]
    fn rts_horizontal_fov_interpolates_authored_aspect_endpoints() {
        let camera = rts();
        for (aspect, degrees) in [(1.0, 60.0), (1.5, 80.0), (2.0, 100.0)] {
            assert!(
                (camera
                    .horizontal_fov_radians_for_aspect(aspect)
                    .to_degrees()
                    - degrees)
                    .abs()
                    < 1.0e-12
            );
        }
    }

    #[test]
    fn rts_terrain_following_keeps_target_and_eye_clear() {
        let mut camera = rts();
        camera.follow_terrain(12.0, 80.0);
        assert!(camera.ground_target.y >= 12.0 + RTS_TARGET_TERRAIN_CLEARANCE_METERS);
        assert!(camera.eye().y >= 80.0 + RTS_EYE_TERRAIN_CLEARANCE_METERS);
    }

    #[test]
    fn free_flight_explicitly_bypasses_rts_terrain_following() {
        let mut camera = free_fly();
        camera.position.y = -25.0;
        camera.update(FreeFlyInput::default(), Duration::from_millis(16));
        assert!((camera.position.y + 25.0).abs() < f64::EPSILON);
    }

    #[test]
    fn rts_from_free_fly_pose_preserves_eye_and_clamps_pitch() {
        let eye = DVec3::new(100.0, 200.0, -300.0);
        let camera = RtsCamera::from_free_fly_pose(eye, 0.5, -0.3, &profile()).unwrap();
        assert!(
            (camera.eye() - eye).length() < 1e-9,
            "eye position must be preserved exactly"
        );
        assert!((camera.yaw_radians - 0.5).abs() < 1e-12);
        assert!((camera.pitch_radians - (-0.3)).abs() < 1e-12);

        // Pitch above the RTS maximum is clamped down to the shallow limit.
        let too_shallow = RtsCamera::from_free_fly_pose(eye, 0.0, 0.5, &profile()).unwrap();
        assert!(
            (too_shallow.pitch_radians - (profile().rts.pitch_max_deg.to_radians())).abs()
                < f64::EPSILON
        );
        // Pitch below the RTS minimum is clamped up to the steep limit.
        let too_steep = RtsCamera::from_free_fly_pose(eye, 0.0, -2.0, &profile()).unwrap();
        assert!(
            (too_steep.pitch_radians - (profile().rts.pitch_min_deg.to_radians())).abs()
                < f64::EPSILON
        );
    }

    #[test]
    fn rts_camera_clamps_pitch_and_produces_finite_view() {
        let mut camera = rts();
        camera.update(
            RtsCameraInput {
                pitch_delta_radians: 100.0,
                ..RtsCameraInput::default()
            },
            Duration::ZERO,
        );
        assert!(
            (camera.pitch_radians - (profile().rts.pitch_max_deg.to_radians())).abs()
                < f64::EPSILON
        );
        let view = camera.camera_view(16.0 / 9.0, 0.1, 5_000.0);
        assert_eq!(view.position, camera.eye().as_vec3());
        assert!(view.view_projection.is_finite());
    }
}
