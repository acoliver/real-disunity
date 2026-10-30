use crate::PostError;

/// Numeric domain of the native-resolution RCAS kernel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RcasDomain {
    DisplayLinear,
    /// Output transfer values before quantization. sRGB outputs use the sRGB OETF;
    /// linear outputs use identity transfer. Alpha is never transfer encoded.
    EncodedOutput,
}

/// Native-only sharpening, after the resolve and CAS. `None` schedules no pass.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NativeRcasSettings {
    /// Finite stops in 0..=2; zero is strongest and two is weakest, not off.
    pub stops: f32,
    pub domain: RcasDomain,
}
impl NativeRcasSettings {
    pub fn validate(self) -> Result<(), PostError> {
        if !self.stops.is_finite() || !(0.0..=2.0).contains(&self.stops) {
            return Err(PostError::UnsupportedProfile(
                "native RCAS sharpness must be finite and within 0..=2 stops",
            ));
        }
        Ok(())
    }
}
