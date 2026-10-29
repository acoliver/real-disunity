//! A Unity `Tilemap`: a rectangular grid layout, a palette of tiles, and the cells that use
//! them. Rendering and `TilemapCollider2D` both read this one description, so editing a cell
//! changes what is drawn and what collides together.
use crate::{
    WorldError,
    physics2d::{PhysicsError, Shape2d},
    presentation::{Sprite, SpriteLighting},
};
use glam::{Mat4, Vec2, Vec3};

const MAX_TILES: usize = 4096;
const MAX_CELLS: usize = 65_536;
const MAX_SHAPE_PATHS: usize = 16;

/// A rectangular `Grid` with XY swizzle, and the tilemap's tile anchor as a fraction of a cell.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TileLayout {
    pub cell_size: Vec2,
    pub cell_gap: Vec2,
    pub tile_anchor: Vec2,
}
impl Default for TileLayout {
    fn default() -> Self {
        Self {
            cell_size: Vec2::ONE,
            cell_gap: Vec2::ZERO,
            tile_anchor: Vec2::splat(0.5),
        }
    }
}

/// Unity's `Tile.ColliderType`. `Sprite` carries the sprite's physics shape: convex paths in
/// sprite-local units around the pivot, as `Sprite.GetPhysicsShape` returns them.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum TileCollider {
    #[default]
    None,
    Sprite(Vec<Vec<Vec2>>),
    Grid,
}

/// One entry of the tilemap's palette, Unity's `m_TileAssetArray`.
#[derive(Clone, Debug, PartialEq)]
pub struct Tile {
    pub sprite: Option<Sprite>,
    pub color: [f32; 4],
    /// Tile-local transform applied around the anchored cell point.
    pub transform: Mat4,
    pub collider: TileCollider,
}
impl Default for Tile {
    fn default() -> Self {
        Self {
            sprite: None,
            color: [1.; 4],
            transform: Mat4::IDENTITY,
            collider: TileCollider::None,
        }
    }
}

/// A cell holding the palette entry `tile`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TileCell {
    pub position: [i32; 2],
    pub tile: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Tilemap {
    pub enabled: bool,
    pub color: [f32; 4],
    pub sorting_layer: i32,
    pub sorting_order: i32,
    /// The `TilemapRenderer` material: lit tiles take the scene's global 2D lights on their
    /// sorting layer, as lit sprites do.
    pub lighting: SpriteLighting,
    pub layout: TileLayout,
    pub tiles: Vec<Tile>,
    pub cells: Vec<TileCell>,
}
impl Default for Tilemap {
    fn default() -> Self {
        Self {
            enabled: true,
            color: [1.; 4],
            sorting_layer: 0,
            sorting_order: 0,
            lighting: SpriteLighting::Unlit,
            layout: TileLayout::default(),
            tiles: Vec::new(),
            cells: Vec::new(),
        }
    }
}

fn invalid() -> WorldError {
    WorldError::InvalidComponent(crate::ComponentError::InvalidPresentation)
}

fn color(value: [f32; 4]) -> Result<(), WorldError> {
    if value
        .iter()
        .all(|v| v.is_finite() && (0. ..=1.).contains(v))
    {
        Ok(())
    } else {
        Err(invalid())
    }
}

impl TileLayout {
    fn validate(self) -> Result<(), WorldError> {
        let finite = [self.cell_size, self.cell_gap, self.tile_anchor]
            .iter()
            .all(|v| v.is_finite());
        if !finite
            || self.cell_size.min_element() <= 0.
            || (self.cell_size + self.cell_gap).min_element() <= 0.
        {
            return Err(invalid());
        }
        Ok(())
    }
}

impl Tile {
    fn validate(&self) -> Result<(), WorldError> {
        color(self.color)?;
        if let Some(sprite) = self.sprite {
            sprite.validate()?;
        }
        if !self.transform.is_finite() || self.transform.row(3) != glam::Vec4::W {
            return Err(invalid());
        }
        if let TileCollider::Sprite(paths) = &self.collider {
            if paths.len() > MAX_SHAPE_PATHS {
                return Err(PhysicsError::InvalidShape.into());
            }
            for path in paths {
                Shape2d::Polygon(path.clone()).validate()?;
            }
        }
        Ok(())
    }
}

impl Tilemap {
    pub(crate) fn validate(&self) -> Result<(), WorldError> {
        color(self.color)?;
        self.layout.validate()?;
        if self.tiles.len() > MAX_TILES || self.cells.len() > MAX_CELLS {
            return Err(invalid());
        }
        for tile in &self.tiles {
            tile.validate()?;
        }
        let mut positions = std::collections::BTreeSet::new();
        for cell in &self.cells {
            let in_palette = usize::try_from(cell.tile).is_ok_and(|i| i < self.tiles.len());
            let exact = cell.position.iter().all(|&p| i16::try_from(p).is_ok());
            if !in_palette || !exact || !positions.insert(cell.position) {
                return Err(invalid());
            }
        }
        Ok(())
    }

    pub(crate) fn register_sprites(&mut self) -> Result<(), WorldError> {
        for tile in &mut self.tiles {
            tile.sprite = tile.sprite.map(Sprite::register).transpose()?;
        }
        Ok(())
    }

    /// The palette entry a validated cell uses.
    pub fn tile(&self, cell: TileCell) -> &Tile {
        &self.tiles[cell.tile as usize]
    }

    /// The bottom-left corner of a cell in tilemap-local units.
    pub fn cell_origin(&self, position: [i32; 2]) -> Vec2 {
        // Validated positions fit i16, so the conversion to f32 is exact.
        let cell = position.map(|p| f32::from(i16::try_from(p).unwrap_or_default()));
        Vec2::from(cell) * (self.layout.cell_size + self.layout.cell_gap)
    }

    /// Where a cell's tile is drawn: the anchored point of the cell, then the tile's transform.
    pub fn tile_matrix(&self, cell: TileCell) -> Mat4 {
        let anchored =
            self.cell_origin(cell.position) + self.layout.tile_anchor * self.layout.cell_size;
        Mat4::from_translation(anchored.extend(0.)) * self.tile(cell).transform
    }

    /// The collider shapes Unity's `TilemapCollider2D` builds from these cells, in
    /// tilemap-local units.
    pub fn collision_shapes(&self) -> Result<Vec<Shape2d>, PhysicsError> {
        let mut shapes = Vec::new();
        for &cell in &self.cells {
            match &self.tile(cell).collider {
                TileCollider::None => {}
                TileCollider::Grid => {
                    let origin = self.cell_origin(cell.position);
                    let size = self.layout.cell_size;
                    shapes.push(Shape2d::Polygon(vec![
                        origin,
                        origin + Vec2::new(size.x, 0.),
                        origin + size,
                        origin + Vec2::new(0., size.y),
                    ]));
                }
                TileCollider::Sprite(paths) => {
                    let matrix = self.tile_matrix(cell);
                    for path in paths {
                        let points = path
                            .iter()
                            .map(|p| matrix.transform_point3(Vec3::new(p.x, p.y, 0.)).truncate())
                            .collect();
                        shapes.push(Shape2d::Polygon(points));
                    }
                }
            }
        }
        for shape in &shapes {
            shape.validate()?;
        }
        Ok(shapes)
    }
}
