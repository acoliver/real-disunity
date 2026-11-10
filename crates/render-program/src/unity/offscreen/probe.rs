//! A detailed look at the camera renders a host asks about: how the target was set up, where the
//! vertices of each draw land in clip space, and what the fragment stage reads.
//!
//! The look is made on the CPU while the draws are planned, from the constants and meshes the
//! draws bind, so it costs nothing on the GPU. It is meant for a render that produces nothing, to
//! tell a draw that is clipped away from one that is shaded to zero.

use std::collections::{BTreeMap, BTreeSet};

use std::ops::Range;

use crate::constants::PackedConstants;
use crate::ids::{ShaderId, TextureId};
use crate::properties::PropertySet;
use crate::unity::buffers::BufferLayout;
use crate::unity::constants::packed_members;
use crate::unity::state::RenderState;

/// What a host asks to look at. The names are those its shaders give the draw's object-to-world
/// matrix and the camera's view-projection matrix, which this crate does not know.
#[derive(Clone, Debug, Default)]
pub struct CameraProbe {
    /// The render textures whose camera renders are examined.
    pub targets: BTreeSet<TextureId>,
    /// The render textures of test renders: camera renders drawn like any other but left out of
    /// every count and report of the frame, so a second render of the same draws does not double
    /// them.
    pub test_targets: BTreeSet<TextureId>,
    pub object_to_world: String,
    pub view_projection: String,
    /// How the draws of a test render into a render texture differ from the state their passes
    /// declare. A test render of a target not listed draws as declared.
    pub variants: BTreeMap<TextureId, StateVariant>,
}

/// A change to the state a test render's draws are made with, to find which part of the declared
/// state, or of the fragment stage, leaves a render with no written texel. Each change is made
/// to every draw of the render.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StateVariant {
    /// Neither face is culled.
    pub cull_none: bool,
    /// The fragment replaces the target's texel and is not blended with it.
    pub blend_off: bool,
    /// The depth test always passes.
    pub depth_always: bool,
    /// The fragment stage is replaced by one that writes [`CONSTANT_COLOR`], whatever it read.
    pub constant_fragment: bool,
}

/// What a constant fragment stage writes: channels 0 and 2 below and channels 1 and 3 above the
/// clear colours a test render uses (0.3 to 0.82), so that a blend that keeps the lesser or the
/// greater of the two still changes a texel.
pub const CONSTANT_COLOR: [f32; 4] = [0.0625, 0.9375, 0.0625, 0.9375];

/// The fragment stage of [`StateVariant::constant_fragment`].
pub(super) const CONSTANT_FRAGMENT: &str = "@fragment fn constant_fragment() -> @location(0) vec4<f32> { return vec4<f32>(0.0625, 0.9375, 0.0625, 0.9375); }";
/// The entry point of [`CONSTANT_FRAGMENT`].
pub(super) const CONSTANT_ENTRY: &str = "constant_fragment";

impl StateVariant {
    /// Whether the variant changes anything.
    #[must_use]
    pub fn is_declared(self) -> bool {
        self == Self::default()
    }

    /// The state with the variant's changes made.
    pub(super) fn apply(self, state: &mut RenderState) {
        if self.cull_none {
            state.cull = None;
        }
        if self.blend_off {
            state.blend = None;
        }
        if self.depth_always {
            state.depth_compare = wgpu::CompareFunction::Always;
        }
    }
}

/// How a camera render was set up, in words. The viewport is the whole target; a camera with a
/// partial viewport is left out before it gets here.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PassSetup {
    pub viewport: [u32; 2],
    /// `clear [r, g, b, a]`, `load` or `no color`.
    pub color: String,
    /// `clear 1.0`, `load` or `no depth`.
    pub depth: String,
}

/// Vertices by where a clip-space position puts them. A position is inside when its x and y are
/// within -w..w and its depth within 0..w, behind when w is not positive, else outside.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ClipCounts {
    pub inside: u64,
    pub outside: u64,
    pub behind: u64,
}

