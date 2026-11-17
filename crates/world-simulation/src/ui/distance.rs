//! Distance interpretation shared by presentation validation and the overlay.
use super::UiError;
use glam::{DVec2, Vec2};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum UiDistanceMode {
    Fill = 1,
    Outline = 2,
    Glow = 3,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiDistanceStyle {
    /// Encoded signed-distance spread in atlas pixels, positive inside the glyph.
    pub spread_px: f32,
    /// Effect extent in final physical screen pixels.
    pub radius_px: f32,
    pub mode: UiDistanceMode,
    /// Fully modulated linear straight-alpha effect tint, independent of fill RGB.
    pub effect_color: [f32; 4],
}
impl UiDistanceStyle {
    pub fn validate(self) -> Result<(), UiError> {
        if !self.spread_px.is_finite()
            || self.spread_px <= 0.
            || self.spread_px > 256.
            || !self.radius_px.is_finite()
            || self.radius_px < 0.
            || (self.mode == UiDistanceMode::Glow && self.radius_px == 0.)
            || self
                .effect_color
                .into_iter()
                .any(|v| !(0. ..=1.).contains(&v))
        {
            return Err(UiError::InvalidTextEffect);
        }
        Ok(())
    }
    /// Checks that the encoded spread covers the effect and its half-pixel antialias edge.
    pub fn validate_extent(
        self,
        corners: [[f32; 3]; 4],
        uv: [f32; 4],
        size: glam::UVec2,
    ) -> Result<(), UiError> {
        self.validate()?;
        validate_distance_mapping(corners, uv)?;
        if size.min_element() == 0 {
            return Err(UiError::InvalidDistanceMapping);
        }
        let point = |i: usize| DVec2::new(f64::from(corners[i][0]), f64::from(corners[i][1]));
        let a = (point(1) - point(0)) / ((f64::from(uv[2]) - f64::from(uv[0])) * f64::from(size.x));
        let b = (point(3) - point(0)) / ((f64::from(uv[3]) - f64::from(uv[1])) * f64::from(size.y));
        let t = a.length_squared() + b.length_squared();
        let determinant = a.perp_dot(b).abs();
        let maximum = ((t + (t * t - 4. * determinant * determinant).max(0.).sqrt()) * 0.5).sqrt();
        let scale = determinant / maximum;
        if f64::from(self.radius_px) + 0.5 > f64::from(self.spread_px) * scale {
            return Err(UiError::DistanceFieldLimit);
        }
        Ok(())
    }
}

/// Distance quads require one invertible affine pixel/UV mapping. Triangles are excluded.
pub fn validate_distance_mapping(corners: [[f32; 3]; 4], uv: [f32; 4]) -> Result<(), UiError> {
    if corners
        .into_iter()
        .flatten()
        .chain(uv)
        .any(|v| !v.is_finite())
        || uv[0] - uv[2] == 0.
        || uv[1] - uv[3] == 0.
    {
        return Err(UiError::InvalidDistanceMapping);
    }
    let p = corners.map(|p| DVec2::new(f64::from(p[0]), f64::from(p[1])));
    let a = p[1] - p[0];
    let b = p[3] - p[0];
    let determinant = a.perp_dot(b);
    // Closure uses four rounded coordinates per axis. Add their half-ULP errors
    // to the existing subpixel transform-arithmetic tolerance, not to the edges.
    let rounding = corners.into_iter().fold(DVec2::ZERO, |error, corner| {
        error
            + DVec2::new(
                coordinate_rounding(corner[0]),
                coordinate_rounding(corner[1]),
            )
    });
    let residual = ((p[2] - p[0] - a - b).abs() - rounding).max(DVec2::ZERO);
    if determinant == 0. || residual.length() > 0.001 {
        return Err(UiError::InvalidDistanceMapping);
    }
    Ok(())
}

fn coordinate_rounding(value: f32) -> f64 {
    let magnitude = value.abs();
    let value = f64::from(magnitude);
    let below = value - f64::from(magnitude.next_down());
    let above = f64::from(magnitude.next_up().min(f32::MAX)) - value;
    below.max(above) * 0.5
}

pub(super) fn effect_raster(radius: f32, scale: Vec2) -> Result<super::GlyphRaster, UiError> {
    let padding = ((radius + 2.) / scale.min_element()).ceil() + 1.;
    if !padding.is_finite() || padding > 127. {
        return Err(UiError::DistanceFieldLimit);
    }
    Ok(super::GlyphRaster {
        mode: super::RenderMode::Sdf,
        padding: Vec2::splat(padding).as_uvec2().x,
    })
}
