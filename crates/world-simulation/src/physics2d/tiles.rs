use super::{PhysicsError, Shape2d};
use glam::Vec2;

#[derive(Clone, Debug, PartialEq)]
pub enum TileShape {
    Empty,
    Grid,
    Convex(Vec<Vec2>),
}
#[derive(Clone, Debug, PartialEq)]
pub struct TileCell {
    pub position: [i32; 2],
    pub shape: TileShape,
}
#[derive(Clone, Debug, PartialEq)]
pub struct TileGrid {
    pub cell_size: Vec2,
    pub gap: Vec2,
    pub offset: Vec2,
    pub cells: Vec<TileCell>,
}
impl Default for TileGrid {
    fn default() -> Self {
        Self {
            cell_size: Vec2::ONE,
            gap: Vec2::ZERO,
            offset: Vec2::ZERO,
            cells: Vec::new(),
        }
    }
}
impl TileGrid {
    /// Explicit convex vertices are cell-local metres. Empty shapes stay empty.
    pub fn shapes(&self) -> Result<Vec<Shape2d>, PhysicsError> {
        if !self.cell_size.is_finite()
            || self.cell_size.min_element() <= 0.0
            || !self.gap.is_finite()
            || (self.cell_size + self.gap).min_element() <= 0.0
            || !self.offset.is_finite()
            || self.cells.len() > 4096
        {
            return Err(PhysicsError::InvalidShape);
        }
        let mut shapes = Vec::new();
        for cell in &self.cells {
            let origin = Vec2::new(
                f32::from(i16::try_from(cell.position[0]).map_err(|_| PhysicsError::InvalidShape)?),
                f32::from(i16::try_from(cell.position[1]).map_err(|_| PhysicsError::InvalidShape)?),
            ) * (self.cell_size + self.gap)
                + self.offset;
            let points = match &cell.shape {
                TileShape::Empty => continue,
                TileShape::Grid => vec![
                    Vec2::ZERO,
                    Vec2::new(self.cell_size.x, 0.0),
                    self.cell_size,
                    Vec2::new(0.0, self.cell_size.y),
                ],
                TileShape::Convex(p) => p.clone(),
            };
            let shape = Shape2d::Polygon(points.into_iter().map(|p| p + origin).collect());
            shape.validate()?;
            shapes.push(shape);
        }
        Ok(shapes)
    }
}
