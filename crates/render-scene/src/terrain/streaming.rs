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

/// Admitted next leaves in chunk-key order and complete chunk uploads.
/// Removed leaves are evicted; upload priority is independent of resident order.
/// A replacement is admitted only when every chunk it uploads is ready, so the
/// current leaves keep covering the grid while a replacement's meshes are built.
#[derive(Debug)]
pub struct TerrainSelection {
    pub leaves: Vec<TerrainChunk>,
    pub uploads: Vec<TerrainChunk>,
    /// Chunks of replacements the budget admits that wait for their meshes, in priority order.
    pub unready: Vec<TerrainChunk>,
    /// Built chunks kept for a later frame: those of admitted replacements that wait for an
    /// unready sibling, and built prefetched chunks.
    pub held: Vec<TerrainChunk>,
    /// Unbuilt chunks of the next replacements past this frame's upload budget, in priority
    /// order, so that their meshes are built while this frame's uploads are drawn.
    pub prefetch: Vec<TerrainChunk>,
    /// Desired selection and replacement work remaining after budget admission.
    pub observation: TerrainSelectionObservation,
}

/// Desired selection compared with admitted coverage, including blocked merges.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TerrainSelectionObservation {
    pub selected_leaves: usize,
    pub drawn_at_detail: usize,
    pub drawn_coarse: usize,
    pub unresolved_leaves: usize,
}

pub(crate) fn observe(
    terrain: &SceneTerrain,
    current: &[TerrainChunk],
    matrix: Mat4,
    viewport: [u32; 2],
) -> TerrainSelectionObservation {
    observation(&desired(terrain, current, matrix, viewport), current)
}

fn observation(desired: &[TerrainChunk], leaves: &[TerrainChunk]) -> TerrainSelectionObservation {
    let drawn_coarse = leaves
        .iter()
        .filter(|leaf| {
            desired
                .iter()
                .any(|next| next != *leaf && contains(**leaf, *next))
        })
        .count();
    TerrainSelectionObservation {
        selected_leaves: desired.len(),
        drawn_at_detail: leaves.len() - drawn_coarse,
        drawn_coarse,
        unresolved_leaves: desired.iter().filter(|node| !leaves.contains(node)).count(),
    }
}

/// Refines by projected cell footprint. Cached splits merge below 1/1.2 of the
/// threshold. Every replacement is atomic, so budget pressure retains coverage.
/// Resident limits include old leaves and replacement uploads during preparation.
/// Every chunk counts as ready, so the selection never waits for a mesh.
pub fn select(
    terrain: &SceneTerrain,
    current: &[TerrainChunk],
    matrix: Mat4,
    viewport: [u32; 2],
) -> TerrainSelection {
    select_with_retired(terrain, current, matrix, viewport, 0, &|_| true)
}

/// The leaves, uploads and replacement work of one selection, built up in admission order.
struct Plan<'a> {
    leaves: Vec<TerrainChunk>,
    uploads: Vec<TerrainChunk>,
    /// Chunks of admitted replacements that wait on a mesh, ready or not.
    held: Vec<TerrainChunk>,
    /// Chunks of replacements past the upload budget, ready or not.
    prefetch: Vec<TerrainChunk>,
    ready: &'a dyn Fn(TerrainChunk) -> bool,
}
impl<'a> Plan<'a> {
    fn new(current: &[TerrainChunk], ready: &'a dyn Fn(TerrainChunk) -> bool) -> Self {
        Self {
            leaves: current.to_vec(),
            uploads: Vec::new(),
            held: Vec::new(),
            prefetch: Vec::new(),
            ready,
        }
    }
    /// Held replacements count against every budget as if they were uploaded, so that the
    /// plan equals the one for fully built meshes and a build is requested only for work the
    /// budget would admit.
    fn admitted(&self) -> usize {
        self.uploads.len() + self.held.len()
    }
    fn is_ready(&self, chunk: TerrainChunk) -> bool {
        (self.ready)(chunk)
    }
    /// Requests a replacement the upload budget defers by one frame. Without this the worker
    /// idles in every frame that admits a full budget, which halves refinement throughput.
    /// Prefetch is limited to one more frame's budget and to the resident limit.
    fn prefetch(
        &mut self,
        children: &[TerrainChunk],
        occupied: usize,
        policy: TerrainStreamingPolicy,
    ) {
        let requested = self.prefetch.len() + children.len();
        if requested <= policy.uploads_per_frame() as usize
            && occupied + self.admitted() + requested <= policy.maximum_resident_chunks() as usize
        {
            self.prefetch.extend_from_slice(children);
        }
    }
    fn finish(self, desired: &[TerrainChunk]) -> TerrainSelection {
        let (mut held, unready): (Vec<_>, _) = self
            .held
            .into_iter()
            .partition(|chunk| (self.ready)(*chunk));
        let (built, prefetch): (Vec<_>, _) = self
            .prefetch
            .into_iter()
            .partition(|chunk| (self.ready)(*chunk));
        held.extend(built);
        let mut leaves = self.leaves;
        leaves.sort_unstable();
        TerrainSelection {
            observation: observation(desired, &leaves),
            leaves,
            uploads: self.uploads,
            unready,
            held,
            prefetch,
        }
    }
}

