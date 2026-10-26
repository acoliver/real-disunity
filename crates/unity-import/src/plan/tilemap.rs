//! A tilemap with its grid, renderer, palette and cells.

use serde::Serialize;

use crate::{
    ObjectId,
    components::{ComponentError, RendererSettings, Tile},
    package::{PackageAsset, PackageScript},
    scene::{
        Reference, Value,
        read::{IDENTITY, flag, integer, object_id},
    },
};

use super::{PlanError, objects::Planner, values::sprite_index};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum TileCollider {
    None,
    Sprite,
    Grid,
}

/// A palette entry. Cells that share a tile asset, sprite, color and offset share an entry.
#[derive(Debug, PartialEq, Serialize)]
pub struct PlanTile {
    pub sprite: Option<usize>,
    pub color: [f64; 4],
    pub collider: TileCollider,
    /// The cell matrix's translation; tiles that rotate or scale are refused.
    pub offset: [f64; 2],
}

#[derive(Debug, Serialize)]
pub struct PlanTilemap {
    pub enabled: bool,
    /// From the parent object's `Grid`.
    pub cell_size: [f64; 2],
    pub cell_gap: [f64; 2],
    pub tile_anchor: [f64; 2],
    pub color: [f64; 4],
    /// From the `TilemapRenderer` on the same object.
    pub renderer: RendererSettings,
    pub palette: Vec<PlanTile>,
    /// Each cell's position and palette index.
    pub cells: Vec<([i64; 2], usize)>,
}

impl Planner<'_> {
    pub(super) fn tilemap(&mut self, id: ObjectId) -> Result<PlanTilemap, PlanError> {
        let scene = self.scene;
        let error = |message: &str| scene.component_error(id, message);
        let view = scene.tilemap(id)?;
        let fields = &scene
            .object(id)
            .ok_or_else(|| error("is not an object"))?
            .fields;
        let enabled = flag(fields, "m_Enabled", true).ok_or_else(|| error("invalid m_Enabled"))?;
        let grid = scene.grid(self.sibling(id, "Grid", true)?)?;
        let renderer = scene
            .tilemap_renderer(self.sibling(id, "TilemapRenderer", false)?)?
            .renderer;
        let mut palette = Vec::new();
        let mut cells = Vec::new();
        for tile in &view.tiles {
            let entry = self.palette_tile(id, tile)?;
            let index = palette
                .iter()
                .position(|known| *known == entry)
                .unwrap_or_else(|| {
                    palette.push(entry);
                    palette.len() - 1
                });
            cells.push(([tile.cell[0], tile.cell[1]], index));
        }
        Ok(PlanTilemap {
            enabled,
            cell_size: [grid.cell_size[0], grid.cell_size[1]],
            cell_gap: [grid.cell_gap[0], grid.cell_gap[1]],
            tile_anchor: [view.tile_anchor[0], view.tile_anchor[1]],
            color: view.color,
            renderer,
            palette,
            cells,
        })
    }

    /// The single `type_name` component on the tilemap's object, or with `parent` on its
    /// parent's object.
    fn sibling(
        &self,
        id: ObjectId,
        type_name: &str,
        parent: bool,
    ) -> Result<ObjectId, ComponentError> {
        let scene = self.scene;
        let missing = || {
            let place = if parent { "its parent" } else { "its object" };
            scene.component_error(id, format!("needs a {type_name} on {place}"))
        };
        let owner = scene
            .object(id)
            .and_then(|object| object.fields.get("m_GameObject"))
            .and_then(object_id)
            .ok_or_else(missing)?;
        let owner = if parent {
            let transform = scene.transform_of(owner).ok_or_else(missing)?;
            let parent = scene
                .transform(transform)
                .and_then(|t| t.parent)
                .ok_or_else(missing)?;
            scene
                .transform(parent)
                .and_then(|t| t.game_object)
                .ok_or_else(missing)?
        } else {
            owner
        };
        let components = scene.game_object(owner).ok_or_else(missing)?.components;
        let mut found = components
            .into_iter()
            .filter(|c| scene.object(*c).is_some_and(|o| o.type_name == type_name));
        match (found.next(), found.next()) {
            (Some(component), None) => Ok(component),
            _ => Err(missing()),
        }
    }

    fn palette_tile(&mut self, id: ObjectId, tile: &Tile) -> Result<PlanTile, PlanError> {
        let scene = self.scene;
        let error = |message: String| {
            scene.component_error(id, format!("tile at {:?}: {message}", tile.cell))
        };
        if tile.cell[2] != 0 {
            return Err(error("cells off the z = 0 layer are not supported".into()).into());
        }
        let mut rest = tile.transform;
        let offset = [rest[3], rest[7]];
        (rest[3], rest[7]) = (IDENTITY[3], IDENTITY[7]);
        if rest.map(f64::to_bits) != IDENTITY.map(f64::to_bits) {
            return Err(error("rotated or scaled tiles are not supported".into()).into());
        }
        let collider = self.tile_collider(tile).map_err(error)?;
        let sprite = sprite_index(self, id, &tile.sprite)?;
        if collider == TileCollider::Sprite
            && let Reference::External { guid, file_id } = tile.sprite
        {
            self.require_shape(guid, file_id)
                .map_err(|e| error(e.to_string()))?;
        }
        Ok(PlanTile {
            sprite,
            color: tile.color,
            collider,
            offset,
        })
    }

    /// The collider type of the cell's built-in `Tile` asset.
    fn tile_collider(&self, tile: &Tile) -> Result<TileCollider, String> {
        let scene = self.scene;
        let Reference::Object(asset) = tile.asset else {
            return Err(format!("tile asset {:?} is not a Tile", tile.asset));
        };
        if scene.script(asset)
            != Some(Reference::Package(PackageAsset::Script(
                PackageScript::Tile,
            )))
        {
            return Err("only Unity's built-in Tile is supported".into());
        }
        let fields = &scene.object(asset).ok_or("tile asset is missing")?.fields;
        if fields
            .get("m_InstancedGameObject")
            .and_then(Value::as_reference)
            .is_some_and(|r| r != Reference::Null)
        {
            return Err("tiles that instantiate GameObjects are not supported".into());
        }
        match integer(fields, "m_ColliderType", 1) {
            Some(0) => Ok(TileCollider::None),
            Some(1) => Ok(TileCollider::Sprite),
            Some(2) => Ok(TileCollider::Grid),
            _ => Err("invalid m_ColliderType".into()),
        }
    }
}
