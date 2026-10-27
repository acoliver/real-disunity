use super::streaming::{TerrainChunk, elevation};
use super::{TerrainError, TerrainGrid, TerrainMesh, axis_positions, axis_samples};
use crate::geometry::VERTEX_BYTES;
use worldrunner_render_plan::SceneTerrain;

pub(crate) fn build(
    terrain: &SceneTerrain,
    chunk: TerrainChunk,
) -> Result<TerrainMesh, TerrainError> {
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
    mesh.index_count = u32::try_from(mesh.indices.len() / 4).expect("bounded chunk");
    Ok(mesh)
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
    for index in [a, base, b, b, base, base + 1, b, base, a, base + 1, base, b] {
        mesh.indices.extend_from_slice(&index.to_le_bytes());
    }
}
