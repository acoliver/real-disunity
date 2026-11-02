//! Brings the meshes a player's managed code stored into scene meshes, and reports what each
//! frame shows.
//!
//! The vertices keep the mesh's own axes. The presentation's object matrix already maps them
//! into the camera's basis, so converting positions here as well would apply the change twice.
//! The presented camera is left handed, whose front face is clockwise (the render scene sets
//! that winding from the camera's convention), and the managed engine's meshes wind their front
//! faces clockwise in the same handedness, so the indices stay in the order the mesh gave them.

use std::{collections::BTreeMap, ops::Range};

use worldrunner_disunity_runtime::{DisunityHost, MeshObjectView, PlayerMesh};
use worldrunner_render_plan::{SceneMesh, SceneMeshLod, SceneValidationError, SceneVertex};

use crate::PlayError;

const MICRO: f64 = 1_000_000.0;
const MILLIMETERS_PER_METER: f64 = 1_000.0;
/// The largest magnitude `f64` holds exactly as a whole number.
const EXACT_LIMIT: f64 = 4_503_599_627_370_496.0;

/// The indices one material slot draws, as the managed mesh numbered its sub-mesh.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SlotRun {
    pub slot: u32,
    pub indices: Range<u32>,
}

/// A stored mesh as a scene mesh, with the slot each run of its triangles belongs to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayerSceneMesh {
    pub mesh: SceneMesh,
    pub slots: Vec<SlotRun>,
}

