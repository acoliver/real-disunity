//! A detailed look at the camera renders a host asks about: how the target was set up, where the
//! vertices of each draw land in clip space, and what the fragment stage reads.
//!
//! The look is made on the CPU while the draws are planned, from the constants and meshes the
//! draws bind, so it costs nothing on the GPU. It is meant for a render that produces nothing, to
//! tell a draw that is clipped away from one that is shaded to zero.

use std::collections::{BTreeMap, BTreeSet};

use crate::constants::PackedConstants;
use crate::ids::{ShaderId, TextureId};
use crate::unity::buffers::BufferLayout;
use crate::unity::constants::packed_members;

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
    /// The textures the program samples, with the counts of their uploaded texels.
    pub textures: Vec<SampledTexture>,
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
}

impl DrawFacts {
    /// Notes the vertex stage's matrices and the fragment stage's members without a value or with
    /// only zeros, from the bytes packed for a buffer `visibility` says which stages read.
    pub(super) fn note_constants(
        &mut self,
        layout: &BufferLayout,
        packed: &PackedConstants,
        visibility: wgpu::ShaderStages,
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
                if packed.missing.contains(&name) {
                    self.without_value[stage].insert(name.clone());
                } else if bytes.iter().all(|byte| *byte == 0) {
                    self.zero[stage].insert(name.clone());
                }
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
}

/// A mesh's vertex count and, when it has a float position channel, its positions as x, y, z
/// triples.
pub(super) struct MeshPositions<'a> {
    pub(super) vertices: u64,
    pub(super) positions: Option<&'a [f32]>,
}

impl ProgramProbes {
    /// Notes one draw of `shader`'s pass `pass`.
    pub(super) fn add(
        &mut self,
        (shader, pass): (ShaderId, usize),
        names: &CameraProbe,
        facts: &DrawFacts,
        mesh: &MeshPositions<'_>,
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
                textures: Vec::new(),
            });
            self.without_value.push(Default::default());
            self.zero.push(Default::default());
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
            }
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
        for ((program, without_value), zero) in self
            .programs
            .iter_mut()
            .zip(self.without_value)
            .zip(self.zero)
        {
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