/// A texture a program samples. `texels` and `nonzero` count the texels (the blocks of a
/// block-compressed format) of the bytes the host uploaded, and are `None` for a render texture.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SampledTexture {
    pub name: String,
    pub texture: TextureId,
    /// Whether the texture is an uploaded source, which has counts, and not a render texture.
    pub uploaded: bool,
    pub texels: Option<usize>,
    pub nonzero: Option<usize>,
}

/// What the draws of one program (a shader's pass) did in a probed camera render.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProgramProbe {
    pub shader: ShaderId,
    pub pass: usize,
    pub draws: usize,
    /// The vertices of the drawn meshes, summed over the draws.
    pub vertices: u64,
    /// Where the vertices land when the clip position is `view_projection * (object_to_world *
    /// position)` with the matrices as the draw bound them, and when it is the same product of
    /// their transposes.
    pub clip: ClipCounts,
    pub clip_transposed: ClipCounts,
    /// Draws whose mesh has no float position channel, and draws that bound no matrix of the
    /// names the host gave. They are not in the clip counts.
    pub without_positions: usize,
    pub without_matrices: usize,
    /// The members of the constant buffers the vertex stage reads that no layer gave a value,
    /// and that have one whose bytes are all zero.
    pub vertex_without_value: Vec<String>,
    pub vertex_zero: Vec<String>,
    /// The same for the constant buffers the fragment stage reads.
    pub fragment_without_value: Vec<String>,
    pub fragment_zero: Vec<String>,
    /// For each member of the fragment stage's constant buffers that some draw gave no value or
    /// only zeros, where its value came from and for how many draws: a layer of the draw (see
    /// [`DRAW_LAYERS`](super::DRAW_LAYERS)) or nowhere.
    pub fragment_origins: Vec<MemberOrigin>,
    /// The triangles of the program's draws and what becomes of them.
    pub triangles: TriangleCounts,
    /// The textures the program samples, with the counts of their uploaded texels.
    pub textures: Vec<SampledTexture>,
}

/// Where a constant buffer member's value came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Origin {
    /// No layer had the member's property.
    Unset,
    /// The layer at this place in the draw's layers, highest first.
    Layer(usize),
}

/// How many draws a member's value came from one origin, and how many of those were all zero.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemberOrigin {
    pub name: String,
    pub origin: Origin,
    pub draws: u64,
    pub zero_draws: u64,
}

/// The triangles of a program's draws, each counted once, by the first of these that holds:
/// `behind` has a vertex at or behind the camera's plane (w not positive), `outside` has all
/// three vertices beyond the same side of the clip volume, `degenerate` has no area in clip
/// space. The rest are `counter_clockwise` or `clockwise` as seen in clip space with Y up, and
/// each is `culled` or `drawn` by the state the pass declares and the winding the draw treats as
/// front.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TriangleCounts {
    pub total: u64,
    pub behind: u64,
    pub outside: u64,
    pub degenerate: u64,
    pub counter_clockwise: u64,
    pub clockwise: u64,
    pub culled: u64,
    pub drawn: u64,
}

/// What one draw bound that the look needs, noted while its constants and textures are bound.
#[derive(Clone, Debug, Default)]
pub(super) struct DrawFacts {
    /// The 4x4 matrices of the buffers the vertex stage reads, by name.
    pub(super) matrices: Vec<(String, [f32; 16])>,
    /// The members without a value or with only zeros, for the vertex and the fragment stage.
    pub(super) without_value: [BTreeSet<String>; 2],
    pub(super) zero: [BTreeSet<String>; 2],
    /// The textures bound, by property name.
    pub(super) textures: Vec<(String, TextureId)>,
    /// For the vertex and the fragment stage, each member read with where its value came from
    /// and whether it was all zeros.
    pub(super) origins: [Vec<(String, Origin, bool)>; 2],
}

