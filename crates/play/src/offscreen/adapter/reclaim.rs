//! Making room in the runtime's storage budget: what the snapshot being adapted does not name is
//! given back, least recently named first, before an allocation is refused.
//!
//! Three kinds of storage can go: stored meshes and textures, which the adapter reads again when a
//! later snapshot names them, and render textures, which the state makes again, empty, when a
//! later frame names them. A snapshot names a render texture when a record reads or draws into it
//! or a property of a material, a renderer or the globals holds it. Anything the snapshot names
//! stays, so the room is never taken from the work being mapped.

use std::collections::BTreeSet;

use worldrunner_render_program::error::ProgramError;
use worldrunner_render_program::ids::TextureId;
use worldrunner_render_program::runtime::ProgramRuntime;
use worldrunner_render_program::unity::offscreen::frame::OffscreenFrame;
use worldrunner_render_program::unity::offscreen::{OffscreenError, OffscreenState};
use worldrunner_world_simulation::graphics::{
    GraphicsSnapshot, RenderTextureId, RendererView, ShaderProperties, TextureRef,
};

use super::super::frame::{Reclaimed, Report};
use super::super::ids;
use super::super::script::temporary_ids;
use super::{OffscreenAdapter, Stored};

/// The bytes by which the budget refused an allocation, when that is why `error` is.
fn shortfall(error: &OffscreenError) -> Option<u64> {
    match error {
        OffscreenError::Program(ProgramError::OverBudget { required, budget }) => {
            Some(required.saturating_sub(*budget).max(1))
        }
        _ => None,
    }
}

/// How giving storage back gets the GPU to finish with it. Released storage counts against the
/// budget until the last submission that used it has completed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Settling {
    /// The caller submits and waits for its own work, so reclaiming waits for every submission so
    /// far and the bytes it gave back are free when it returns.
    Wait,
    /// The viewer submits the work in a frame of its own, so reclaiming collects only what the
    /// GPU has already finished and never waits for the viewer's submission. The bytes of work
    /// still in flight stay counted, and an allocation they leave without room is refused as
    /// over budget until a later frame collects them.
    Poll,
}

impl Settling {
    pub(in crate::offscreen) fn poll_type(self) -> wgpu::PollType {
        match self {
            Self::Wait => wgpu::PollType::wait_indefinitely(),
            Self::Poll => wgpu::PollType::Poll,
        }
    }

    /// Frees the released storage the GPU has finished with, after waiting for it under
    /// [`Wait`](Self::Wait).
    fn collect(self, runtime: &ProgramRuntime) {
        // A poll that fails leaves the completed count where it was, which frees less.
        let _ = runtime.device().poll(self.poll_type());
        runtime.collect();
    }
}

/// What can be given back. The order is the order among equally old holdings: meshes, then
/// uploaded textures, then render textures, whose contents cost the most to make again.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Holding {
    Mesh(u64),
    Texture(u64),
    Target(TextureId),
}

/// The render textures the snapshot names, and the temporary ones its commands allocate.
pub(super) fn named_targets(snapshot: &GraphicsSnapshot) -> BTreeSet<TextureId> {
    let mut named = temporary_ids(&snapshot.work);
    let held_by_property = |id: RenderTextureId| {
        let holds = |properties: &ShaderProperties| {
            properties
                .textures
                .iter()
                .any(|(_, slot)| slot.texture == TextureRef::Render(id))
        };
        snapshot
            .materials
            .values()
            .any(|material| holds(&material.properties))
            || snapshot
                .renderers
                .iter()
                .flat_map(RendererView::blocks)
                .any(holds)
            || snapshot
                .global
                .iter()
                .any(|global| holds(&global.properties))
    };
    for &id in snapshot.render_textures.keys() {
        let by_record = snapshot
            .work
            .iter()
            .any(|work| work.names_render_texture(id));
        if by_record || held_by_property(id) {
            named.insert(ids::render_texture(id));
        }
    }
    named
}

impl OffscreenAdapter {
    /// Notes that the snapshot names the meshes and textures given. A mesh that holds no storage
    /// and that the snapshot does not name is forgotten: the engine gives it again when it is
    /// named, and nothing is lost by reading it again.
    pub(super) fn stamp(&mut self, meshes: &BTreeSet<u64>, textures: &BTreeSet<u64>) {
        for (named, held) in [(meshes, &mut self.meshes), (textures, &mut self.textures)] {
            for identity in named {
                if let Some(stored) = held.get_mut(identity) {
                    stored.last_named = self.epoch;
                }
            }
        }
        self.meshes
            .retain(|_, held| held.usable || held.last_named == self.epoch);
    }

    /// Notes that the UI read the render texture `id` since the last snapshot, as a frame naming it
    /// would: of the textures nothing names, the one read last is given back last.
    pub(crate) fn note_ui_read(&mut self, id: TextureId) {
        if let Some(named) = self.targets.get_mut(&id) {
            *named = self.epoch;
        }
    }

