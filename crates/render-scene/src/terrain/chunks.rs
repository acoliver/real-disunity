use super::streaming::{TerrainChunk, elevation};
use super::{TerrainError, TerrainGrid, TerrainMesh, axis_positions, axis_samples};
use crate::geometry::VERTEX_BYTES;
use worldrunner_render_plan::SceneTerrain;

/// Two faces of the two triangles spanning each skirt segment.
const SKIRT_INDICES: usize = 12;

/// Tessellates a stock terrain chunk, including its double-sided perimeter skirts.
///
/// The chunk endpoints are inclusive height samples. The far endpoint on each axis
/// is kept even when it is not a multiple of the stride from the near endpoint.
/// Positions use scene metres and the stock packed vertex layout.
///
/// # Errors
/// Returns [`TerrainError`] for invalid chunk bounds or stride, unaddressable geometry,
/// or sample positions outside the signed millimetre range.
pub fn build(terrain: &SceneTerrain, chunk: TerrainChunk) -> Result<TerrainMesh, TerrainError> {
    validate(terrain, chunk)?;
    let columns = samples(chunk.minimum[0], chunk.maximum[0], chunk.stride);
    let rows = samples(chunk.minimum[1], chunk.maximum[1], chunk.stride);
    let spacing = terrain.sample_spacing_millimeters();
    let grid = TerrainGrid {
        terrain,
        east_millimeters: axis_positions(terrain.origin_east_millimeters(), spacing, &columns)
            .ok_or(TerrainError::ExtentOverflow)?,
        north_millimeters: axis_positions(terrain.origin_north_millimeters(), spacing, &rows)
            .ok_or(TerrainError::ExtentOverflow)?,
        columns,
        rows,
    };
    let mut mesh = super::build_grid_mesh(&grid)?;
    let width = grid.columns.len();
    let height = grid.rows.len();
    let bottom = elevation(terrain)[0] - 1.0;
    let edges = [
        (0..width).collect::<Vec<_>>(),
        (0..height).map(|y| y * width + width - 1).collect(),
        (0..width).rev().map(|x| (height - 1) * width + x).collect(),
        (0..height).rev().map(|y| y * width).collect(),
    ];
    for edge in edges {
        for pair in edge.windows(2) {
            append_skirt(&mut mesh, pair[0], pair[1], bottom);
        }
    }
    mesh.index_count = u32::try_from(mesh.indices.len() / size_of::<u32>()).expect("bounded chunk");
    Ok(mesh)
}
fn validate(terrain: &SceneTerrain, chunk: TerrainChunk) -> Result<(), TerrainError> {
    let dimensions = [terrain.columns(), terrain.rows()];
    let valid_window = (0..2).all(|axis| {
        chunk.minimum[axis] < chunk.maximum[axis] && chunk.maximum[axis] < dimensions[axis]
    });
    if !chunk.stride.is_power_of_two() || !valid_window {
        return Err(TerrainError::InvalidChunk);
    }
    let intervals = |axis: usize| {
        let span = chunk.maximum[axis] - chunk.minimum[axis];
        usize::try_from(span.div_ceil(chunk.stride)).expect("valid terrain interval count")
    };
    let (width, height) = (intervals(0), intervals(1));
    let (columns, rows) = (width + 1, height + 1);
    let too_large = TerrainError::GridTooLarge { columns, rows };
    let edges = width
        .checked_add(height)
        .and_then(|v| v.checked_mul(2))
        .ok_or(too_large)?;
    let vertices = buffer_count(columns, rows, 1, edges, 2, VERTEX_BYTES).ok_or(too_large)?;
    let indices = buffer_count(
        width,
        height,
        super::INDICES_PER_QUAD,
        edges,
        SKIRT_INDICES,
        size_of::<u32>(),
    )
    .ok_or(too_large)?;
    u32::try_from(vertices).map_err(|_| too_large)?;
    u32::try_from(indices).map_err(|_| too_large)?;
    Ok(())
}

fn buffer_count(
    columns: usize,
    rows: usize,
    surface_per_cell: usize,
    edges: usize,
    skirt_per_edge: usize,
    element_bytes: usize,
) -> Option<usize> {
    let surface = columns.checked_mul(rows)?.checked_mul(surface_per_cell)?;
    let count = surface.checked_add(edges.checked_mul(skirt_per_edge)?)?;
    isize::try_from(count.checked_mul(element_bytes)?).ok()?;
    Some(count)
}

fn samples(minimum: u32, maximum: u32, stride: u32) -> Vec<u32> {
    axis_samples(maximum - minimum + 1, stride)
        .into_iter()
        .map(|v| v + minimum)
        .collect()
}
fn append_skirt(mesh: &mut TerrainMesh, a: usize, b: usize, bottom: f32) {
    let base = u32::try_from(mesh.vertices.len() / VERTEX_BYTES).expect("bounded chunk");
    for index in [a, b] {
        let mut vertex = mesh.vertices[index * VERTEX_BYTES..(index + 1) * VERTEX_BYTES].to_vec();
        vertex[8..12].copy_from_slice(&bottom.to_le_bytes());
        mesh.vertices.extend(vertex);
    }
    let a = u32::try_from(a).expect("bounded chunk");
    let b = u32::try_from(b).expect("bounded chunk");
    let indices: [u32; SKIRT_INDICES] =
        [a, base, b, b, base, base + 1, b, base, a, base + 1, base, b];
    for index in indices {
        mesh.indices.extend_from_slice(&index.to_le_bytes());
    }
}