/// `ready` says whether a chunk's mesh is built. A replacement with an unbuilt chunk is
/// held back and reported in `unready`, and its leaves keep being drawn.
pub(crate) fn select_with_retired(
    terrain: &SceneTerrain,
    current: &[TerrainChunk],
    matrix: Mat4,
    viewport: [u32; 2],
    retired: usize,
    ready: &dyn Fn(TerrainChunk) -> bool,
) -> TerrainSelection {
    let policy = terrain.streaming_policy().expect("streaming policy");
    let mut plan = Plan::new(current, ready);
    let desired = desired(terrain, current, matrix, viewport);
    merge(
        terrain,
        TerrainChunk::root(terrain, policy),
        &desired,
        &mut plan,
    );
    if current.len() + retired + plan.admitted() > policy.maximum_resident_chunks() as usize {
        plan = Plan::new(current, ready);
    }
    refine(
        terrain,
        &desired,
        (matrix, viewport),
        current.len() + retired,
        &mut plan,
    );
    plan.finish(&desired)
}

fn refine(
    terrain: &SceneTerrain,
    desired: &[TerrainChunk],
    (matrix, viewport): (Mat4, [u32; 2]),
    occupied: usize,
    plan: &mut Plan<'_>,
) {
    let policy = terrain.streaming_policy().expect("streaming policy");
    let mut candidates = plan.leaves.clone();
    candidates.sort_by(|a, b| {
        error(terrain, *b, matrix, viewport).total_cmp(&error(terrain, *a, matrix, viewport))
            // Larger strides precede smaller strides, ordering levels from the root.
            .then_with(|| b.stride.cmp(&a.stride))
            .then_with(|| a.cmp(b))
    });
    for node in candidates {
        let Some(children) = node.children() else {
            continue;
        };
        if !desired
            .iter()
            .any(|next| *next != node && contains(node, *next))
        {
            continue;
        }
        if plan.admitted() + children.len() > policy.uploads_per_frame() as usize
            || occupied + plan.admitted() + children.len()
                > policy.maximum_resident_chunks() as usize
        {
            plan.prefetch(&children, occupied, policy);
            continue;
        }
        if children.iter().all(|child| plan.is_ready(*child)) {
            plan.leaves.retain(|leaf| *leaf != node);
            plan.leaves.extend(children.iter().copied());
            plan.uploads.extend(children);
        } else {
            plan.held.extend(children);
        }
    }
}

fn merge(
    terrain: &SceneTerrain,
    node: TerrainChunk,
    desired: &[TerrainChunk],
    plan: &mut Plan<'_>,
) {
    if plan.leaves.contains(&node) {
        return;
    }
    let policy = terrain.streaming_policy().expect("streaming policy");
    if desired.contains(&node) {
        if plan.admitted() == 0 && plan.leaves.len() < policy.maximum_resident_chunks() as usize {
            if plan.is_ready(node) {
                plan.leaves.retain(|leaf| !contains(node, *leaf));
                plan.leaves.push(node);
                plan.uploads.push(node);
            } else {
                plan.held.push(node);
            }
        }
    } else if let Some(children) = node.children() {
        for child in children {
            merge(terrain, child, desired, plan);
        }
    }
}
fn desired(
    terrain: &SceneTerrain,
    current: &[TerrainChunk],
    matrix: Mat4,
    viewport: [u32; 2],
) -> Vec<TerrainChunk> {
    let policy = terrain.streaming_policy().expect("streaming policy");
    let mut pending = vec![TerrainChunk::root(terrain, policy)];
    let mut selected = Vec::new();
    while let Some(node) = pending.pop() {
        let refined = current
            .iter()
            .any(|leaf| *leaf != node && contains(node, *leaf));
        let footprint = error(terrain, node, matrix, viewport);
        let threshold = f64::from(policy.split_error_millipixels());
        let subdivide = if refined {
            footprint >= threshold / 1200.0
        } else {
            footprint > threshold / 1000.0
        };
        if subdivide && let Some(children) = node.children() {
            pending.extend(children);
        } else {
            selected.push(node);
        }
    }
    selected
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
/// World bounds of a chunk's geometry: its sample corners across the terrain's elevation
/// range, down to the skirt one metre below the lowest sample.
pub(crate) fn chunk_bounds(terrain: &SceneTerrain, node: TerrainChunk) -> [Vec3; 2] {
    let corners = points(terrain, node);
    let mut minimum = corners.iter().copied().fold(Vec3::INFINITY, Vec3::min);
    let maximum = corners.iter().copied().fold(Vec3::NEG_INFINITY, Vec3::max);
    minimum.z -= 1.0;
    [minimum, maximum]
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
