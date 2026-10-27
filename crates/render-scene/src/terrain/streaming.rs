//! Camera-dependent quadtree selection. Leaves always cover the entire grid.
use glam::{Mat4, Vec3};
use worldrunner_render_plan::{SceneTerrain, TerrainStreamingPolicy};

/// A rectangular sample window, with inclusive endpoints and a power-of-two stride.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct TerrainChunk {
    pub minimum: [u32; 2],
    pub maximum: [u32; 2],
    pub stride: u32,
}
impl TerrainChunk {
    pub fn root(terrain: &SceneTerrain, policy: TerrainStreamingPolicy) -> Self {
        let maximum = [terrain.columns() - 1, terrain.rows() - 1];
        let cells = maximum[0].max(maximum[1]);
        let stride = cells.div_ceil(policy.chunk_intervals()).next_power_of_two();
        Self {
            minimum: [0; 2],
            maximum,
            stride,
        }
    }
    pub fn children(self) -> Option<Vec<Self>> {
        if self.stride == 1 {
            return None;
        }
        let middle = std::array::from_fn::<_, 2, _>(|axis| {
            self.minimum[axis] + (self.maximum[axis] - self.minimum[axis]) / 2
        });
        let xs = if middle[0] > self.minimum[0] {
            vec![(self.minimum[0], middle[0]), (middle[0], self.maximum[0])]
        } else {
            vec![(self.minimum[0], self.maximum[0])]
        };
        let ys = if middle[1] > self.minimum[1] {
            vec![(self.minimum[1], middle[1]), (middle[1], self.maximum[1])]
        } else {
            vec![(self.minimum[1], self.maximum[1])]
        };
        if xs.len() * ys.len() == 1 {
            return None;
        }
        Some(
            ys.into_iter()
                .flat_map(|(south, north)| {
                    xs.iter().map(move |&(west, east)| Self {
                        minimum: [west, south],
                        maximum: [east, north],
                        stride: self.stride / 2,
                    })
                })
                .collect(),
        )
    }
}

/// Selected next leaf set and complete chunk uploads. Removed leaves are evicted.
#[derive(Debug)]
pub struct TerrainSelection {
    pub leaves: Vec<TerrainChunk>,
    pub uploads: Vec<TerrainChunk>,
}

/// Refines by projected cell footprint. Cached splits merge below 1/1.2 of the
/// threshold. Every replacement is atomic, so budget pressure retains coverage.
/// Resident limits include old leaves and replacement uploads during preparation.
pub fn select(
    terrain: &SceneTerrain,
    current: &[TerrainChunk],
    matrix: Mat4,
    viewport: [u32; 2],
) -> TerrainSelection {
    select_with_retired(terrain, current, matrix, viewport, 0)
}

pub(crate) fn select_with_retired(
    terrain: &SceneTerrain,
    current: &[TerrainChunk],
    matrix: Mat4,
    viewport: [u32; 2],
    retired: usize,
) -> TerrainSelection {
    let policy = terrain.streaming_policy().expect("streaming policy");
    let mut leaves = current.to_vec();
    let mut uploads = Vec::new();
    let root = TerrainChunk::root(terrain, policy);
    merge(terrain, root, matrix, viewport, &mut leaves, &mut uploads);
    if current.len() + retired + uploads.len() > policy.maximum_resident_chunks() as usize {
        leaves = current.to_vec();
        uploads.clear();
    }
    let mut candidates = leaves.clone();
    candidates.sort_by(|a, b| {
        error(terrain, *b, matrix, viewport).total_cmp(&error(terrain, *a, matrix, viewport))
    });
    for node in candidates {
        let Some(children) = node.children() else {
            continue;
        };
        if error(terrain, node, matrix, viewport)
            <= f64::from(policy.split_error_millipixels()) / 1000.0
        {
            continue;
        }
        if uploads.len() + children.len() > policy.uploads_per_frame() as usize
            || current.len() + retired + uploads.len() + children.len()
                > policy.maximum_resident_chunks() as usize
        {
            continue;
        }
        leaves.retain(|leaf| *leaf != node);
        leaves.extend(children.iter().copied());
        uploads.extend(children);
    }
    TerrainSelection { leaves, uploads }
}

