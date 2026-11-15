use super::streaming::{TerrainChunk, elevation, points};
use glam::Vec3;
use worldrunner_render_plan::SceneTerrain;

/// Exact inclusive height-grid ranges for a streaming terrain's quadtree.
/// Build once for the terrain and policy, then reuse for selection and camera culling.
#[derive(Debug)]
pub struct TerrainHeightBounds {
    nodes: Box<[(TerrainChunk, [i32; 2])]>,
}

impl TerrainHeightBounds {
    pub fn new(terrain: &SceneTerrain) -> Self {
        let root = TerrainChunk::root(
            terrain,
            terrain.streaming_policy().expect("streaming policy"),
        );
        let mut nodes = Vec::new();
        build(terrain, root, &mut nodes);
        nodes.sort_unstable_by_key(|(node, _)| *node);
        Self {
            nodes: nodes.into_boxed_slice(),
        }
    }

    /// Includes every source sample in the window, including shared edges.
    pub fn height_range_millimeters(&self, node: TerrainChunk) -> [i32; 2] {
        let index = self
            .nodes
            .binary_search_by_key(&node, |(key, _)| *key)
            .expect("chunk belongs to the terrain quadtree");
        self.nodes[index].1
    }

    pub(super) fn elevation(&self, node: TerrainChunk) -> [f32; 2] {
        self.height_range_millimeters(node)
            .map(|height| crate::geometry::millimeter_to_meter(i64::from(height)))
    }

    pub(crate) fn chunk_bounds(&self, terrain: &SceneTerrain, node: TerrainChunk) -> [Vec3; 2] {
        let corners = points(terrain, node, self.elevation(node));
        let mut minimum = corners.iter().copied().fold(Vec3::INFINITY, Vec3::min);
        let maximum = corners.iter().copied().fold(Vec3::NEG_INFINITY, Vec3::max);
        // The mesh builder places all skirts below the terrain-wide minimum.
        minimum.z = elevation(terrain)[0] - 1.0;
        [minimum, maximum]
    }
}

fn union(a: [i32; 2], b: [i32; 2]) -> [i32; 2] {
    [a[0].min(b[0]), a[1].max(b[1])]
}

fn build(
    terrain: &SceneTerrain,
    node: TerrainChunk,
    nodes: &mut Vec<(TerrainChunk, [i32; 2])>,
) -> [i32; 2] {
    let mut range = [i32::MAX, i32::MIN];
    if let Some(children) = node.children() {
        for child in children {
            range = union(range, build(terrain, child, nodes));
        }
    } else {
        let columns = terrain.columns() as usize;
        for row in node.minimum[1]..=node.maximum[1] {
            let start = row as usize * columns + node.minimum[0] as usize;
            let end = row as usize * columns + node.maximum[0] as usize;
            for &height in &terrain.heights_millimeters()[start..=end] {
                range = union(range, [height; 2]);
            }
        }
    }
    nodes.push((node, range));
    range
}
