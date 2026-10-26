//! Tilemaps: grid layout and renderer settings, a palette of tiles, and the cells using them.
//! Every write validates the whole tilemap before changing it, so a rejected edit leaves the
//! previous tiles both drawn and colliding.
use crate::{
    DisunityResult,
    callbacks::mutate,
    physics::{component, create},
    sprites::DisunitySprite,
    store::Session,
};
use glam::{Mat4, Vec2, Vec4};
use worldrunner_world_simulation::{
    ComponentDesc, ComponentHandle,
    tilemap::{Tile, TileCell, TileCollider, TileLayout, Tilemap},
};

/// A tilemap's renderer settings and grid layout: cell size and gap from its `Grid`, and the
/// tilemap's tile anchor as a fraction of a cell.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct DisunityTilemap {
    pub enabled: u32,
    pub sorting_layer: i32,
    pub sorting_order: i32,
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
    pub cell_x: f32,
    pub cell_y: f32,
    pub gap_x: f32,
    pub gap_y: f32,
    pub anchor_x: f32,
    pub anchor_y: f32,
}

/// One palette entry besides its sprite. `collider` is Unity's `Tile.ColliderType`: 0 None,
/// 1 Sprite, 2 Grid. The tile transform is the 2D affine `[m00 m01 tx; m10 m11 ty]`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct DisunityTile {
    pub collider: u32,
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
    pub m00: f32,
    pub m01: f32,
    pub m10: f32,
    pub m11: f32,
    pub tx: f32,
    pub ty: f32,
}

/// A cell at (`x`, `y`) using palette entry `tile`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct DisunityTileCell {
    pub x: i32,
    pub y: i32,
    pub tile: u32,
}

impl DisunityTile {
    fn to_native(self, sprite: Option<&DisunitySprite>) -> Result<Tile, crate::errors::AbiError> {
        let collider = match self.collider {
            0 => TileCollider::None,
            1 => TileCollider::Sprite(Vec::new()),
            2 => TileCollider::Grid,
            _ => return Err("unknown tile collider type".into()),
        };
        Ok(Tile {
            sprite: sprite.map(crate::sprites::content),
            color: [self.r, self.g, self.b, self.a],
            transform: Mat4::from_cols(
                Vec4::new(self.m00, self.m10, 0., 0.),
                Vec4::new(self.m01, self.m11, 0., 0.),
                Vec4::Z,
                Vec4::new(self.tx, self.ty, 0., 1.),
            ),
            collider,
        })
    }
}

fn tilemap(
    session: &Session,
    handle: u64,
) -> Result<(ComponentHandle, Tilemap), crate::errors::AbiError> {
    let id = component(session, handle)?;
    match &session.native.component(id)?.desc {
        ComponentDesc::Tilemap(map) => Ok((id, map.clone())),
        _ => Err("component is not a Tilemap".into()),
    }
}

/// Applies `change` to a copy of the tilemap and publishes it only if the result validates.
fn edit(
    world: u64,
    handle: u64,
    change: impl FnOnce(&mut Tilemap) -> Result<(), crate::errors::AbiError>,
) -> DisunityResult {
    mutate(world, |s| {
        let (id, mut map) = tilemap(s, handle)?;
        change(&mut map)?;
        s.native.set_component(id, ComponentDesc::Tilemap(map))?;
        Ok(())
    })
}

/// Creates an empty enabled tilemap on a unit grid with a centred tile anchor.
/// # Safety
/// Output is writable for one component identity.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_tilemap_create(
    world: u64,
    object: u64,
    output: *mut u64,
) -> DisunityResult {
    if output.is_null() {
        return DisunityResult::invalid_argument();
    }
    match create(world, object, ComponentDesc::Tilemap(Tilemap::default())) {
        Ok(id) => {
            unsafe { *output = id };
            DisunityResult::ok()
        }
        Err(e) => crate::abi::translate_error(Some(world), &e),
    }
}

/// # Safety
/// Value is readable for one tilemap record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_tilemap_write(
    world: u64,
    handle: u64,
    value: *const DisunityTilemap,
) -> DisunityResult {
    if value.is_null() {
        return DisunityResult::invalid_argument();
    }
    let v = unsafe { *value };
    edit(world, handle, |map| {
        if v.enabled > 1 {
            return Err("tilemap enabled must be 0 or 1".into());
        }
        map.enabled = v.enabled == 1;
        map.sorting_layer = v.sorting_layer;
        map.sorting_order = v.sorting_order;
        map.color = [v.r, v.g, v.b, v.a];
        map.layout = TileLayout {
            cell_size: Vec2::new(v.cell_x, v.cell_y),
            cell_gap: Vec2::new(v.gap_x, v.gap_y),
            tile_anchor: Vec2::new(v.anchor_x, v.anchor_y),
        };
        Ok(())
    })
}

