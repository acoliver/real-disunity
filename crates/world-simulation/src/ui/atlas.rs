use super::{UiError, UiLimits};
use ab_glyph::OutlinedGlyph;
use glam::{UVec2, Vec2};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GlyphAtlas {
    /// Shared coverage or distance bytes, row-major, top row first. No colour encoding.
    pub alpha: std::sync::Arc<[u8]>,
    pub size: UVec2,
}

pub(super) struct AtlasBuilder {
    atlas: GlyphAtlas,
    pixels: Vec<u8>,
    cursor: UVec2,
    row_height: u32,
    limits: UiLimits,
}
impl AtlasBuilder {
    pub fn new(limits: UiLimits) -> Self {
        Self {
            atlas: GlyphAtlas::default(),
            pixels: Vec::new(),
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
        let start = self.reserve(size)?;
        let stride = self.limits.max_atlas_side;
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
                self.pixels[usize::try_from(index).expect("bounded atlas index")] =
                    u8::try_from(Vec2::splat((coverage.min(1.) * 255.).round()).as_uvec2().x)
                        .expect("coverage in 0..255");
            }
        });
        Ok((start, size))
    }
    fn reserve(&mut self, size: UVec2) -> Result<UVec2, UiError> {
        if size.max_element() > self.limits.max_atlas_side {
            return Err(UiError::AtlasLimit);
        }
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
        self.pixels
            .resize(usize::try_from(count).map_err(|_| UiError::AtlasLimit)?, 0);
        self.atlas.size = UVec2::new(stride, height);
        let start = self.cursor;
        self.cursor.x = end.x;
        self.row_height = self.row_height.max(size.y);
        Ok(start)
    }
    pub fn insert_distance(
        &mut self,
        font: &ab_glyph::FontArc,
        glyph: &OutlinedGlyph,
        min: Vec2,
        max: Vec2,
        padding: u32,
    ) -> Result<(UVec2, UVec2), UiError> {
        use ab_glyph::Font;
        let size = (max - min).as_uvec2();
        super::raster::check_distance(size, self.limits.max_atlas_pixels)?;
        let start = self.reserve(size + UVec2::splat(2))? + UVec2::ONE;
        let mut high = glyph.glyph().clone();
        high.scale.x *= 4.;
        high.scale.y *= 4.;
        high.position.x *= 4.;
        high.position.y *= 4.;
        let outline = font.outline_glyph(high).ok_or(UiError::InvalidGlyph)?;
        let coverage = super::raster::coverage(&outline, size * 4, min * 4.);
        let bytes = super::raster::distance_bytes(&coverage, size, padding);
        let stride = usize::try_from(self.limits.max_atlas_side).expect("bounded atlas");
        for (row, line) in bytes
            .chunks_exact(usize::try_from(size.x).expect("small raster"))
            .enumerate()
        {
            let offset = (usize::try_from(start.y).expect("atlas row") + row) * stride
                + usize::try_from(start.x).expect("atlas column");
            self.pixels[offset..offset + line.len()].copy_from_slice(line);
        }
        Ok((start, size))
    }
    pub fn finish(mut self) -> GlyphAtlas {
        self.atlas.alpha = self.pixels.into();
        self.atlas
    }
}
