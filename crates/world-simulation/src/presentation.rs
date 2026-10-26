//! Typed native presentation state. Textures are immutable content identities.
use crate::{ComponentError, WorldError};
use glam::{Mat4, Vec3};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sprite {
    /// Zero requests a fresh identity when registered in a world.
    pub identity: u64,
    pub texture: u64,
    /// Pixel rectangle in bottom-left texture coordinates.
    pub rect: [f32; 4],
    pub pivot: [f32; 2],
    pub pixels_per_unit: f32,
    pub texture_size: [u32; 2],
}
impl Sprite {
    /// Registers immutable content before sharing it between native renderers.
    pub fn register(mut self) -> Result<Self, WorldError> {
        self.validate()?;
        if self.identity == 0 {
            self.identity = allocate_sprite_identity()?;
        }
        Ok(self)
    }

    pub fn bounds(self) -> [f32; 4] {
        let [_, _, width, height] = self.rect;
        let [px, py] = self.pivot;
        [
            -px * width / self.pixels_per_unit,
            -py * height / self.pixels_per_unit,
            (1. - px) * width / self.pixels_per_unit,
            (1. - py) * height / self.pixels_per_unit,
        ]
    }
    pub fn validate(self) -> Result<(), WorldError> {
        let [x, y, w, h] = self.rect;
        if !self.rect.into_iter().chain(self.pivot).all(f32::is_finite)
            || !self.pixels_per_unit.is_finite()
            || self.pixels_per_unit <= 0.
            || x < 0.
            || y < 0.
            || w <= 0.
            || h <= 0.
            || self.texture_size.contains(&0)
            || f64::from(x) + f64::from(w) > f64::from(self.texture_size[0])
            || f64::from(y) + f64::from(h) > f64::from(self.texture_size[1])
        {
            return Err(invalid());
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct SpriteRenderer {
    pub lighting: SpriteLighting,
    pub enabled: bool,
    pub sprite: Option<Sprite>,
    /// Linear straight-alpha tint.
    pub color: [f32; 4],
    pub flip_x: bool,
    pub flip_y: bool,
    pub sorting_layer: i32,
    pub sorting_order: i32,
}
impl Default for SpriteRenderer {
    fn default() -> Self {
        Self {
            lighting: SpriteLighting::Unlit,
            enabled: true,
            sprite: None,
            color: [1.; 4],
            flip_x: false,
            flip_y: false,
            sorting_layer: 0,
            sorting_order: 0,
        }
    }
}
impl SpriteRenderer {
    pub(crate) fn validate(&self) -> Result<(), WorldError> {
        color(self.color)?;
        if let Some(sprite) = self.sprite {
            sprite.validate()?;
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct Tile {
    pub sprite: Sprite,
    pub transform: Mat4,
    pub color: [f32; 4],
}
#[derive(Clone, Debug, PartialEq)]
pub struct Tilemap {
    pub enabled: bool,
    pub tiles: Vec<Tile>,
    pub color: [f32; 4],
    pub sorting_layer: i32,
    pub sorting_order: i32,
}
impl Default for Tilemap {
    fn default() -> Self {
        Self {
            enabled: true,
            tiles: vec![],
            color: [1.; 4],
            sorting_layer: 0,
            sorting_order: 0,
        }
    }
}
impl Tilemap {
    pub(crate) fn validate(&self) -> Result<(), WorldError> {
        color(self.color)?;
        if self.tiles.len() > 65_536 {
            return Err(invalid());
        }
        for tile in &self.tiles {
            tile.sprite.validate()?;
            color(tile.color)?;
            if !tile.transform.is_finite() || tile.transform.row(3) != glam::Vec4::W {
                return Err(invalid());
            }
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    pub enabled: bool,
    pub half_height: f32,
    pub near: f32,
    pub far: f32,
    pub clear: [f64; 4],
    pub depth: i32,
}
impl Default for Camera {
    fn default() -> Self {
        Self {
            enabled: true,
            half_height: 5.,
            near: 0.01,
            far: 1000.,
            clear: [0., 0., 0., 1.],
            depth: 0,
        }
    }
}
impl Camera {
    pub fn validate(self) -> Result<(), WorldError> {
        if ![self.half_height, self.near, self.far]
            .into_iter()
            .all(f32::is_finite)
            || self.half_height <= 0.
            || self.near < 0.
            || self.far <= self.near
            || self
                .clear
                .iter()
                .any(|v| !v.is_finite() || !(0. ..=1.).contains(v))
        {
            return Err(invalid());
        }
        Ok(())
    }
}
pub fn sprite_corners(sprite: Sprite, matrix: Mat4, flip_x: bool, flip_y: bool) -> [[f32; 3]; 4] {
    let [left, bottom, right, top] = sprite.bounds();
    let sx = if flip_x { -1. } else { 1. };
    let sy = if flip_y { -1. } else { 1. };
    [[left, bottom], [right, bottom], [right, top], [left, top]].map(|[x, y]| {
        matrix
            .transform_point3(Vec3::new(x * sx, y * sy, 0.))
            .to_array()
    })
}
fn color(value: [f32; 4]) -> Result<(), WorldError> {
    if value
        .iter()
        .any(|v| !v.is_finite() || !(0. ..=1.).contains(v))
    {
        Err(invalid())
    } else {
        Ok(())
    }
}
fn invalid() -> WorldError {
    WorldError::InvalidComponent(ComponentError::InvalidPresentation)
}

pub mod extraction;
/// Owned linear texels: single-channel input coverage or RGBA prepared coverage.
#[derive(Clone, Debug, PartialEq)]
pub struct CoverageTexture {
    pub width: u32,
    pub height: u32,
    pub bytes: Box<[u8]>,
}

pub fn allocate_sprite_identity() -> Result<u64, WorldError> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
        .map_err(|_| invalid())
}

impl crate::ComponentDesc {
    pub(crate) fn register_presentation(&mut self) -> Result<(), WorldError> {
        match self {
            Self::Sprite(renderer) => {
                renderer.sprite = renderer.sprite.map(Sprite::register).transpose()?;
            }
            Self::Tilemap(map) => {
                for tile in &mut map.tiles {
                    tile.sprite = tile.sprite.register()?;
                }
            }
            _ => {}
        }
        Ok(())
    }
}

pub mod camera_data;
pub mod lighting;
pub use camera_data::UniversalAdditionalCameraData;
pub use lighting::{GlobalLight2d, Light2dType, SpriteLighting};