impl DrawFacts {
    /// Notes the vertex stage's matrices and the fragment stage's members without a value or with
    /// only zeros, from the bytes packed for a buffer `visibility` says which stages read.
    pub(super) fn note_constants(
        &mut self,
        layout: &BufferLayout,
        packed: &PackedConstants,
        visibility: wgpu::ShaderStages,
        layers: &[&PropertySet],
    ) {
        for (name, bytes) in packed_members(layout, &packed.bytes) {
            // A matrix no layer gave a value is zeros, which is not a matrix the draw bound.
            if visibility.contains(wgpu::ShaderStages::VERTEX)
                && !packed.missing.contains(&name)
                && let Some(matrix) = matrix_of(bytes)
            {
                self.matrices.push((name.clone(), matrix));
            }
            for (stage, wanted) in [wgpu::ShaderStages::VERTEX, wgpu::ShaderStages::FRAGMENT]
                .into_iter()
                .enumerate()
            {
                if !visibility.contains(wanted) {
                    continue;
                }
                let zero = bytes.iter().all(|byte| *byte == 0);
                let origin = if packed.missing.contains(&name) {
                    self.without_value[stage].insert(name.clone());
                    Origin::Unset
                } else {
                    if zero {
                        self.zero[stage].insert(name.clone());
                    }
                    layers
                        .iter()
                        .position(|layer| layer.get(&name).is_some())
                        .map_or(Origin::Unset, Origin::Layer)
                };
                self.origins[stage].push((name.clone(), origin, zero));
            }
        }
    }
}

/// The sixteen floats of a member of exactly four registers.
fn matrix_of(bytes: &[u8]) -> Option<[f32; 16]> {
    if bytes.len() != 64 {
        return None;
    }
    let mut matrix = [0.0; 16];
    for (value, float) in matrix.iter_mut().zip(bytes.chunks_exact(4)) {
        *value = f32::from_le_bytes([float[0], float[1], float[2], float[3]]);
    }
    Some(matrix)
}

/// The product of two 4x4 matrices stored column by column.
fn multiply(left: &[f32; 16], right: &[f32; 16]) -> [f32; 16] {
    let mut product = [0.0; 16];
    for column in 0..4 {
        for row in 0..4 {
            product[column * 4 + row] = (0..4)
                .map(|k| left[k * 4 + row] * right[column * 4 + k])
                .sum();
        }
    }
    product
}

fn transpose(matrix: &[f32; 16]) -> [f32; 16] {
    let mut transposed = [0.0; 16];
    for column in 0..4 {
        for row in 0..4 {
            transposed[row * 4 + column] = matrix[column * 4 + row];
        }
    }
    transposed
}

impl ClipCounts {
    /// Counts the clip positions of `positions`, tightly packed x, y, z triples, after `matrix`.
    fn add(&mut self, matrix: &[f32; 16], positions: &[f32]) {
        for point in positions.chunks_exact(3) {
            let at = |row: usize| {
                matrix[row].mul_add(
                    point[0],
                    matrix[4 + row].mul_add(point[1], matrix[8 + row] * point[2]),
                ) + matrix[12 + row]
            };
            let (x, y, z, w) = (at(0), at(1), at(2), at(3));
            if w <= 0.0 || w.is_nan() {
                self.behind += 1;
            } else if x.abs() <= w && y.abs() <= w && (0.0..=w).contains(&z) {
                self.inside += 1;
            } else {
                self.outside += 1;
            }
        }
    }
}

fn find<'a>(facts: &'a DrawFacts, name: &str) -> Option<&'a [f32; 16]> {
    facts
        .matrices
        .iter()
        .find(|(found, _)| found == name)
        .map(|(_, matrix)| matrix)
}

/// The programs of one camera render, in the order their first draw was planned.
#[derive(Default)]
pub(super) struct ProgramProbes {
    programs: Vec<ProgramProbe>,
    places: BTreeMap<(ShaderId, usize), usize>,
    /// For each program, the members without a value and with zeros, by stage.
    without_value: Vec<[BTreeSet<String>; 2]>,
    zero: Vec<[BTreeSet<String>; 2]>,
    /// For each program, the fragment stage's members by (name, origin), with the draws and the
    /// draws among them that were all zeros.
    origins: Vec<BTreeMap<(String, Origin), (u64, u64)>>,
}

