use std::{collections::BTreeSet, path::Path};

use super::{
    Image, SINGLE_SPRITE_FILE_ID,
    settings::{SpriteMode, TextureSettings, float, int, vector2},
};
use crate::yaml::{Mapping, Node};

/// Unity's `SpriteAlignment::Custom`: the only alignment that uses the authored pivot.
const CUSTOM_ALIGNMENT: i64 = 9;

/// A sprite: a rectangle of a texture with a pivot and a world scale.
#[derive(Clone, Debug, PartialEq)]
pub struct Sprite {
    pub name: String,
    /// The fileID references to this sprite use.
    pub file_id: i64,
    /// `x, y, width, height` in pixels, with the origin at the texture's bottom-left corner.
    pub rect: [f64; 4],
    /// Normalized pivot inside `rect`, from its bottom-left corner.
    pub pivot: [f64; 2],
    pub pixels_per_unit: f64,
    /// Nine-slice border: left, bottom, right, top, in pixels.
    pub border: [f64; 4],
}

pub(super) fn read(
    path: &Path,
    importer: &Mapping,
    settings: &TextureSettings,
    image: &Image,
) -> Result<Vec<Sprite>, String> {
    let sprites = match settings.sprite_mode {
        SpriteMode::None => Vec::new(),
        SpriteMode::Single => vec![single(path, settings, image)?],
        SpriteMode::Multiple => multiple(importer, settings, image)?,
    };
    let mut ids = BTreeSet::new();
    for sprite in &sprites {
        if !ids.insert(sprite.file_id) {
            return Err(format!("sprite fileID {} is used twice", sprite.file_id));
        }
    }
    Ok(sprites)
}

fn single(path: &Path, settings: &TextureSettings, image: &Image) -> Result<Sprite, String> {
    let name = path
        .file_stem()
        .map_or_else(String::new, |stem| stem.to_string_lossy().into_owned());
    Ok(Sprite {
        name,
        file_id: SINGLE_SPRITE_FILE_ID,
        rect: [0.0, 0.0, f64::from(image.width), f64::from(image.height)],
        pivot: pivot(settings.alignment, settings.pivot)?,
        pixels_per_unit: settings.pixels_per_unit,
        border: [0.0; 4],
    })
}

fn multiple(
    importer: &Mapping,
    settings: &TextureSettings,
    image: &Image,
) -> Result<Vec<Sprite>, String> {
    let entries = importer
        .get("spriteSheet")
        .and_then(|sheet| sheet.get("sprites"))
        .and_then(Node::as_sequence)
        .unwrap_or_default();
    entries
        .iter()
        .map(|entry| {
            let sprite = entry_sprite(importer, settings, entry)?;
            check_rect(&sprite, image)?;
            Ok(sprite)
        })
        .collect()
}

fn entry_sprite(
    importer: &Mapping,
    settings: &TextureSettings,
    entry: &Node,
) -> Result<Sprite, String> {
    let name = entry
        .get("name")
        .and_then(Node::as_scalar)
        .ok_or("sprite sheet entry has no name")?
        .text()
        .to_owned();
    let context = |message: String| format!("sprite `{name}`: {message}");
    let rect = entry.get("rect").ok_or_else(|| context("no rect".into()))?;
    let rect = ["x", "y", "width", "height"]
        .map(|key| float(rect.get(key), f64::NAN))
        .into_iter()
        .collect::<Result<Vec<_>, _>>()
        .map_err(context)?;
    let border = entry.get("border");
    let border = ["x", "y", "z", "w"]
        .map(|key| float(border.and_then(|b| b.get(key)), 0.0))
        .into_iter()
        .collect::<Result<Vec<_>, _>>()
        .map_err(context)?;
    let alignment = int(entry.get("alignment"), 0).map_err(context)?;
    let custom = vector2(entry.get("pivot"), [0.5, 0.5]).map_err(context)?;
    Ok(Sprite {
        file_id: file_id(importer, entry, &name).map_err(context)?,
        rect: [rect[0], rect[1], rect[2], rect[3]],
        pivot: pivot(alignment, custom).map_err(context)?,
        pixels_per_unit: settings.pixels_per_unit,
        border: [border[0], border[1], border[2], border[3]],
        name,
    })
}

/// Current Unity stores each sprite's `internalID`. Unity 2017 maps fileIDs to sprite names in
/// `fileIDToRecycleName`.
fn file_id(importer: &Mapping, entry: &Node, name: &str) -> Result<i64, String> {
    let internal = int(entry.get("internalID"), 0)?;
    if internal != 0 {
        return Ok(internal);
    }
    importer
        .get("fileIDToRecycleName")
        .and_then(Node::as_mapping)
        .into_iter()
        .flat_map(Mapping::iter)
        .find(|(_, value)| value.as_scalar().is_some_and(|s| s.text() == name))
        .and_then(|(key, _)| key.parse().ok())
        .ok_or_else(|| "has neither an internalID nor a recycled fileID".to_owned())
}

/// The normalized pivot of `SpriteAlignment`; `custom` applies only to the Custom alignment.
fn pivot(alignment: i64, custom: [f64; 2]) -> Result<[f64; 2], String> {
    Ok(match alignment {
        0 => [0.5, 0.5],
        1 => [0.0, 1.0],
        2 => [0.5, 1.0],
        3 => [1.0, 1.0],
        4 => [0.0, 0.5],
        5 => [1.0, 0.5],
        6 => [0.0, 0.0],
        7 => [0.5, 0.0],
        8 => [1.0, 0.0],
        CUSTOM_ALIGNMENT => custom,
        other => return Err(format!("sprite alignment {other} is not supported")),
    })
}

fn check_rect(sprite: &Sprite, image: &Image) -> Result<(), String> {
    let [x, y, width, height] = sprite.rect;
    let inside = x >= 0.0
        && y >= 0.0
        && width > 0.0
        && height > 0.0
        && x + width <= f64::from(image.width)
        && y + height <= f64::from(image.height);
    if inside {
        Ok(())
    } else {
        Err(format!(
            "sprite `{}` rect {:?} is outside the {}x{} image",
            sprite.name, sprite.rect, image.width, image.height
        ))
    }
}
