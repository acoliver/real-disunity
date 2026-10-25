use super::{UiError, UiLimits};
use ab_glyph::OutlinedGlyph;
use glam::{UVec2, Vec2};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GlyphAtlas {
    /// Alpha coverage, row-major, top row first. No color encoding is applied.
    pub alpha: Vec<u8>,
    pub size: UVec2,
}

pub(super) struct AtlasBuilder {
    atlas: GlyphAtlas,
    cursor: UVec2,
    row_height: u32,
    limits: UiLimits,
}
impl AtlasBuilder {
    pub fn new(limits: UiLimits) -> Self {
        Self {
            atlas: GlyphAtlas::default(),
            cursor: UVec2::ZERO,
            row_height: 0,
            limits,
        }
    }
    pub fn insert(
        &mut self,
        glyph: &OutlinedGlyph,
        min: Vec2,
        max: Vec2,
    ) -> Result<(UVec2, UVec2), UiError> {
        let extent = max - min;
        if !extent.is_finite()
            || extent.min_element() <= 0.
            || extent.max_element()
                > f32::from(u16::try_from(self.limits.max_atlas_side).expect("bounded atlas"))
        {
            return Err(UiError::AtlasLimit);
        }
        let size = extent.as_uvec2();
        let stride = self.limits.max_atlas_side;
        if self.cursor.x + size.x > stride {
            self.cursor.x = 0;
            self.cursor.y += self.row_height;
            self.row_height = 0;
        }
        let end = self.cursor + size;
        let height = self.atlas.size.y.max(end.y);
        let count = u64::from(stride) * u64::from(height);
        if end.y > stride || count > self.limits.max_atlas_pixels as u64 {
            return Err(UiError::AtlasLimit);
        }
        self.atlas
            .alpha
            .resize(usize::try_from(count).map_err(|_| UiError::AtlasLimit)?, 0);
        self.atlas.size = UVec2::new(stride, height);
        let start = self.cursor;
        let bounds = glyph.px_bounds();
        let outline_size = Vec2::new(bounds.max.x - bounds.min.x, bounds.max.y - bounds.min.y);
        let maximum = f32::from(u16::try_from(stride).expect("bounded atlas"));
        if !outline_size.is_finite() || outline_size.max_element() > maximum {
            return Err(UiError::AtlasLimit);
        }
        glyph.draw(|x, y, coverage| {
            let dx = f32::from(u16::try_from(x).expect("bounded outline")) + bounds.min.x - min.x;
            let dy = f32::from(u16::try_from(y).expect("bounded outline")) + bounds.min.y - min.y;
            if dx >= 0. && dy >= 0. && dx < extent.x && dy < extent.y {
                let px = Vec2::new(dx, dy).as_uvec2().x + start.x;
                let py = Vec2::new(dx, dy).as_uvec2().y + start.y;
                let index = u64::from(py) * u64::from(stride) + u64::from(px);
                self.atlas.alpha[usize::try_from(index).expect("bounded atlas index")] =
                    u8::try_from(Vec2::splat((coverage.min(1.) * 255.).round()).as_uvec2().x)
                        .expect("coverage in 0..255");
            }
        });
        self.cursor.x = end.x;
        self.row_height = self.row_height.max(size.y);
        Ok((start, size))
    }
    pub fn finish(self) -> GlyphAtlas {
        self.atlas
    }
}
