//! Tessellates canonical Z-up terrain using the shared mesh upload layout.

pub mod pages;
use glam::{DVec3, Vec3};
use thiserror::Error;
use worldrunner_render_plan::{SceneRotation, SceneTerrain, SceneTransform, TerrainLodPolicy};

use crate::geometry::{VERTEX_BYTES, millimeters_to_meters, push_f32, push_vec3};

/// Fixed world-space tiling extent used by the terrain material shader.
pub const TERRAIN_METERS_PER_TILE: f32 = 10.0;

/// Millionths of scale that leave a placement's geometry at its authored size.
const UNIT_SCALE_MILLIONTHS: u64 = 1_000_000;

/// Indices one quad contributes: two triangles of three corners each.
const INDICES_PER_QUAD: usize = 6;

/// Corners of one quad in the order the two triangles wind through them.
///
/// The corners are collected as (west, south), (east, south), (east, north),
/// (west, north), so this order traverses them counter-clockwise seen from
/// above — which is the front face the pipelines declare with
/// [`wgpu::FrontFace::Ccw`]. Wound the other way, every terrain triangle would
/// be back-facing and the single-sided pipeline would cull the entire world.
const QUAD_TRIANGLE_CORNERS: [usize; INDICES_PER_QUAD] = [0, 1, 2, 0, 2, 3];

/// The placement one terrain mesh is drawn with.
///
/// Terrain vertices are built directly in scene space, origin offset included,
/// so the instance that places them must not move, turn or resize them. It
/// exists at all because the instanced vertex buffer is not optional: the
/// pipelines read a model matrix per instance, and terrain supplies the
/// identity rather than earning a second vertex layout.
pub const TERRAIN_PLACEMENT: SceneTransform = SceneTransform {
    position_millimeters: [0, 0, 0],
    rotation: SceneRotation::IDENTITY,
    scale_millionths: UNIT_SCALE_MILLIONTHS,
};

/// One governed terrain could not be turned into drawable meshes.
#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum TerrainError {
    /// The reduced grid needs more vertices or indices than a draw can address.
    #[error(
        "the reduced terrain grid of {columns}x{rows} samples needs more vertices or indices than a u32 draw can address"
    )]
    GridTooLarge {
        /// Selected sample columns after level-of-detail reduction.
        columns: usize,
        /// Selected sample rows after level-of-detail reduction.
        rows: usize,
    },
    /// A sample position left the signed millimetre range the domain uses.
    #[error("terrain sample positions leave the signed millimeter range")]
    ExtentOverflow,
}

/// The whole terrain surface, packed as one mesh.
///
/// The buffers are byte-for-byte what an authored mesh LOD produces, so the
/// draw path uploads them with the same calls and no special case.
///
/// There is no material handle here, and its absence is the point. The surface
/// is not split by material any more, so there is no single material to name:
/// the shader reads the type grid per fragment and blends whichever surfaces
/// meet there. What used to be carried on this struct now lives in the terrain
/// uniform's surface table, where it can vary within a triangle.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TerrainMesh {
    /// Packed vertices: position, normal, texture coordinate, fixed billboard mode and neutral AO.
    pub vertices: Vec<u8>,
    /// Packed little-endian `u32` indices addressing [`Self::vertices`].
    pub indices: Vec<u8>,
    /// Index count, as the draw range a draw call takes.
    pub index_count: u32,
}

