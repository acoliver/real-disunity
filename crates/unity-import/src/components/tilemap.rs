use super::{ComponentError, GRID, TILEMAP};
use crate::scene::{
    ObjectId, Reference, Scene, Value,
    read::{IDENTITY, MATRIX, RGBA, XYZW, integer, members, vector},
};

/// Unity's "no entry" index in a tile's lookup arrays.
const NO_INDEX: i64 = 65_535;

#[derive(Clone, Debug, PartialEq)]
pub struct Grid {
    pub cell_size: [f64; 3],
    pub cell_gap: [f64; 3],
}

#[derive(Clone, Debug, PartialEq)]
pub struct Tile {
    /// Cell position in grid coordinates.
    pub cell: [i64; 3],
    /// The tile asset, usually an object loaded from a `.asset` file.
    pub asset: Reference,
    /// The sprite drawn in the cell, `Reference::Null` for none.
    pub sprite: Reference,
    /// Row-major `Matrix4x4` applied to the sprite inside the cell.
    pub transform: [f64; 16],
    /// Linear RGBA tint.
    pub color: [f64; 4],
    pub flags: i64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Tilemap {
    pub tiles: Vec<Tile>,
    /// Sprite anchor inside each cell, as a fraction of the cell size.
    pub tile_anchor: [f64; 3],
    pub color: [f64; 4],
}

/// The arrays a tile's indices point into.
struct Lookups<'a> {
    assets: &'a [Value],
    sprites: &'a [Value],
    matrices: &'a [Value],
    colors: &'a [Value],
}

impl Scene {
    pub fn grid(&self, id: ObjectId) -> Result<Grid, ComponentError> {
        let fields = self.expect_class(id, GRID, "Grid")?;
        let error = |message: &str| self.component_error(id, message);
        if integer(fields, "m_CellLayout", 0) != Some(0) {
            return Err(error("only the rectangle cell layout is supported"));
        }
        if integer(fields, "m_CellSwizzle", 0) != Some(0) {
            return Err(error("only the XYZ cell swizzle is supported"));
        }
        Ok(Grid {
            cell_size: vector(fields, "m_CellSize", XYZW, [1.0, 1.0, 0.0])
                .ok_or_else(|| error("invalid m_CellSize"))?,
            cell_gap: vector(fields, "m_CellGap", XYZW, [0.0; 3])
                .ok_or_else(|| error("invalid m_CellGap"))?,
        })
    }

    pub fn tilemap(&self, id: ObjectId) -> Result<Tilemap, ComponentError> {
        let fields = self.expect_class(id, TILEMAP, "Tilemap")?;
        let error = |message: String| self.component_error(id, message);
        check_supported(fields).map_err(|message| error(message.into()))?;
        let lookups = Lookups {
            assets: array(fields, "m_TileAssetArray"),
            sprites: array(fields, "m_TileSpriteArray"),
            matrices: array(fields, "m_TileMatrixArray"),
            colors: array(fields, "m_TileColorArray"),
        };
        let tiles = array(fields, "m_Tiles")
            .iter()
            .map(|entry| tile(entry, &lookups))
            .collect::<Result<_, _>>()
            .map_err(error)?;
        Ok(Tilemap {
            tiles,
            tile_anchor: vector(fields, "m_TileAnchor", XYZW, [0.5, 0.5, 0.0])
                .ok_or_else(|| error("invalid m_TileAnchor".into()))?,
            color: vector(fields, "m_Color", RGBA, [1.0; 4])
                .ok_or_else(|| error("invalid m_Color".into()))?,
        })
    }
}

fn check_supported(fields: &Value) -> Result<(), &'static str> {
    if fields
        .get("m_AnimatedTiles")
        .and_then(Value::as_record)
        .is_some_and(|tiles| !tiles.is_empty())
    {
        return Err("animated tiles are not supported");
    }
    if integer(fields, "m_TileOrientation", 0) != Some(0) {
        return Err("only the XY tile orientation is supported");
    }
    Ok(())
}

fn array<'a>(fields: &'a Value, key: &str) -> &'a [Value] {
    fields
        .get(key)
        .and_then(Value::as_sequence)
        .unwrap_or_default()
}

fn tile(entry: &Value, lookups: &Lookups<'_>) -> Result<Tile, String> {
    let cell = cell(entry).ok_or("tile has an invalid cell position")?;
    tile_data(entry, cell, lookups).map_err(|message| format!("tile at {cell:?}: {message}"))
}

fn cell(entry: &Value) -> Option<[i64; 3]> {
    let first = entry.get("first")?;
    let [x, y, z] = ["x", "y", "z"].map(|axis| integer(first, axis, 0));
    Some([x?, y?, z?])
}

fn tile_data(entry: &Value, cell: [i64; 3], lookups: &Lookups<'_>) -> Result<Tile, String> {
    let data = entry.get("second").ok_or("tile has no data")?;
    let index = |name: &str| integer(data, name, NO_INDEX).ok_or(format!("invalid {name}"));
    if index("m_TileObjectToInstantiateIndex")? != NO_INDEX {
        return Err("tiles that instantiate GameObjects are not supported".into());
    }
    let reference = |values, name| -> Result<Reference, String> {
        lookup(values, index(name)?)?.map_or(Ok(Reference::Null), |value| {
            value
                .as_reference()
                .ok_or(format!("{name} entry is not a reference"))
        })
    };
    let matrix = lookup(lookups.matrices, index("m_TileMatrixIndex")?)?;
    let color = lookup(lookups.colors, index("m_TileColorIndex")?)?;
    Ok(Tile {
        cell,
        asset: reference(lookups.assets, "m_TileIndex")?,
        sprite: reference(lookups.sprites, "m_TileSpriteIndex")?,
        transform: matrix
            .map_or(Some(IDENTITY), |m| members(m, MATRIX, IDENTITY))
            .ok_or("invalid tile matrix")?,
        color: color
            .map_or(Some([1.0; 4]), |c| members(c, RGBA, [1.0; 4]))
            .ok_or("invalid tile color")?,
        flags: integer(data, "m_AllTileFlags", 0).ok_or("invalid tile flags")?,
    })
}

/// The `m_Data` of entry `index`, or `None` for Unity's no-entry index.
fn lookup(values: &[Value], index: i64) -> Result<Option<&Value>, String> {
    if index == NO_INDEX {
        return Ok(None);
    }
    let entry = usize::try_from(index)
        .ok()
        .and_then(|index| values.get(index))
        .ok_or(format!("index {index} is outside its array"))?;
    entry
        .get("m_Data")
        .map(Some)
        .ok_or("array entry has no m_Data".into())
}
