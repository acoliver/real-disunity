//! Registering the meshes a snapshot names: reading what the engine stored a new generation of,
//! uploading it, and keeping the adapter's record of it in step with what the state holds.

use std::collections::BTreeSet;

use worldrunner_disunity_runtime::MeshChannels;
use worldrunner_render_program::runtime::ProgramRuntime;
use worldrunner_render_program::unity::offscreen::{OffscreenError, OffscreenState};

use super::super::assets::{Assets, MeshError, mesh_of};
use super::super::bounds::Bounds;
use super::super::frame::Report;
use super::super::ids;
use super::{OffscreenAdapter, Stored};

/// What uploading a mesh the engine stored came to.
enum MeshUpload {
    /// It is in the state, with the box of its vertices and the index count of each sub-mesh.
    Held {
        bounds: Option<Bounds>,
        sub_meshes: Vec<u32>,
    },
    /// It has no vertices or no sub-mesh with an index, which is not a failure.
    NothingToDraw,
    /// It is wrong, and stays wrong until the engine stores a new generation of it.
    Malformed,
    /// The device or the budget refused it. Nothing says it would be refused again.
    Refused,
}

impl OffscreenAdapter {
    /// Reads, uploads and records each mesh of `identities` the engine has a new generation of.
    pub(super) fn register_meshes(
        &mut self,
        identities: BTreeSet<u64>,
        assets: &dyn Assets,
        (state, runtime): (&mut OffscreenState, &mut ProgramRuntime),
        notes: &mut Report,
    ) {
        for identity in identities {
            let known = self.meshes.get(&identity).map_or(0, |held| held.generation);
            let stored = match assets.mesh(identity, known) {
                Ok(Some(stored)) => stored,
                Ok(None) => continue,
                Err(_) => {
                    notes.count("mesh:Read");
                    continue;
                }
            };
            let upload = self.upload_mesh(identity, &stored, (state, runtime), notes);
            self.note_mesh(identity, stored.generation, upload, (state, runtime));
        }
    }

    /// Uploads a mesh the engine stored a new generation of, in place of the one held.
    fn upload_mesh(
        &mut self,
        identity: u64,
        stored: &MeshChannels,
        (state, runtime): (&mut OffscreenState, &mut ProgramRuntime),
        notes: &mut Report,
    ) -> MeshUpload {
        let mesh = match mesh_of(stored) {
            Ok(mesh) => mesh,
            Err(MeshError::Empty(empty)) => {
                // Nothing to draw is not a failure. The mesh stays unusable, so renderers of it
                // are skipped and a mesh draw of it is refused as empty.
                *notes
                    .census
                    .entry(format!("mesh with nothing to draw: {empty}"))
                    .or_default() += 1;
                return MeshUpload::NothingToDraw;
            }
            Err(error) => {
                notes.fail("mesh", &error);
                return MeshUpload::Malformed;
            }
        };
        let uploaded = self.with_room(state, runtime, notes, |state, runtime| {
            state.set_mesh(runtime, ids::mesh(identity), &mesh)
        });
        match uploaded {
            Ok(()) => MeshUpload::Held {
                bounds: Bounds::of_mesh(&mesh),
                sub_meshes: mesh.sub_meshes.iter().map(|sub| sub.count).collect(),
            },
            // The state found the mesh itself wrong; the engine's mesh is the same until its
            // generation rises.
            Err(error @ OffscreenError::BadMesh { .. }) => {
                notes.fail("mesh", &error);
                MeshUpload::Malformed
            }
            // The device or the budget refused it, which a later snapshot may find otherwise.
            Err(error) => {
                notes.fail("mesh", &error);
                MeshUpload::Refused
            }
        }
    }

    /// Keeps what an upload came to. The state holds the mesh exactly when the adapter's entry is
    /// usable: any other outcome gives back the storage of the mesh that was held and its
    /// metadata, so a replacement that failed leaves no old geometry behind.
    fn note_mesh(
        &mut self,
        identity: u64,
        generation: u64,
        upload: MeshUpload,
        (state, runtime): (&mut OffscreenState, &mut ProgramRuntime),
    ) {
        let (usable, empty) = match upload {
            MeshUpload::Held { bounds, sub_meshes } => {
                match bounds {
                    Some(bounds) => self.bounds.insert(identity, bounds),
                    None => self.bounds.remove(&identity),
                };
                self.sub_meshes.insert(identity, sub_meshes);
                (true, false)
            }
            MeshUpload::NothingToDraw | MeshUpload::Malformed => {
                self.release_mesh(identity, state, runtime);
                (false, matches!(upload, MeshUpload::NothingToDraw))
            }
            MeshUpload::Refused => {
                // Without the generation, the engine gives the whole mesh again next time.
                self.release_mesh(identity, state, runtime);
                self.meshes.remove(&identity);
                return;
            }
        };
        self.meshes.insert(
            identity,
            Stored {
                generation,
                sampler_generation: 0,
                usable,
                empty,
                last_named: self.epoch,
            },
        );
    }

    /// Gives back the storage the state holds for mesh `identity`, and the metadata kept for it.
    fn release_mesh(
        &mut self,
        identity: u64,
        state: &mut OffscreenState,
        runtime: &mut ProgramRuntime,
    ) {
        state.release_mesh(runtime, ids::mesh(identity));
        self.bounds.remove(&identity);
        self.sub_meshes.remove(&identity);
    }
}