/// Tessellates one governed heightfield into a single mesh.
///
/// Every selected sample becomes exactly one vertex, in row-major order, and a
/// quad indexes the four it is bounded by. That is only possible because the
/// surface is no longer split: while it was, a grid vertex on a boundary had to
/// be duplicated into every material mesh that reached it, and each of those
/// meshes needed a map from grid coordinate to its own local index to avoid
/// duplicating it further. One mesh needs neither. The index of a sample is its
/// position in the grid, computed rather than looked up, and each vertex is
/// built once.
///
/// # Errors
///
/// Returns [`TerrainError`] when the reduced grid outgrows the `u32` a draw
/// indexes with, or when sample positions leave the signed millimetre range.
pub fn build_terrain_mesh(terrain: &SceneTerrain) -> Result<TerrainMesh, TerrainError> {
    let grid = TerrainGrid::new(terrain)?;
    let (columns, rows) = (grid.columns.len(), grid.rows.len());
    require_addressable(columns, rows)?;

    let mut vertices = Vec::with_capacity(columns * rows * VERTEX_BYTES);
    for row in 0..rows {
        for column in 0..columns {
            grid.push_vertex(column, row, &mut vertices);
        }
    }

    let quads = (columns - 1) * (rows - 1);
    let mut indices = Vec::with_capacity(quads * INDICES_PER_QUAD * size_of::<u32>());
    for row in 0..rows - 1 {
        for column in 0..columns - 1 {
            let corners = [
                row * columns + column,
                row * columns + column + 1,
                (row + 1) * columns + column + 1,
                (row + 1) * columns + column,
            ];
            for corner in QUAD_TRIANGLE_CORNERS {
                let index = u32::try_from(corners[corner])
                    .expect("the grid was proven addressable up front");
                indices.extend_from_slice(&index.to_le_bytes());
            }
        }
    }

    Ok(TerrainMesh {
        vertices,
        indices,
        index_count: u32::try_from(quads * INDICES_PER_QUAD)
            .expect("the grid was proven addressable up front"),
    })
}

/// Rejects a reduced grid a `u32` draw could not address.
///
/// The check runs once, before any vertex is built, so every later narrowing
/// of a vertex or index count is provably exact rather than hopeful.
fn require_addressable(columns: usize, rows: usize) -> Result<(), TerrainError> {
    let too_large = TerrainError::GridTooLarge { columns, rows };
    let vertices = columns.checked_mul(rows).ok_or(too_large)?;
    u32::try_from(vertices).map_err(|_| too_large)?;
    let quads = (columns - 1).checked_mul(rows - 1).ok_or(too_large)?;
    let indices = quads.checked_mul(INDICES_PER_QUAD).ok_or(too_large)?;
    u32::try_from(indices).map_err(|_| too_large)?;
    Ok(())
}

fn lod_stride(policy: TerrainLodPolicy) -> u32 {
    match policy {
        TerrainLodPolicy::FullResolution => 1,
        TerrainLodPolicy::UniformSteps { steps } => {
            1u32.checked_shl(u32::from(steps)).unwrap_or(u32::MAX)
        }
    }
}

/// Selects the sample indices one axis contributes at a given stride.
///
/// The far edge is always included, even when the stride does not divide the
/// extent: dropping it would shrink the terrain by up to a whole stride of
/// spacing and leave a visible seam of nothing at the north and east borders.
/// The final interval is then shorter than the others, which costs nothing
/// because positions come from the sample indices themselves rather than from
/// a multiplied-out stride.
fn axis_samples(extent: u32, stride: u32) -> Vec<u32> {
    let last = extent - 1;
    let mut samples = Vec::new();
    let mut index = 0;
    while index < last {
        samples.push(index);
        index = index.saturating_add(stride);
    }
    samples.push(last);
    samples
}

/// Millimetre positions of the samples one axis selected.
///
/// Returns `None` when a position leaves the signed millimetre range, which
/// the caller reports rather than silently wrapping into a mirrored world.
fn axis_positions(origin: i64, spacing: u32, samples: &[u32]) -> Option<Vec<i64>> {
    samples
        .iter()
        .map(|index| {
            i64::from(*index)
                .checked_mul(i64::from(spacing))?
                .checked_add(origin)
        })
        .collect()
}

/// The pair of neighbours a central difference uses on one axis.
///
/// At either end of the axis the pair collapses onto the sample itself, which
/// turns the central difference into the one-sided difference an edge sample
/// has to settle for. Every axis carries at least two samples, so the two
/// neighbours are never the same index and the difference never divides by
/// zero.
fn neighbours(length: usize, index: usize) -> (usize, usize) {
    (index.saturating_sub(1), (index + 1).min(length - 1))
}

