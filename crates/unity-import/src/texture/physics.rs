//! A sprite's physics shape, which a tile with the Sprite collider type collides with.

use super::{Image, Sprite, Texture, TextureError};

impl Texture {
    /// The physics shape of sprite `file_id`: convex counter-clockwise paths in world units,
    /// around the sprite's pivot.
    ///
    /// With no authored shape, Unity generates one from the sprite's alpha when the importer
    /// allows a fallback, and has none otherwise. A fully opaque sprite's generated shape is its
    /// rectangle, which is the case the importer reproduces. Authored shapes and outlines traced
    /// from transparent pixels are rejected rather than approximated.
    pub fn physics_shape(&self, file_id: i64) -> Result<Vec<Vec<[f64; 2]>>, TextureError> {
        let sprite = self.sprite(file_id).ok_or_else(|| TextureError::NoSprite {
            path: self.path.clone(),
            file_id,
        })?;
        let error = |message: &str| {
            TextureError::invalid(&self.path, format!("sprite `{}`: {message}", sprite.name))
        };
        if sprite.authored_physics_shape {
            return Err(error("authored physics shapes are not supported"));
        }
        if !self.settings.generate_fallback_physics_shape {
            return Ok(Vec::new());
        }
        if !opaque(&self.image, sprite) {
            return Err(error(
                "a generated physics shape needs a fully opaque sprite; tracing outlines from alpha is not supported",
            ));
        }
        Ok(vec![rectangle(sprite)])
    }
}

/// Whether every texel the sprite's rect overlaps has full alpha.
fn opaque(image: &Image, sprite: &Sprite) -> bool {
    let [x, y, width, height] = sprite.rect;
    let overlaps = |texel: u32, start: f64, length: f64| {
        let low = f64::from(texel);
        low + 1.0 > start && low < start + length
    };
    let columns: Vec<u32> = (0..image.width)
        .filter(|column| overlaps(*column, x, width))
        .collect();
    (0..image.height)
        .filter(|row| overlaps(*row, y, height))
        .all(|row| {
            // Rect rows count from the bottom; image rows from the top.
            let texel_row = image.height - 1 - row;
            columns.iter().all(|column| {
                let alpha =
                    (u64::from(texel_row) * u64::from(image.width) + u64::from(*column)) * 4 + 3;
                usize::try_from(alpha)
                    .ok()
                    .and_then(|index| image.rgba8.get(index))
                    == Some(&u8::MAX)
            })
        })
}

fn rectangle(sprite: &Sprite) -> Vec<[f64; 2]> {
    let [_, _, width, height] = sprite.rect.map(|v| v / sprite.pixels_per_unit);
    let [pivot_x, pivot_y] = sprite.pivot;
    let (left, right) = (-pivot_x * width, (1.0 - pivot_x) * width);
    let (bottom, top) = (-pivot_y * height, (1.0 - pivot_y) * height);
    vec![[left, bottom], [right, bottom], [right, top], [left, top]]
}
