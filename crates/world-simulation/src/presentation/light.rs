//! A 3D light component's settings. The engine draws no 3D lights yet, so a light is state that
//! scripts and scene data set and read back; nothing is lit by it. The defaults are the engine's
//! choices.
use super::invalid;
use crate::WorldError;

/// What shape of light a component casts. The numbering is the one managed `LightType` uses.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u32)]
pub enum LightKind {
    Spot = 0,
    Directional = 1,
    #[default]
    Point = 2,
    Rectangle = 3,
    Disc = 4,
    Pyramid = 5,
    Box = 6,
    Tube = 7,
}
impl LightKind {
    pub fn from_value(value: u32) -> Option<Self> {
        [
            Self::Spot,
            Self::Directional,
            Self::Point,
            Self::Rectangle,
            Self::Disc,
            Self::Pyramid,
            Self::Box,
            Self::Tube,
        ]
        .into_iter()
        .find(|kind| *kind as u32 == value)
    }
}

/// Whether and how a light casts shadows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u32)]
pub enum LightShadowMode {
    #[default]
    None = 0,
    Hard = 1,
    Soft = 2,
}
impl LightShadowMode {
    pub fn from_value(value: u32) -> Option<Self> {
        [Self::None, Self::Hard, Self::Soft]
            .into_iter()
            .find(|mode| *mode as u32 == value)
    }
}

/// When a light's lighting is computed. The numbering is the one managed `LightmapBakeType` uses.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u32)]
pub enum LightBakeType {
    Mixed = 1,
    Baked = 2,
    #[default]
    Realtime = 4,
}
impl LightBakeType {
    pub fn from_value(value: u32) -> Option<Self> {
        [Self::Mixed, Self::Baked, Self::Realtime]
            .into_iter()
            .find(|bake| *bake as u32 == value)
    }
}

/// The narrowest and widest cone of a spot light, in degrees.
pub const SPOT_ANGLE_RANGE: (f32, f32) = (1., 179.);

/// Every setting of a light other than whether it is enabled.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LightSettings {
    pub kind: LightKind,
    /// Linear RGBA.
    pub color: [f32; 4],
    pub intensity: f32,
    pub range: f32,
    /// Cone angle of a spot light in degrees, within [`SPOT_ANGLE_RANGE`].
    pub spot_angle: f32,
    /// Inner cone angle in degrees, from 0 to `spot_angle`.
    pub inner_spot_angle: f32,
    pub shadows: LightShadowMode,
    /// From 0 to 1.
    pub shadow_strength: f32,
    pub shadow_bias: f32,
    pub shadow_normal_bias: f32,
    pub shadow_near_plane: f32,
    /// One bit per `GameObject` layer the light affects.
    pub culling_mask: u32,
    pub rendering_layer_mask: u32,
    pub bounce_intensity: f32,
    /// In kelvin; finite and above zero.
    pub color_temperature: f32,
    pub use_color_temperature: bool,
    pub cookie_size: f32,
    /// Instance id of the managed cookie texture; zero for none.
    pub cookie_texture: u64,
    pub bake_type: LightBakeType,
}
impl Default for LightSettings {
    fn default() -> Self {
        Self {
            kind: LightKind::Point,
            color: [1.; 4],
            intensity: 1.,
            range: 10.,
            spot_angle: 30.,
            inner_spot_angle: 21.8,
            shadows: LightShadowMode::None,
            shadow_strength: 1.,
            shadow_bias: 0.05,
            shadow_normal_bias: 0.4,
            shadow_near_plane: 0.2,
            culling_mask: u32::MAX,
            rendering_layer_mask: 1,
            bounce_intensity: 1.,
            color_temperature: 6570.,
            use_color_temperature: false,
            cookie_size: 10.,
            cookie_texture: 0,
            bake_type: LightBakeType::Realtime,
        }
    }
}
impl LightSettings {
    /// Rejects what the settings cannot hold: a negative or non-finite intensity, range, bounce
    /// intensity or cookie size, a color temperature that is not above zero, a non-finite color or
    /// shadow bias, an angle outside its range and a shadow strength outside 0 to 1.
    pub fn validate(self) -> Result<(), WorldError> {
        let non_negative = [
            self.intensity,
            self.range,
            self.bounce_intensity,
            self.cookie_size,
        ]
        .into_iter()
        .all(|value| value.is_finite() && value >= 0.);
        let finite = self
            .color
            .into_iter()
            .chain([
                self.shadow_bias,
                self.shadow_normal_bias,
                self.shadow_near_plane,
            ])
            .all(f32::is_finite);
        let (narrowest, widest) = SPOT_ANGLE_RANGE;
        let angles = (narrowest..=widest).contains(&self.spot_angle)
            && (0. ..=self.spot_angle).contains(&self.inner_spot_angle);
        let temperature = self.color_temperature.is_finite() && self.color_temperature > 0.;
        let strength = (0. ..=1.).contains(&self.shadow_strength);
        if non_negative && finite && angles && temperature && strength {
            Ok(())
        } else {
            Err(invalid())
        }
    }

    /// Brings the angles and shadow strength into range, then validates. The inner angle comes
    /// down with the spot angle. A NaN angle or strength is rejected, not clamped.
    pub fn normalized(mut self) -> Result<Self, WorldError> {
        let angles = [self.spot_angle, self.inner_spot_angle, self.shadow_strength];
        if angles.iter().any(|value| value.is_nan()) {
            return Err(invalid());
        }
        let (narrowest, widest) = SPOT_ANGLE_RANGE;
        self.spot_angle = self.spot_angle.clamp(narrowest, widest);
        self.inner_spot_angle = self.inner_spot_angle.clamp(0., self.spot_angle);
        self.shadow_strength = self.shadow_strength.clamp(0., 1.);
        self.validate()?;
        Ok(self)
    }
}

/// A 3D light component: whether it is enabled and its settings, which a disabled light keeps.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Light {
    pub enabled: bool,
    pub settings: LightSettings,
}
impl Default for Light {
    fn default() -> Self {
        Self {
            enabled: true,
            settings: LightSettings::default(),
        }
    }
}
impl Light {
    pub fn validate(self) -> Result<(), WorldError> {
        self.settings.validate()
    }

    /// See [`LightSettings::normalized`].
    pub fn normalized(self) -> Result<Self, WorldError> {
        Ok(Self {
            settings: self.settings.normalized()?,
            ..self
        })
    }
}