    /// Notes that the snapshot names the render textures given, and forgets the ones that no
    /// longer exist.
    pub(super) fn stamp_targets(
        &mut self,
        named: impl IntoIterator<Item = TextureId>,
        live: &BTreeSet<TextureId>,
    ) {
        self.targets.retain(|id, _| live.contains(id));
        for id in named {
            self.targets.insert(id, self.epoch);
        }
    }

    /// Runs `upload`, and when the budget refuses it gives back what the snapshot does not name,
    /// least recently named first, as far as the refusal asks, and runs it once more. The second
    /// run happens when that gave back storage or when the budget holds fewer bytes than before,
    /// which is the case when the GPU finished work between the refusal and the collection, and
    /// the refusal then no longer describes the budget.
    pub(super) fn with_room<T>(
        &mut self,
        state: &mut OffscreenState,
        runtime: &mut ProgramRuntime,
        notes: &mut Report,
        settling: Settling,
        mut upload: impl FnMut(&mut OffscreenState, &mut ProgramRuntime) -> Result<T, OffscreenError>,
    ) -> Result<T, OffscreenError> {
        let error = match upload(state, runtime) {
            Ok(done) => return Ok(done),
            Err(error) => error,
        };
        let Some(short) = shortfall(&error) else {
            return Err(error);
        };
        let held = runtime.live_bytes();
        let reclaimed = self.reclaim(state, runtime, short, settling);
        notes.reclaimed.add(reclaimed);
        if reclaimed.bytes == 0 && runtime.live_bytes() >= held {
            return Err(error);
        }
        upload(state, runtime)
    }

    /// Gives back what the last snapshot did not name until the render textures of `frame` fit in
    /// the budget, or nothing more can go. `settling` says how the GPU is got to finish with what
    /// is given back: wait for it, or take only what is already done.
    pub(crate) fn make_room(
        &mut self,
        state: &mut OffscreenState,
        runtime: &mut ProgramRuntime,
        frame: &OffscreenFrame,
        settling: Settling,
    ) -> Reclaimed {
        runtime.collect();
        let needed = state.new_render_texture_bytes(runtime, &frame.render_textures);
        self.make_room_for(state, runtime, needed, settling)
    }

    /// Gives back what the last snapshot did not name until `needed` bytes are free in the budget,
    /// or nothing more can go, with `settling` as for [`make_room`](Self::make_room). Call it while
    /// no frame is being recorded.
    pub(crate) fn make_room_for(
        &mut self,
        state: &mut OffscreenState,
        runtime: &mut ProgramRuntime,
        needed: u64,
        settling: Settling,
    ) -> Reclaimed {
        runtime.collect();
        let free = runtime.budget().0.saturating_sub(runtime.live_bytes());
        if needed <= free {
            return Reclaimed::default();
        }
        self.reclaim(state, runtime, needed - free, settling)
    }

    /// Releases stored meshes and textures and render textures that the current snapshot does not
    /// name, least recently named first, until `shortfall` bytes are given back or none are left.
    /// The storage is counted in the budget until the GPU is done with it, which `settling` says
    /// how to get: wait for it, or take only what is already done.
    fn reclaim(
        &mut self,
        state: &mut OffscreenState,
        runtime: &mut ProgramRuntime,
        shortfall: u64,
        settling: Settling,
    ) -> Reclaimed {
        let unnamed = |held: &std::collections::BTreeMap<u64, Stored>, texture: bool| {
            held.iter()
                .filter(|(_, stored)| stored.usable && stored.last_named < self.epoch)
                .map(|(identity, stored)| {
                    let holding = if texture {
                        Holding::Texture(*identity)
                    } else {
                        Holding::Mesh(*identity)
                    };
                    (stored.last_named, holding)
                })
                .collect::<Vec<_>>()
        };
        let mut candidates = unnamed(&self.textures, true);
        candidates.extend(unnamed(&self.meshes, false));
        candidates.extend(
            state
                .render_textures_held()
                .into_iter()
                .filter_map(|(id, _)| {
                    let named = self.targets.get(&id).copied().unwrap_or(0);
                    (named < self.epoch).then_some((named, Holding::Target(id)))
                }),
        );
        candidates.sort_unstable();
        let mut reclaimed = Reclaimed::default();
        for (_, holding) in candidates {
            if reclaimed.bytes >= shortfall {
                break;
            }
            let freed = match holding {
                Holding::Texture(identity) => ids::stored_texture(identity)
                    .and_then(|id| state.release_source_texture(runtime, id)),
                Holding::Mesh(identity) => state.release_mesh(runtime, ids::mesh(identity)),
                Holding::Target(id) => state.release_render_texture(runtime, id),
            };
            let Some(bytes) = freed else { continue };
            reclaimed.bytes += bytes;
            match holding {
                Holding::Texture(identity) => {
                    self.textures.remove(&identity);
                    reclaimed.textures += 1;
                }
                Holding::Mesh(identity) => {
                    self.meshes.remove(&identity);
                    self.bounds.remove(&identity);
                    self.sub_meshes.remove(&identity);
                    reclaimed.meshes += 1;
                }
                Holding::Target(id) => {
                    self.targets.remove(&id);
                    reclaimed.targets += 1;
                }
            }
        }
        settling.collect(runtime);
        reclaimed
    }
}
