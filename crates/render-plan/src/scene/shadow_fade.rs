//! The far part of the shadow distance over which receivers return to fully lit.
use super::{SceneShadowCascades, SceneValidationError};

impl SceneShadowCascades {
    /// Fades receivers to fully lit over the final `fade_out_micro` millionths of the
    /// shadow distance, ending where the last cascade ends. Zero keeps the hard end.
    ///
    /// # Errors
    ///
    /// Returns [`SceneValidationError`] when the fraction exceeds one.
    pub fn with_fade_out(mut self, fade_out_micro: u64) -> Result<Self, SceneValidationError> {
        super::effects::require_range("shadow fade_out_micro", fade_out_micro, 0, 1_000_000)?;
        self.fade_out_micro = fade_out_micro;
        Ok(self)
    }

    /// Millionths of the shadow distance that fade to lit; zero by default.
    #[must_use]
    pub const fn fade_out_micro(&self) -> u64 {
        self.fade_out_micro
    }
}