/// A mesh's vertex count and, when it has a float position channel, its positions as x, y, z
/// triples.
pub(super) struct MeshPositions<'a> {
    pub(super) vertices: u64,
    pub(super) positions: Option<&'a [f32]>,
}

/// The triangles one draw reads and the state that decides which of them are drawn.
pub(super) struct DrawTriangles<'a> {
    /// The mesh's indices, or `None` when the draw reads its vertices in order.
    pub(super) indices: Option<&'a [u32]>,
    pub(super) elements: Range<u32>,
    pub(super) base_vertex: i32,
    pub(super) cull: Option<wgpu::Face>,
    pub(super) front_face: wgpu::FrontFace,
}

/// The clip-space position, x, y, z, w, of each vertex of `positions` after `matrix`.
fn clip_positions(matrix: &[f32; 16], positions: &[f32]) -> Vec<[f32; 4]> {
    positions
        .chunks_exact(3)
        .map(|point| {
            let at = |row: usize| {
                matrix[row].mul_add(
                    point[0],
                    matrix[4 + row].mul_add(point[1], matrix[8 + row] * point[2]),
                ) + matrix[12 + row]
            };
            [at(0), at(1), at(2), at(3)]
        })
        .collect()
}

/// Whether all three vertices lie beyond the same side of the clip volume.
fn all_outside(corners: &[[f32; 4]; 3]) -> bool {
    let beyond = |test: fn(&[f32; 4]) -> bool| corners.iter().all(test);
    beyond(|v| v[0] < -v[3])
        || beyond(|v| v[0] > v[3])
        || beyond(|v| v[1] < -v[3])
        || beyond(|v| v[1] > v[3])
        || beyond(|v| v[2] < 0.0)
        || beyond(|v| v[2] > v[3])
}

impl TriangleCounts {
    /// Counts the triangles `draw` reads from the vertices at `clip`.
    fn add(&mut self, clip: &[[f32; 4]], draw: &DrawTriangles<'_>) {
        let corner = |element: u32| -> Option<[f32; 4]> {
            let index = match draw.indices {
                Some(indices) => i64::from(*indices.get(element as usize)?),
                None => i64::from(element),
            } + i64::from(draw.base_vertex);
            clip.get(usize::try_from(index).ok()?).copied()
        };
        for first in draw.elements.clone().step_by(3) {
            let corners = [first, first + 1, first + 2].map(corner);
            let [Some(a), Some(b), Some(c)] = corners else {
                continue;
            };
            self.total += 1;
            let corners = [a, b, c];
            if corners.iter().any(|v| v[3] <= 0.0 || v[3].is_nan()) {
                self.behind += 1;
            } else if all_outside(&corners) {
                self.outside += 1;
            } else {
                let [(ax, ay), (bx, by), (cx, cy)] = corners.map(|v| (v[0] / v[3], v[1] / v[3]));
                let area = (bx - ax) * (cy - ay) - (cx - ax) * (by - ay);
                if area == 0.0 || area.is_nan() {
                    self.degenerate += 1;
                } else {
                    self.wound(area > 0.0, draw);
                }
            }
        }
    }

    /// Counts a triangle wound counter-clockwise in clip space with Y up (`counter_clockwise`).
    /// Clip Y grows down the image, so such a triangle is clockwise in the image.
    fn wound(&mut self, counter_clockwise: bool, draw: &DrawTriangles<'_>) {
        if counter_clockwise {
            self.counter_clockwise += 1;
        } else {
            self.clockwise += 1;
        }
        let counter_clockwise_in_image = !counter_clockwise;
        let front = (draw.front_face == wgpu::FrontFace::Ccw) == counter_clockwise_in_image;
        let culled = match draw.cull {
            None => false,
            Some(wgpu::Face::Back) => !front,
            Some(wgpu::Face::Front) => front,
        };
        if culled {
            self.culled += 1;
        } else {
            self.drawn += 1;
        }
    }
}