/// One heightfield together with the samples its LOD policy selects.
struct TerrainGrid<'a> {
    terrain: &'a SceneTerrain,
    columns: Vec<u32>,
    rows: Vec<u32>,
    east_millimeters: Vec<i64>,
    north_millimeters: Vec<i64>,
}

impl<'a> TerrainGrid<'a> {
    /// Selects the samples one terrain's policy renders and places them.
    fn new(terrain: &'a SceneTerrain) -> Result<Self, TerrainError> {
        let stride = lod_stride(terrain.lod_policy());
        let spacing = terrain.sample_spacing_millimeters();
        let columns = axis_samples(terrain.columns(), stride);
        let rows = axis_samples(terrain.rows(), stride);
        let east_millimeters = axis_positions(terrain.origin_east_millimeters(), spacing, &columns)
            .ok_or(TerrainError::ExtentOverflow)?;
        let north_millimeters = axis_positions(terrain.origin_north_millimeters(), spacing, &rows)
            .ok_or(TerrainError::ExtentOverflow)?;
        Ok(Self {
            terrain,
            columns,
            rows,
            east_millimeters,
            north_millimeters,
        })
    }

    /// Returns the row-major offset of one original grid coordinate.
    fn offset(&self, column: u32, row: u32) -> usize {
        let index = u64::from(row) * u64::from(self.terrain.columns()) + u64::from(column);
        usize::try_from(index).expect("a validated terrain sample index addresses its own grid")
    }

    /// Returns the height sample at one original grid coordinate.
    fn height(&self, column: u32, row: u32) -> i32 {
        self.terrain.heights_millimeters()[self.offset(column, row)]
    }

    /// Returns the scene-space position of one selected grid coordinate.
    fn position(&self, column: usize, row: usize) -> Vec3 {
        let up = i64::from(self.height(self.columns[column], self.rows[row]));
        millimeters_to_meters([
            self.east_millimeters[column],
            self.north_millimeters[row],
            up,
        ])
    }

    /// Returns the horizontal run in millimetres between two axis samples.
    fn axis_run(&self, samples: &[u32], from: usize, to: usize) -> f64 {
        let steps = samples[to] - samples[from];
        f64::from(steps) * f64::from(self.terrain.sample_spacing_millimeters())
    }

    /// Returns the surface normal at one selected grid coordinate.
    ///
    /// The differences are taken between the *selected* neighbours rather than
    /// the original ones, so the normals describe the surface actually being
    /// drawn. Shading a reduced mesh with full-resolution normals would light
    /// detail that no longer has silhouette to match it.
    fn normal(&self, column: usize, row: usize) -> Vec3 {
        let (west, east) = neighbours(self.columns.len(), column);
        let east_rise = f64::from(self.height(self.columns[east], self.rows[row]))
            - f64::from(self.height(self.columns[west], self.rows[row]));
        let (south, north) = neighbours(self.rows.len(), row);
        let north_rise = f64::from(self.height(self.columns[column], self.rows[north]))
            - f64::from(self.height(self.columns[column], self.rows[south]));
        DVec3::new(
            -east_rise / self.axis_run(&self.columns, west, east),
            -north_rise / self.axis_run(&self.rows, south, north),
            1.0,
        )
        .normalize()
        .as_vec3()
    }

    /// Appends one packed vertex for a selected grid coordinate.
    ///
    /// Texture coordinates are derived from the world position rather than
    /// from the grid index, so adjacent quads continue one another's tiling
    /// and a reduced level tiles identically to the full one. Indexing by grid
    /// coordinate would stretch the pattern by the stride every time the level
    /// of detail changed.
    fn push_vertex(&self, column: usize, row: usize, bytes: &mut Vec<u8>) {
        let position = self.position(column, row);
        push_vec3(bytes, position);
        push_vec3(bytes, self.normal(column, row));
        push_f32(bytes, position.x / TERRAIN_METERS_PER_TILE);
        push_f32(bytes, position.y / TERRAIN_METERS_PER_TILE);
        bytes.extend_from_slice(&0_u32.to_le_bytes());
        push_f32(bytes, 1.0);
    }
}

#[cfg(test)]
mod tests;
