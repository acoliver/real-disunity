//! Camera discontinuities remain pending until the frame that shows them is submitted.
use super::{CameraPose, CameraShadowPolicy, RetainedUpdate, SceneCamera};
use worldrunner_camera::ViewConvention;

/// Whether a camera update can reuse the preceding view's temporal samples.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CameraContinuity {
    #[default]
    Continuous,
    Cut,
}

/// Why this submitted frame discarded its camera-dependent temporal history.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TemporalHistoryReset {
    #[default]
    None,
    CameraCut,
}

pub(crate) struct CameraUpdate {
    pub(crate) pose: CameraPose,
    pub(crate) lens: SceneCamera,
    pub(crate) convention: ViewConvention,
    pub(crate) continuity: CameraContinuity,
    pub(crate) shadow_policy: Option<CameraShadowPolicy>,
}

impl RetainedUpdate {
    /// Selects continuity explicitly. The ordinary `Camera` variant is continuous.
    pub fn camera_with_continuity(
        pose: CameraPose,
        lens: SceneCamera,
        convention: ViewConvention,
        continuity: CameraContinuity,
    ) -> Self {
        Self::CameraWithContinuity {
            pose,
            lens,
            convention,
            continuity,
        }
    }

    /// Selects native shadow reception in the same ordered camera transaction.
    pub fn camera_with_shadow_policy(
        pose: CameraPose,
        lens: SceneCamera,
        convention: ViewConvention,
        continuity: CameraContinuity,
        shadow_policy: Option<CameraShadowPolicy>,
    ) -> Self {
        Self::CameraWithShadowPolicy {
            pose,
            lens,
            convention,
            continuity,
            shadow_policy,
        }
    }

    pub(crate) fn camera_update(&self) -> Option<CameraUpdate> {
        match *self {
            Self::Camera {
                pose,
                lens,
                convention,
            } => Some(CameraUpdate {
                pose,
                lens,
                convention,
                continuity: CameraContinuity::Continuous,
                shadow_policy: None,
            }),
            Self::CameraWithContinuity {
                pose,
                lens,
                convention,
                continuity,
            } => Some(CameraUpdate {
                pose,
                lens,
                convention,
                continuity,
                shadow_policy: None,
            }),
            Self::CameraWithShadowPolicy {
                pose,
                lens,
                convention,
                continuity,
                shadow_policy,
            } => Some(CameraUpdate {
                pose,
                lens,
                convention,
                continuity,
                shadow_policy,
            }),
            _ => None,
        }
    }
}

#[derive(Default)]
pub(crate) struct CameraHistory {
    continuity: CameraContinuity,
}
impl CameraHistory {
    pub(crate) fn publish(&mut self, continuity: CameraContinuity) {
        if continuity == CameraContinuity::Cut {
            self.continuity = continuity;
        }
    }

    pub(crate) fn reset_reason(&self) -> TemporalHistoryReset {
        match self.continuity {
            CameraContinuity::Continuous => TemporalHistoryReset::None,
            CameraContinuity::Cut => TemporalHistoryReset::CameraCut,
        }
    }

    pub(crate) fn prepare(&self, reset: impl FnOnce()) -> TemporalHistoryReset {
        let reason = self.reset_reason();
        if reason == TemporalHistoryReset::CameraCut {
            reset();
        }
        reason
    }

    pub(crate) fn submitted(&mut self) {
        self.continuity = CameraContinuity::Continuous;
    }
}