#[derive(Debug, thiserror::Error)]
pub enum MeshConversionError {
    #[error("{0}")]
    OutOfRange(String),
    #[error("the mesh cannot be drawn: {0}")]
    Scene(#[from] SceneValidationError),
}

/// Converts a stored mesh: metres become whole millimeters, normals and texture coordinates
/// become micro units, missing texture coordinates are zero and missing normals are the area
/// weighted average of the faces around each vertex.
pub fn player_scene_mesh(mesh: &PlayerMesh) -> Result<PlayerSceneMesh, MeshConversionError> {
    let normals: Vec<[f64; 3]> = if mesh.normals.is_empty() {
        face_normals(&mesh.positions, &mesh.indices)
    } else {
        mesh.normals.iter().map(|n| n.map(f64::from)).collect()
    };
    let mut vertices = Vec::with_capacity(mesh.positions.len());
    for (index, position) in mesh.positions.iter().enumerate() {
        let uv = mesh.uvs.get(index).copied().unwrap_or([0.0; 2]);
        vertices.push(SceneVertex {
            tangent_micro: None,
            position_millimeters: millimeters(index, *position)?,
            normal_micro: normals
                .get(index)
                .map_or(Ok([0; 3]), |n| micro3(index, *n))?,
            texcoord_micro: [
                micro(index, f64::from(uv[0]))?,
                micro(index, f64::from(uv[1]))?,
            ],
        });
    }
    let lod = SceneMeshLod::new(0, vertices, mesh.indices.clone(), None)?;
    Ok(PlayerSceneMesh {
        mesh: SceneMesh::new(vec![lod])?,
        slots: mesh
            .submeshes
            .iter()
            .map(|run| SlotRun {
                slot: run.slot,
                indices: run.first_index..run.first_index + run.index_count,
            })
            .collect(),
    })
}

fn millimeters(index: usize, position: [f32; 3]) -> Result<[i64; 3], MeshConversionError> {
    let mut out = [0; 3];
    for (slot, metres) in out.iter_mut().zip(position) {
        *slot = round_to_i64(f64::from(metres) * MILLIMETERS_PER_METER).ok_or_else(|| {
            MeshConversionError::OutOfRange(format!(
                "vertex {index} lies {metres} metres from the origin, beyond the scene's range"
            ))
        })?;
    }
    Ok(out)
}

fn micro(index: usize, value: f64) -> Result<i64, MeshConversionError> {
    round_to_i64(value * MICRO).ok_or_else(|| {
        MeshConversionError::OutOfRange(format!("vertex {index} has the value {value}"))
    })
}

fn micro3(index: usize, value: [f64; 3]) -> Result<[i64; 3], MeshConversionError> {
    Ok([
        micro(index, value[0])?,
        micro(index, value[1])?,
        micro(index, value[2])?,
    ])
}

/// The average of the face normals `(b - a) x (c - a)` around each vertex, weighted by face area
/// (the cross product's length) and normalised; zero for a vertex no triangle uses.
fn face_normals(positions: &[[f32; 3]], indices: &[u32]) -> Vec<[f64; 3]> {
    let mut sums = vec![[0.0_f64; 3]; positions.len()];
    for triangle in indices.chunks_exact(3) {
        let corners: Option<Vec<[f64; 3]>> = triangle
            .iter()
            .map(|i| positions.get(*i as usize).map(|p| p.map(f64::from)))
            .collect();
        let Some([a, b, c]) = corners
            .as_deref()
            .and_then(|c| <&[[f64; 3]; 3]>::try_from(c).ok())
        else {
            continue;
        };
        let (u, v) = (sub(*b, *a), sub(*c, *a));
        let face = [
            u[1] * v[2] - u[2] * v[1],
            u[2] * v[0] - u[0] * v[2],
            u[0] * v[1] - u[1] * v[0],
        ];
        for corner in triangle {
            for (sum, part) in sums[*corner as usize].iter_mut().zip(face) {
                *sum += part;
            }
        }
    }
    sums.iter().map(|sum| unit(*sum)).collect()
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

/// `sum` as a unit vector, or zero when it has no direction.
fn unit(sum: [f64; 3]) -> [f64; 3] {
    let length = sum.iter().map(|part| part * part).sum::<f64>().sqrt();
    if length > 0.0 && length.is_finite() {
        sum.map(|part| part / length)
    } else {
        [0.0; 3]
    }
}

/// `value` rounded half away from zero as an integer, or `None` when it is not finite or its
/// magnitude is above 2^52, so the whole number is exact. Built from the float's bits so that no
/// narrowing cast is needed.
pub(crate) fn round_to_i64(value: f64) -> Option<i64> {
    let rounded = value.round();
    if !rounded.is_finite() || rounded.abs() > EXACT_LIMIT {
        return None;
    }
    let bits = rounded.abs().to_bits();
    let exponent = i32::try_from((bits >> 52) & 0x7ff).ok()? - 1023;
    let magnitude: u64 = if exponent < 0 {
        0
    } else {
        let mantissa = (bits & ((1 << 52) - 1)) | (1 << 52);
        let shift = exponent - 52;
        if shift >= 0 {
            mantissa << u32::try_from(shift).ok()?
        } else {
            mantissa >> u32::try_from(-shift).ok()?
        }
    };
    let magnitude = i64::try_from(magnitude).ok()?;
    Some(if rounded.is_sign_negative() {
        -magnitude
    } else {
        magnitude
    })
}

/// The scene meshes of the stored meshes a frame shows, and the one stderr line per ten frames
/// that says what the frame holds.
#[derive(Debug, Default)]
pub struct PlayerMeshes {
    converted: BTreeMap<u64, (u64, PlayerSceneMesh)>,
    reported: std::collections::BTreeSet<u64>,
    frames: u64,
}

impl PlayerMeshes {
    /// Converts each mesh among the shown `objects` that is new or changed since the last call.
    /// A mesh that cannot be converted is named once on stderr and left out of the table.
    pub fn refresh(
        &mut self,
        host: &DisunityHost,
        objects: &[MeshObjectView],
    ) -> Result<(), PlayError> {
        for identity in objects.iter().map(|object| object.mesh) {
            let known = self.converted.get(&identity).map_or(0, |held| held.0);
            let Some(stored) = host.player_mesh(identity, known)? else {
                continue;
            };
            match player_scene_mesh(&stored) {
                Ok(scene) => {
                    self.converted.insert(identity, (stored.generation, scene));
                }
                Err(error) => {
                    self.converted.remove(&identity);
                    if self.reported.insert(identity) {
                        eprintln!("mesh {identity}: {error}; its renderers are left out");
                    }
                }
            }
        }
        self.frames += 1;
        if self.frames % 10 == 1 {
            self.log(objects);
        }
        Ok(())
    }

    /// The converted mesh stored under renderer `identity`.
    #[must_use]
    pub fn get(&self, identity: u64) -> Option<&PlayerSceneMesh> {
        self.converted.get(&identity).map(|held| &held.1)
    }

    fn log(&self, objects: &[MeshObjectView]) {
        let shown: std::collections::BTreeSet<u64> = objects.iter().map(|o| o.mesh).collect();
        let mut sizes: Vec<(usize, usize)> = shown
            .iter()
            .filter_map(|id| self.get(*id))
            .map(|held| {
                let lod = &held.mesh.lods()[0];
                (lod.vertices().len(), lod.indices().len() / 3)
            })
            .collect();
        sizes.sort_by(|a, b| b.cmp(a));
        eprintln!(
            "frame {}: {} mesh objects over {} distinct meshes; largest (vertices, triangles): {:?}",
            self.frames,
            objects.len(),
            shown.len(),
            &sizes[..sizes.len().min(3)]
        );
    }
}

#[cfg(test)]
#[path = "player_meshes_tests.rs"]
mod tests;