fn merge(
    terrain: &SceneTerrain,
    node: TerrainChunk,
    matrix: Mat4,
    viewport: [u32; 2],
    leaves: &mut Vec<TerrainChunk>,
    uploads: &mut Vec<TerrainChunk>,
) {
    if leaves.contains(&node) {
        return;
    }
    let policy = terrain.streaming_policy().expect("streaming policy");
    let threshold = f64::from(policy.split_error_millipixels()) / 1200.0;
    if error(terrain, node, matrix, viewport) < threshold {
        if uploads.is_empty() && leaves.len() < policy.maximum_resident_chunks() as usize {
            leaves.retain(|leaf| !contains(node, *leaf));
            leaves.push(node);
            uploads.push(node);
        }
    } else if let Some(children) = node.children() {
        for child in children {
            merge(terrain, child, matrix, viewport, leaves, uploads);
        }
    }
}
fn contains(parent: TerrainChunk, child: TerrainChunk) -> bool {
    (0..2).all(|axis| {
        parent.minimum[axis] <= child.minimum[axis] && parent.maximum[axis] >= child.maximum[axis]
    })
}

/// Maximum projected cell-axis footprint, sampled at the chunk's eight bounding corners.
/// A near-plane intersection requests finest geometry rather than dividing by zero.
pub fn error(terrain: &SceneTerrain, node: TerrainChunk, matrix: Mat4, viewport: [u32; 2]) -> f64 {
    let points = points(terrain, node);
    let clips = points.map(|point| matrix * point.extend(1.0));
    if outside(&clips) {
        return 0.0;
    }
    let cell = crate::geometry::millimeter_to_meter(
        i64::from(node.stride) * i64::from(terrain.sample_spacing_millimeters()),
    );
    let mut result = 0.0_f64;
    for (point, clip) in points.into_iter().zip(clips) {
        if clip.w <= 0.0 {
            return f64::INFINITY;
        }
        for axis in [Vec3::X, Vec3::Y, Vec3::Z] {
            let next = matrix * (point + axis * cell).extend(1.0);
            if next.w <= 0.0 {
                return f64::INFINITY;
            }
            let delta = (next.truncate() / next.w - clip.truncate() / clip.w).abs();
            result = result
                .max(f64::from(delta.x) * f64::from(viewport[0]) / 2.0)
                .max(f64::from(delta.y) * f64::from(viewport[1]) / 2.0);
        }
    }
    result
}
fn points(terrain: &SceneTerrain, node: TerrainChunk) -> [Vec3; 8] {
    let height = elevation(terrain);
    std::array::from_fn(|index| {
        let x = if index & 1 == 0 {
            node.minimum[0]
        } else {
            node.maximum[0]
        };
        let y = if index & 2 == 0 {
            node.minimum[1]
        } else {
            node.maximum[1]
        };
        Vec3::new(
            crate::geometry::millimeter_to_meter(
                terrain.origin_east_millimeters()
                    + i64::from(x) * i64::from(terrain.sample_spacing_millimeters()),
            ),
            crate::geometry::millimeter_to_meter(
                terrain.origin_north_millimeters()
                    + i64::from(y) * i64::from(terrain.sample_spacing_millimeters()),
            ),
            height[index / 4],
        )
    })
}
fn outside(clips: &[glam::Vec4; 8]) -> bool {
    (0..6).any(|plane| {
        clips.iter().all(|p| match plane {
            0 => p.x < -p.w,
            1 => p.x > p.w,
            2 => p.y < -p.w,
            3 => p.y > p.w,
            4 => p.z < 0.0,
            _ => p.z > p.w,
        })
    })
}

pub(super) fn elevation(terrain: &SceneTerrain) -> [f32; 2] {
    terrain
        .elevation_bounds()
        .map(|height| crate::geometry::millimeter_to_meter(i64::from(height)))
}
