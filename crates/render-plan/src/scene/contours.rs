//! Exact authored contour styles. Object IDs refer to this table, with zero reserved.
use super::effects::require_range;
use super::{SceneEffects, SceneValidationError};

const UNIT: u64 = 1_000_000;
const MAX_WIDTH: u64 = 32 * UNIT;

/// How a contour colour combines with the scene colour.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SceneContourBlend {
    /// Alpha-weighted replacement, preserving the scene alpha.
    #[default]
    AlphaOver,
    /// Alpha-weighted addition, preserving the scene alpha.
    Additive,
}

/// Optional interior effect, with linear RGBA and ratios in millionths.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SceneContourFill {
    pub filler_color_micro: [u32; 4],
    pub edge_color_micro: [u32; 4],
    /// Width in reference pixels, in millionths, within 0..=32 pixels.
    pub inner_width_micro: u32,
    pub effect_strength_micro: u32,
    pub grain_strength_micro: u32,
    /// Symmetric horizontal displacement in reference pixels, in millionths.
    pub aberration_shift_micro: u32,
    pub aberration_strength_micro: u32,
    pub interlacing_strength_micro: u32,
}

impl SceneContourFill {
    pub fn validate(&self) -> Result<(), SceneValidationError> {
        color(self.filler_color_micro)?;
        color(self.edge_color_micro)?;
        for (part, value, maximum) in [
            ("contour inner width", self.inner_width_micro, MAX_WIDTH),
            ("contour effect strength", self.effect_strength_micro, UNIT),
            ("contour grain strength", self.grain_strength_micro, UNIT),
            (
                "contour aberration shift",
                self.aberration_shift_micro,
                MAX_WIDTH,
            ),
            (
                "contour aberration strength",
                self.aberration_strength_micro,
                UNIT,
            ),
            (
                "contour interlacing strength",
                self.interlacing_strength_micro,
                UNIT,
            ),
        ] {
            require_range(part, u64::from(value), 0, maximum)?;
        }
        Ok(())
    }
}

/// One ID's outline and optional interior policy, using exact integer quantities.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SceneContourStyle {
    /// Outer radius in reference pixels, in millionths, within 0..=32 pixels.
    pub size_micro: u32,
    /// Linear RGB within 0..=16 and alpha within 0..=1, all in millionths.
    pub color_micro: [u32; 4],
    pub blend: SceneContourBlend,
    /// Zero means steady. Nonzero periods use the frame animation clock.
    pub pulse_period_milliseconds: u32,
    /// Curve exponent in millionths, in (0,16] when the period is nonzero.
    pub pulse_power_micro: u32,
    pub fill: Option<SceneContourFill>,
}

impl Default for SceneContourStyle {
    fn default() -> Self {
        Self {
            size_micro: 1_000_000,
            color_micro: [1_000_000; 4],
            blend: SceneContourBlend::AlphaOver,
            pulse_period_milliseconds: 0,
            pulse_power_micro: 1_000_000,
            fill: None,
        }
    }
}

impl SceneContourStyle {
    pub fn validate(&self) -> Result<(), SceneValidationError> {
        require_range("contour size", u64::from(self.size_micro), 0, MAX_WIDTH)?;
        color(self.color_micro)?;
        if self.pulse_period_milliseconds != 0 {
            require_range(
                "contour pulse power",
                u64::from(self.pulse_power_micro),
                1,
                16 * UNIT,
            )?;
        }
        if let Some(fill) = self.fill {
            fill.validate()?;
        }
        Ok(())
    }
}

/// Owned style table. Entry zero is always absent; unreferenced styles cost no GPU work.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SceneContours {
    reference_extent: [u32; 2],
    styles: Box<[Option<SceneContourStyle>; 256]>,
}

impl Default for SceneContours {
    fn default() -> Self {
        Self {
            reference_extent: [1920, 1080],
            styles: vec![None; 256]
                .into_boxed_slice()
                .try_into()
                .expect("256 styles"),
        }
    }
}

impl SceneContours {
    pub fn new(
        reference_extent: [u32; 2],
        styles: Vec<Option<SceneContourStyle>>,
    ) -> Result<Self, SceneValidationError> {
        let value = Self {
            reference_extent,
            styles: styles.into_boxed_slice().try_into().map_err(|_| {
                SceneValidationError::UnsupportedInput {
                    part: "contour style table must have 256 entries",
                }
            })?,
        };
        value.validate()?;
        Ok(value)
    }

    pub const fn reference_extent(&self) -> [u32; 2] {
        self.reference_extent
    }
    pub fn styles(&self) -> &[Option<SceneContourStyle>; 256] {
        &self.styles
    }

    pub fn validate(&self) -> Result<(), SceneValidationError> {
        for dimension in self.reference_extent {
            require_range(
                "contour reference extent",
                u64::from(dimension),
                1,
                u64::from(u32::MAX),
            )?;
        }
        if self.styles[0].is_some() {
            return Err(SceneValidationError::UnsupportedInput {
                part: "contour ID zero is reserved",
            });
        }
        for style in self.styles.iter().flatten() {
            style.validate()?;
        }
        Ok(())
    }

    /// Zero has no style; every nonzero object ID must be defined.
    pub fn validate_id(&self, id: u8) -> Result<(), SceneValidationError> {
        if id != 0 && self.styles[usize::from(id)].is_none() {
            return Err(SceneValidationError::UnsupportedInput {
                part: "object contour ID is undefined",
            });
        }
        Ok(())
    }
}

impl SceneEffects {
    pub fn with_contours(mut self, contours: SceneContours) -> Result<Self, SceneValidationError> {
        contours.validate()?;
        self.contours = contours;
        Ok(self)
    }

    pub const fn contours(&self) -> &SceneContours {
        &self.contours
    }
}

fn color(value: [u32; 4]) -> Result<(), SceneValidationError> {
    for component in &value[..3] {
        require_range("contour linear RGB", u64::from(*component), 0, 16 * UNIT)?;
    }
    require_range("contour alpha", u64::from(value[3]), 0, UNIT)
}

/// Validates references even when the scene has no effects profile.
pub(super) fn validate_object_id(
    effects: Option<&SceneEffects>,
    id: u8,
) -> Result<(), SceneValidationError> {
    if id == 0 {
        return Ok(());
    }
    effects
        .ok_or(SceneValidationError::UnsupportedInput {
            part: "object contour ID requires a style table",
        })?
        .contours()
        .validate_id(id)
}
