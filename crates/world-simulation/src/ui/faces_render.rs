//! Rendering a glyph into a caller's single-channel atlas. Atlas rows run from bottom to top, as
//! texture rows do, so the glyph's top lands on the rectangle's highest row. A rectangle includes
//! `padding` pixels on every side; the glyph's bounding box starts at the padding's inner corner.
//! Coverage mode writes antialiased coverage (0 to 255). Distance mode writes a signed distance
//! field: the glyph is rasterized at 4 times the size and thresholded at half coverage, the exact
//! distance to the nearest opposite pixel is found by 8SSEDT (see `sdf`), and each pixel centre
//! takes the distance of the high-resolution pixel at its middle. The padding is the spread:
//! the edge is 128, `padding` pixels inside is 255 and `padding` pixels outside is 0.

use super::{
    UiError,
    faces::{FaceBook, FaceId, LoadedFace, glyph_id},
    sdf::signed_distance,
};
use ab_glyph::{Font, FontRef, Glyph, PxScale, point};
use glam::{DVec2, Vec2};

/// What a glyph is drawn as.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RenderMode {
    Coverage,
    Sdf,
}

/// A rectangle of whole pixels in an atlas.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GlyphRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// One glyph to draw: where, how, and with how much padding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GlyphJob {
    pub glyph: u32,
    pub mode: RenderMode,
    pub padding: u32,
    pub rect: GlyphRect,
}

/// The longest side of a coverage rectangle, and of a distance rectangle (drawn 4 times larger).
const MAX_SIDE: u32 = 1024;
const MAX_SDF_SIDE: u32 = 256;
const SDF_SCALE: u32 = 4;

fn small(value: u32) -> f32 {
    f32::from(u16::try_from(value).expect("rectangles are at most 1024 pixels"))
}

fn to_byte(value: f64) -> u8 {
    u8::try_from(DVec2::splat(value.clamp(0., 255.).round()).as_uvec2().x).unwrap_or(u8::MAX)
}

impl FaceBook {
    /// Draws `job.glyph` into `atlas`, a `width` by `height` image of one byte per pixel, filling
    /// the whole rectangle (the padding included). Pixels outside the rectangle are untouched.
    pub fn render_glyph(
        &self,
        id: FaceId,
        job: GlyphJob,
        atlas: &mut [u8],
        width: u32,
        height: u32,
    ) -> Result<(), UiError> {
        let stored = self.get(id)?;
        let limit = match job.mode {
            RenderMode::Coverage => MAX_SIDE,
            RenderMode::Sdf => MAX_SDF_SIDE,
        };
        check_rect(&job, limit, atlas.len(), width, height)?;
        glyph_id(&stored.parse(), job.glyph)?;
        let scale = match job.mode {
            RenderMode::Coverage => 1,
            RenderMode::Sdf => SDF_SCALE,
        };
        let coverage = coverage_bitmap(stored, &job, scale)?;
        let bytes = match job.mode {
            RenderMode::Coverage => coverage
                .iter()
                .map(|&c| to_byte(f64::from(c) * 255.))
                .collect(),
            RenderMode::Sdf => distance_bytes(&coverage, &job),
        };
        store(atlas, width, &job.rect, &bytes);
        Ok(())
    }
}

fn check_rect(
    job: &GlyphJob,
    limit: u32,
    length: usize,
    width: u32,
    height: u32,
) -> Result<(), UiError> {
    let rect = job.rect;
    let fits =
        |origin: u32, size: u32, bound: u32| origin.checked_add(size).is_some_and(|e| e <= bound);
    let padded = job
        .padding
        .checked_mul(2)
        .is_some_and(|p| p < rect.width && p < rect.height);
    let pixels = u64::from(width) * u64::from(height);
    if rect.width == 0
        || rect.height == 0
        || rect.width > limit
        || rect.height > limit
        || !fits(rect.x, rect.width, width)
        || !fits(rect.y, rect.height, height)
        || !padded
        || u64::try_from(length).map_or(true, |l| l < pixels)
    {
        return Err(UiError::AtlasLimit);
    }
    Ok(())
}