impl ProgramProbes {
    /// Notes one draw of `shader`'s pass `pass`.
    pub(super) fn add(
        &mut self,
        (shader, pass): (ShaderId, usize),
        names: &CameraProbe,
        facts: &DrawFacts,
        (mesh, triangles): (&MeshPositions<'_>, &DrawTriangles<'_>),
        textures: impl IntoIterator<Item = SampledTexture>,
    ) {
        let place = *self.places.entry((shader, pass)).or_insert_with(|| {
            self.programs.push(ProgramProbe {
                shader,
                pass,
                draws: 0,
                vertices: 0,
                clip: ClipCounts::default(),
                clip_transposed: ClipCounts::default(),
                without_positions: 0,
                without_matrices: 0,
                vertex_without_value: Vec::new(),
                vertex_zero: Vec::new(),
                fragment_without_value: Vec::new(),
                fragment_zero: Vec::new(),
                fragment_origins: Vec::new(),
                triangles: TriangleCounts::default(),
                textures: Vec::new(),
            });
            self.without_value.push(Default::default());
            self.zero.push(Default::default());
            self.origins.push(BTreeMap::new());
            self.programs.len() - 1
        });
        let program = &mut self.programs[place];
        program.draws += 1;
        program.vertices += mesh.vertices;
        let matrices = find(facts, &names.object_to_world).zip(find(facts, &names.view_projection));
        match (matrices, mesh.positions) {
            (None, _) => program.without_matrices += 1,
            (Some(_), None) => program.without_positions += 1,
            (Some((object_to_world, view_projection)), Some(positions)) => {
                let whole = multiply(view_projection, object_to_world);
                program.clip.add(&whole, positions);
                let swapped = multiply(&transpose(view_projection), &transpose(object_to_world));
                program.clip_transposed.add(&swapped, positions);
                program
                    .triangles
                    .add(&clip_positions(&whole, positions), triangles);
            }
        }
        for (name, origin, zero) in &facts.origins[1] {
            let entry = self.origins[place]
                .entry((name.clone(), *origin))
                .or_default();
            entry.0 += 1;
            entry.1 += u64::from(*zero);
        }
        for stage in 0..2 {
            self.without_value[place][stage].extend(facts.without_value[stage].iter().cloned());
            self.zero[place][stage].extend(facts.zero[stage].iter().cloned());
        }
        for texture in textures {
            if !program.textures.contains(&texture) {
                program.textures.push(texture);
            }
        }
    }

    pub(super) fn finish(mut self) -> Vec<ProgramProbe> {
        for (((program, without_value), zero), origins) in self
            .programs
            .iter_mut()
            .zip(self.without_value)
            .zip(self.zero)
            .zip(self.origins)
        {
            // Only the members that some draw gave no value or only zeros are of interest.
            let wanted: BTreeSet<&String> = origins
                .iter()
                .filter(|((_, origin), (_, zero_draws))| {
                    *origin == Origin::Unset || *zero_draws > 0
                })
                .map(|((name, _), _)| name)
                .collect();
            program.fragment_origins = origins
                .iter()
                .filter(|((name, _), _)| wanted.contains(name))
                .map(|((name, origin), (draws, zero_draws))| MemberOrigin {
                    name: name.clone(),
                    origin: *origin,
                    draws: *draws,
                    zero_draws: *zero_draws,
                })
                .collect();
            let [vertex, fragment] = without_value;
            program.vertex_without_value = vertex.into_iter().collect();
            program.fragment_without_value = fragment.into_iter().collect();
            let [vertex, fragment] = zero;
            program.vertex_zero = vertex.into_iter().collect();
            program.fragment_zero = fragment.into_iter().collect();
        }
        self.programs
    }
}
