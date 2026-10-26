//! Textures that ship in Unity packages rather than in the project.
//!
//! Their files cannot be redistributed, so each is described from its importer settings and
//! pixels in the package as Unity ships it.

use std::path::PathBuf;

use super::{Filter, Image, Sprite, SpriteMode, Texture, TextureSettings, Wrap};
use crate::{guid::Guid, package::PackageSprite};

impl Texture {
    /// The package texture `guid`, if it is a known one.
    pub(super) fn package(guid: Guid) -> Option<Self> {
        match PackageSprite::of_texture(guid)? {
            PackageSprite::Square => Some(square()),
        }
    }
}

/// `v2/Square.png` of `com.unity.2d.sprite` 1.0.0 in Unity 6000.3.5f2: a 256x256 RGB image in
/// which every pixel is white, imported as one sRGB, bilinear, clamped sprite of the whole image
/// at 256 pixels per unit, centred, with no border and Unity's generated physics shape.
fn square() -> Texture {
    const SIZE: u32 = 256;
    let side = f64::from(SIZE);
    let settings = TextureSettings {
        sprite_mode: SpriteMode::Multiple,
        pixels_per_unit: side,
        filter: Filter::Bilinear,
        wrap: [Wrap::Clamp; 2],
        srgb: true,
        max_size: 2048,
        generate_fallback_physics_shape: true,
        alignment: 0,
        pivot: [0.5, 0.5],
    };
    let sprite = Sprite {
        name: "Square".into(),
        file_id: PackageSprite::Square.file_id(),
        rect: [0.0, 0.0, side, side],
        pivot: [0.5, 0.5],
        pixels_per_unit: side,
        border: [0.0; 4],
        authored_physics_shape: false,
    };
    Texture {
        path: PathBuf::from(
            "Packages/com.unity.2d.sprite/Editor/ObjectMenuCreation/DefaultAssets/Textures/v2/Square.png",
        ),
        settings,
        image: Image {
            width: SIZE,
            height: SIZE,
            rgba8: vec![u8::MAX; (SIZE * SIZE * 4) as usize],
        },
        sprites: vec![sprite],
    }
}