/// Coverage of the glyph, top row first, over the rectangle at `scale` times its size.
fn coverage_bitmap(face: &LoadedFace, job: &GlyphJob, scale: u32) -> Result<Vec<f32>, UiError> {
    let font = FontRef::try_from_slice_and_index(&face.data, face.index)
        .map_err(|_| UiError::InvalidFont)?;
    let parsed = face.parse();
    let units = face.point_size * small(scale) / f32::from(parsed.units_per_em());
    let id = glyph_id(&parsed, job.glyph)?;
    let (width, height) = (job.rect.width * scale, job.rect.height * scale);
    let mut bitmap = vec![0.; usize::try_from(width * height).map_err(|_| UiError::AtlasLimit)?];
    let Some(bounds) = parsed.glyph_bounding_box(id) else {
        return Ok(bitmap);
    };
    // ab_glyph scales by the font's ascent-minus-descent height, so a scale of `height * units`
    // gives `units` pixels per font unit.
    let (left, top) = (
        f32::from(bounds.x_min) * units,
        f32::from(bounds.y_max) * units,
    );
    let inner = small(job.padding * scale);
    let glyph = Glyph {
        id: ab_glyph::GlyphId(id.0),
        scale: PxScale::from(font.height_unscaled() * units),
        position: point(inner - left, inner + top),
    };
    let Some(outline) = font.outline_glyph(glyph) else {
        return Ok(bitmap);
    };
    let origin = outline.px_bounds().min;
    let origin = Vec2::new(origin.x.floor(), origin.y.floor()).as_ivec2();
    let stride = usize::try_from(width).map_err(|_| UiError::AtlasLimit)?;
    outline.draw(|x, y, c| {
        let column = i64::from(origin.x) + i64::from(x);
        let row = i64::from(origin.y) + i64::from(y);
        if (0..i64::from(width)).contains(&column) && (0..i64::from(height)).contains(&row) {
            let at =
                usize::try_from(row).unwrap_or(0) * stride + usize::try_from(column).unwrap_or(0);
            bitmap[at] = c;
        }
    });
    Ok(bitmap)
}

/// The distance field of an oversampled coverage bitmap, one byte per final pixel, top row first.
fn distance_bytes(coverage: &[f32], job: &GlyphJob) -> Vec<u8> {
    let scale = usize::try_from(SDF_SCALE).expect("small scale");
    let (width, height) = (
        usize::try_from(job.rect.width).expect("small") * scale,
        usize::try_from(job.rect.height).expect("small") * scale,
    );
    let inside: Vec<bool> = coverage.iter().map(|&c| c >= 0.5).collect();
    let distance = signed_distance(&inside, width, height);
    let spread = f64::from(job.padding.max(1));
    let (columns, rows) = (width / scale, height / scale);
    let mut bytes = Vec::with_capacity(columns * rows);
    for row in 0..rows {
        for column in 0..columns {
            let cell = (row * scale + scale / 2) * width + column * scale + scale / 2;
            let pixels = distance[cell] / f64::from(SDF_SCALE);
            bytes.push(to_byte((0.5 + pixels / (2. * spread)) * 255.));
        }
    }
    bytes
}

/// Copies a rectangle's rows, top row first, into the atlas with the top row on the highest row.
fn store(atlas: &mut [u8], atlas_width: u32, rect: &GlyphRect, bytes: &[u8]) {
    let stride = usize::try_from(atlas_width).expect("atlas width");
    let (x, y) = (
        usize::try_from(rect.x).expect("x"),
        usize::try_from(rect.y).expect("y"),
    );
    let (w, h) = (
        usize::try_from(rect.width).expect("width"),
        usize::try_from(rect.height).expect("height"),
    );
    for (row, line) in bytes.chunks_exact(w).enumerate() {
        let target = (y + h - 1 - row) * stride + x;
        atlas[target..target + w].copy_from_slice(line);
    }
}
