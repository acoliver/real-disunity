use crate::components::{PhysicsMaterial, Polygon};
use glam::Vec2;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct TilemapCollider {
    pub cell_size: [f32; 2],
    pub cell_gap: [f32; 2],
    pub tile_anchor: [f32; 2],
    pub offset: [f32; 2],
    pub is_trigger: bool,
    pub material: Option<PhysicsMaterial>,
    pub cells: Vec<Cell>,
    #[serde(skip)]
    pub polygons: Arc<Vec<Vec<[f32; 2]>>>,
}
impl Default for TilemapCollider {
    fn default() -> Self {
        Self {
            cell_size: [1.0; 2],
            cell_gap: [0.0; 2],
            tile_anchor: [0.5; 2],
            offset: [0.0; 2],
            is_trigger: false,
            material: None,
            cells: Vec::new(),
            polygons: Arc::default(),
        }
    }
}
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ColliderType {
    None,
    #[default]
    Grid,
    Sprite,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct Cell {
    pub position: [i32; 2],
    pub collider_type: ColliderType,
    pub matrix: [f32; 6],
    pub sprite_size: Option<[f32; 2]>,
    pub sprite_pivot: [f32; 2],
    pub physics_shapes: Vec<Vec<[f32; 2]>>,
}
impl Default for Cell {
    fn default() -> Self {
        Self {
            position: [0; 2],
            collider_type: ColliderType::Grid,
            matrix: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
            sprite_size: None,
            sprite_pivot: [0.5; 2],
            physics_shapes: Vec::new(),
        }
    }
}
impl TilemapCollider {
    pub fn parse(value: Value) -> Result<Self, String> {
        let mut collider: Self = serde_json::from_value(value).map_err(|e| e.to_string())?;
        collider.validate()?;
        let mut polygons = Vec::new();
        for cell in &collider.cells {
            for polygon in collider.cell_polygons(cell)? {
                if polygons.len() >= 4096 {
                    return Err("tile collider shape budget exceeded".into());
                }
                polygon.validate()?;
                polygons.push(polygon.points);
            }
        }
        collider.polygons = Arc::new(polygons);
        Ok(collider)
    }
    fn validate(&self) -> Result<(), String> {
        if self.cells.len() > 4096
            || self.cell_size.iter().any(|v| !v.is_finite() || *v <= 0.0)
            || self.cell_gap.iter().any(|v| !v.is_finite())
            || self
                .tile_anchor
                .iter()
                .chain(&self.offset)
                .any(|v| !v.is_finite())
            || self
                .cell_size
                .iter()
                .zip(self.cell_gap)
                .any(|(s, g)| *s + g <= 0.0)
        {
            return Err("invalid tile grid metrics or cell budget exceeded".into());
        }
        Ok(())
    }
    fn cell_polygons(&self, cell: &Cell) -> Result<Vec<Polygon>, String> {
        cell.validate()?;
        let points = match cell.collider_type {
            ColliderType::None => return Ok(Vec::new()),
            ColliderType::Grid => vec![rectangle(self.cell_size, self.tile_anchor)],
            ColliderType::Sprite if cell.physics_shapes.is_empty() => {
                vec![rectangle(
                    cell.sprite_size
                        .ok_or("sprite tile requires sprite_size or physics_shapes")?,
                    cell.sprite_pivot,
                )]
            }
            ColliderType::Sprite => cell.physics_shapes.clone(),
        };
        if points.len() > 4096 {
            return Err("tile collider shape budget exceeded".into());
        }
        let size = Vec2::from(self.cell_size);
        let origin = Vec2::new(cell.position[0] as f32, cell.position[1] as f32)
            * (size + Vec2::from(self.cell_gap))
            + size * Vec2::from(self.tile_anchor)
            + Vec2::from(self.offset);
        let [xx, yx, xy, yy, tx, ty] = cell.matrix;
        Ok(points
            .into_iter()
            .map(|points| Polygon {
                points: points
                    .into_iter()
                    .map(|p| {
                        (origin + Vec2::new(xx * p[0] + xy * p[1] + tx, yx * p[0] + yy * p[1] + ty))
                            .to_array()
                    })
                    .collect(),
                offset: [0.0; 2],
                is_trigger: self.is_trigger,
                material: None,
            })
            .collect())
    }
}
impl Cell {
    fn validate(&self) -> Result<(), String> {
        let [xx, yx, xy, yy, _, _] = self.matrix;
        if self.matrix.iter().any(|v| !v.is_finite())
            || (xx * yy - xy * yx).abs() < 1e-8
            || self.sprite_pivot.iter().any(|v| !(0.0..=1.0).contains(v))
            || self
                .sprite_size
                .is_some_and(|s| s.iter().any(|v| !v.is_finite() || *v <= 0.0))
            || self
                .physics_shapes
                .iter()
                .any(|p| p.len() < 3 || p.len() > 128)
        {
            return Err("invalid tile matrix, sprite bounds or physics shape".into());
        }
        Ok(())
    }
}
fn rectangle(size: [f32; 2], pivot: [f32; 2]) -> Vec<[f32; 2]> {
    let size = Vec2::from(size);
    let min = -Vec2::from(pivot) * size;
    let max = min + size;
    vec![
        [min.x, min.y],
        [max.x, min.y],
        [max.x, max.y],
        [min.x, max.y],
    ]
}
