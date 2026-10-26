use crate::{PostError, ToneMap};

/// Display-linear grading. Composition is exposure, bloom addition, scale, curve,
/// channel powers, then vignette. Identity Reinhard/Filmic retain legacy bloom
/// addition before exposure. Attachment encoding remains exclusively sRGB.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ColorGrading {
    /// Multiplies the complete exposed scene plus bloom, within 0..=64.
    pub linear_scale: f32,
    /// Positive channel exponents within 0.01..=16. Identity skips pow entirely.
    pub display_power: [f32; 3],
    pub vignette: Option<Vignette>,
}
impl Default for ColorGrading {
    fn default() -> Self {
        Self {
            linear_scale: 1.0,
            display_power: [1.0; 3],
            vignette: None,
        }
    }
}
/// Radius is `length((uv - 0.5) * axis_scale) / sqrt(0.5)` using pixel centres.
/// Tint mixes toward a display-linear RGB by `smoothstep(tint_range) * tint alpha`.
/// Darkening multiplies by `mix(1, smoothstep(darken_range), darken_opacity)`.
/// Both endpoint orders are supported; equal endpoints are rejected.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vignette {
    pub axis_scale: [f32; 2],
    pub tint_range: [f32; 2],
    pub tint: [f32; 4],
    pub darken_range: [f32; 2],
    pub darken_opacity: f32,
}
impl ColorGrading {
    pub fn validate(self) -> Result<(), PostError> {
        if !valid(self.linear_scale, 0.0, 64.0)
            || self.display_power.iter().any(|&v| !valid(v, 0.01, 16.0))
        {
            return Err(PostError::UnsupportedProfile("display grading"));
        }
        if let Some(v) = self.vignette
            && (v.axis_scale.iter().any(|&x| !valid(x, 0.0, 16.0))
                || v.tint.iter().any(|&x| !valid(x, 0.0, 1.0))
                || !valid(v.darken_opacity, 0.0, 1.0)
                || [v.tint_range, v.darken_range]
                    .iter()
                    .any(|r| (r[0] - r[1]).abs() == 0.0 || r.iter().any(|&x| !valid(x, 0.0, 16.0))))
        {
            return Err(PostError::UnsupportedProfile("vignette"));
        }
        Ok(())
    }
    /// CPU reference, including exposure and an already filtered, unweighted bloom.
    pub fn evaluate(
        self,
        curve: ToneMap,
        rgb: [f64; 3],
        exposure: f64,
        bloom: ([f64; 3], f64),
        uv: [f64; 2],
    ) -> [f64; 3] {
        if curve != ToneMap::AcesFitted && self == Self::default() {
            return std::array::from_fn(|i| {
                if !rgb[i].is_finite() {
                    return curve.evaluate(rgb[i], exposure);
                }
                let glow = if bloom.1 > 0.0 {
                    bloom.0[i] * bloom.1
                } else {
                    0.0
                };
                curve.evaluate(sanitize(rgb[i]) + glow, exposure)
            });
        }
        let input = std::array::from_fn(|i| {
            if self.linear_scale == 0.0 {
                return 0.0;
            }
            let exposed = if exposure == 0.0 {
                0.0
            } else {
                sanitize(rgb[i]) * exposure
            };
            let glow = if bloom.1 == 0.0 {
                0.0
            } else {
                sanitize(bloom.0[i]) * bloom.1
            };
            (exposed + glow) * f64::from(self.linear_scale)
        });
        self.apply(curve.evaluate_rgb(input, 1.0), uv)
    }
    pub fn apply(self, mut rgb: [f64; 3], uv: [f64; 2]) -> [f64; 3] {
        for (c, p) in rgb.iter_mut().zip(self.display_power) {
            if p.to_bits() != 1.0_f32.to_bits() {
                *c = c.powf(f64::from(p));
            }
        }
        if let Some(v) = self.vignette {
            let radius = ((uv[0] - 0.5) * f64::from(v.axis_scale[0]))
                .hypot((uv[1] - 0.5) * f64::from(v.axis_scale[1]))
                / 0.5_f64.sqrt();
            let tint = smooth(v.tint_range, radius) * f64::from(v.tint[3]);
            let dark = 1.0 + (smooth(v.darken_range, radius) - 1.0) * f64::from(v.darken_opacity);
            for (c, target) in rgb.iter_mut().zip(v.tint) {
                *c = (*c + (f64::from(target) - *c) * tint) * dark;
            }
        }
        rgb
    }
    pub(crate) fn constants(self) -> Vec<(&'static str, f64)> {
        let mut out = vec![("LINEAR_SCALE", self.linear_scale.into())];
        for (name, value) in ["POWER_R", "POWER_G", "POWER_B"]
            .into_iter()
            .zip(self.display_power)
        {
            out.push((name, value.into()));
        }
        if let Some(v) = self.vignette {
            out.push(("VIGNETTE", 1.0));
            for (name, value) in [
                "AXIS_X",
                "AXIS_Y",
                "VG_VG_TINT_BEGIN",
                "VG_TINT_END",
                "VG_TINT_R",
                "VG_TINT_G",
                "VG_TINT_B",
                "VG_TINT_A",
                "DARK_BEGIN",
                "DARK_END",
                "DARK_OPACITY",
            ]
            .into_iter()
            .zip(
                v.axis_scale
                    .into_iter()
                    .chain(v.tint_range)
                    .chain(v.tint)
                    .chain(v.darken_range)
                    .chain([v.darken_opacity]),
            ) {
                out.push((name, value.into()));
            }
        }
        out
    }
}
fn valid(v: f32, min: f32, max: f32) -> bool {
    v.is_finite() && (min..=max).contains(&v)
}
fn smooth(range: [f32; 2], x: f64) -> f64 {
    let t = ((x - f64::from(range[0])) / f64::from(range[1] - range[0])).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}
pub(crate) fn sanitize(x: f64) -> f64 {
    if x.is_nan() || x < 0.0 { 0.0 } else { x }
}
