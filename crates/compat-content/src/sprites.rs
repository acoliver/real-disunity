use crate::{FallbackSprite, ImportError};
use std::collections::BTreeSet;

pub(crate) fn validate_fallbacks(sprites: &[FallbackSprite]) -> Result<(), ImportError> {
    let mut names = BTreeSet::new();
    let mut total = 0;
    for sprite in sprites {
        total += fallback_length(sprite)?;
        if !names.insert(&sprite.name) || total > 16 * 1024 * 1024 {
            return Err(ImportError::Limit(
                "duplicate or oversized fallback sprites".into(),
            ));
        }
    }
    Ok(())
}
fn fallback_length(sprite: &FallbackSprite) -> Result<usize, ImportError> {
    let [width, height] = sprite.texture_size.map(u64::from);
    let bytes = width.checked_mul(height).and_then(|n| n.checked_mul(4));
    let metrics = &sprite.sprite;
    let [x, y, w, h] = metrics.rect;
    if sprite.name.trim().is_empty()
        || sprite.reason.trim().is_empty()
        || width == 0
        || height == 0
        || bytes.is_none_or(|n| n > 16 * 1024 * 1024)
        || !metrics.pixels_per_unit.is_finite()
        || metrics.pixels_per_unit <= 0.
        || metrics
            .rect
            .iter()
            .chain(&metrics.pivot)
            .chain(&metrics.border)
            .any(|n| !n.is_finite())
        || x < 0.
        || y < 0.
        || w <= 0.
        || h <= 0.
        || x + w > f64::from(sprite.texture_size[0])
        || y + h > f64::from(sprite.texture_size[1])
    {
        return Err(ImportError::Source {
            path: "sidecar.fallback_sprites".into(),
            message:
                "fallback requires unique name, reason, bounded pixels and valid sprite metrics"
                    .into(),
        });
    }
    usize::try_from(bytes.expect("checked byte length"))
        .map_err(|_| ImportError::Limit("fallback pixel bytes".into()))
}
pub(crate) fn fallback_pixels(sprite: &FallbackSprite) -> Result<Vec<u8>, ImportError> {
    let length = fallback_length(sprite)?;
    Ok(sprite.rgba.repeat(length / 4))
}

use crate::{
    SpriteAsset,
    native::{number, vector},
};
use serde_json::Value;

pub(crate) fn extract(metadata: &Value, size: Option<[u32; 2]>) -> Vec<SpriteAsset> {
    let importer = &metadata["TextureImporter"];
    let ppu = number(&importer["spritePixelsToUnits"], 100.);
    let mut result = Vec::new();
    if let Some(sprites) = importer["spriteSheet"]["sprites"].as_array() {
        for sprite in sprites {
            let name = sprite["name"].as_str().unwrap_or("");
            let id = sprite["internalID"]
                .as_i64()
                .or_else(|| importer["spriteSheet"]["nameFileIdTable"][name].as_i64())
                .or_else(|| {
                    importer["fileIDToRecycleName"]
                        .as_object()?
                        .iter()
                        .find(|(_, v)| v.as_str() == Some(name))?
                        .0
                        .parse()
                        .ok()
                });
            if let Some(file_id) = id {
                result.push(SpriteAsset {
                    file_id,
                    name: name.into(),
                    rect: vector(&sprite["rect"], ["x", "y", "width", "height"], [0.; 4]),
                    pivot: vector(&sprite["pivot"], ["x", "y"], [0.5; 2]),
                    pixels_per_unit: ppu,
                    border: vector(&sprite["border"], ["x", "y", "z", "w"], [0.; 4]),
                });
            }
        }
    }
    // Older imports retain stable subasset aliases in nameFileIdTable after a rename.
    // Each alias describes the same original sprite rectangle and pivot.
    if let Some(table) = importer["spriteSheet"]["nameFileIdTable"].as_object() {
        for (name, id) in table {
            if let Some(file_id) = id.as_i64()
                && !result.iter().any(|s| s.file_id == file_id)
                && let Some(sprite) = result.iter().find(|s| s.name == *name).cloned()
            {
                result.push(SpriteAsset { file_id, ..sprite });
            }
        }
    }
    if importer["spriteMode"].as_i64() == Some(1)
        && let Some([width, height]) = size
    {
        result.push(SpriteAsset {
            file_id: 21_300_000,
            name: String::new(),
            rect: [0., 0., f64::from(width), f64::from(height)],
            pivot: vector(&importer["spritePivot"], ["x", "y"], [0.5; 2]),
            pixels_per_unit: ppu,
            border: vector(&importer["spriteBorder"], ["x", "y", "z", "w"], [0.; 4]),
        });
    }
    result.sort_by_key(|s| s.file_id);
    result
}