/// # Safety
/// Output is writable for one tilemap record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_tilemap_read(
    world: u64,
    handle: u64,
    output: *mut DisunityTilemap,
) -> DisunityResult {
    if output.is_null() {
        return DisunityResult::invalid_argument();
    }
    mutate(world, |s| {
        let (_, map) = tilemap(s, handle)?;
        let [r, g, b, a] = map.color;
        let layout = map.layout;
        unsafe {
            *output = DisunityTilemap {
                enabled: u32::from(map.enabled),
                sorting_layer: map.sorting_layer,
                sorting_order: map.sorting_order,
                r,
                g,
                b,
                a,
                cell_x: layout.cell_size.x,
                cell_y: layout.cell_size.y,
                gap_x: layout.cell_gap.x,
                gap_y: layout.cell_gap.y,
                anchor_x: layout.tile_anchor.x,
                anchor_y: layout.tile_anchor.y,
            };
        }
        Ok(())
    })
}

/// Sets palette entry `index`, replacing it or, at the palette's length, appending it. `sprite`
/// may be null for a tile with no sprite; only its content fields are read. A Sprite collider
/// starts with no physics shape; set it with `disunity_tilemap_set_tile_shape`.
/// # Safety
/// Tile is readable for one tile record; sprite is null or readable for one sprite record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_tilemap_set_tile(
    world: u64,
    handle: u64,
    index: u32,
    tile: *const DisunityTile,
    sprite: *const DisunitySprite,
) -> DisunityResult {
    if tile.is_null() {
        return DisunityResult::invalid_argument();
    }
    let tile = unsafe { *tile };
    let sprite = unsafe { sprite.as_ref() }.copied();
    edit(world, handle, |map| {
        let value = tile.to_native(sprite.as_ref())?;
        let index = usize::try_from(index).map_err(|_| "tile index out of range")?;
        match index.cmp(&map.tiles.len()) {
            std::cmp::Ordering::Less => map.tiles[index] = value,
            std::cmp::Ordering::Equal => map.tiles.push(value),
            std::cmp::Ordering::Greater => return Err("tile index past the palette end".into()),
        }
        Ok(())
    })
}

/// Sets the physics shape of Sprite-collider tile `index`: `path_count` convex paths, path `i`
/// holding `lengths[i]` points, read in order from `points` as x, y pairs in sprite units
/// around the pivot. This is Unity's `Sprite.GetPhysicsShape` for the tile's sprite.
/// # Safety
/// Lengths is readable for path_count values and points for twice their sum; either may be
/// null when path_count is zero.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_tilemap_set_tile_shape(
    world: u64,
    handle: u64,
    index: u32,
    points: *const f32,
    lengths: *const u32,
    path_count: u32,
) -> DisunityResult {
    let Some(paths) = (unsafe { read_paths(points, lengths, path_count) }) else {
        return DisunityResult::invalid_argument();
    };
    edit(world, handle, |map| {
        let tile = usize::try_from(index)
            .ok()
            .and_then(|i| map.tiles.get_mut(i))
            .ok_or("tile index out of range")?;
        let TileCollider::Sprite(shape) = &mut tile.collider else {
            return Err("tile does not use a Sprite collider".into());
        };
        *shape = paths;
        Ok(())
    })
}

/// Copies borrowed path arrays, or `None` if a pointer the counts need is null. Each path is
/// bounded before its points are read; validation then checks the shapes themselves.
unsafe fn read_paths(
    points: *const f32,
    lengths: *const u32,
    count: u32,
) -> Option<Vec<Vec<Vec2>>> {
    const MAX_PATHS: u32 = 16;
    const MAX_POINTS: u32 = 128;
    if count == 0 {
        return Some(Vec::new());
    }
    if count > MAX_PATHS || points.is_null() || lengths.is_null() {
        return None;
    }
    let lengths = unsafe { std::slice::from_raw_parts(lengths, count as usize) };
    if lengths.iter().any(|&n| n > MAX_POINTS) {
        return None;
    }
    let total: usize = lengths.iter().map(|&n| n as usize).sum();
    let flat = unsafe { std::slice::from_raw_parts(points, total * 2) };
    let mut pairs = flat.chunks_exact(2).map(|p| Vec2::new(p[0], p[1]));
    Some(
        lengths
            .iter()
            .map(|&n| pairs.by_ref().take(n as usize).collect())
            .collect(),
    )
}

/// Replaces every cell. Positions must be distinct and each tile index must be in the palette.
/// # Safety
/// Cells is readable for count records, or null when count is zero.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_tilemap_set_cells(
    world: u64,
    handle: u64,
    cells: *const DisunityTileCell,
    count: u32,
) -> DisunityResult {
    if cells.is_null() && count != 0 {
        return DisunityResult::invalid_argument();
    }
    let cells: Vec<TileCell> = if count == 0 {
        Vec::new()
    } else {
        unsafe { std::slice::from_raw_parts(cells, count as usize) }
            .iter()
            .map(|c| TileCell {
                position: [c.x, c.y],
                tile: c.tile,
            })
            .collect()
    };
    edit(world, handle, |map| {
        map.cells = cells;
        Ok(())
    })
}
