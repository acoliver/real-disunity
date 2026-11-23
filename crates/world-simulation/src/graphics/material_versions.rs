//! Materials as the frame's draws saw them.
//!
//! A script sets values on a material, draws with it, sets others and draws again. The world holds
//! one state per material, and a host replays the frame from a snapshot taken at the flush, so
//! every draw would read the material as the script's last edit left it. A draw record therefore
//! names the material by its own id only while the material still holds the state the draw saw.
//! The first edit of the material after such draws copies the state they saw, a version, and the
//! draws name the version from then on. A version is a [`MaterialId`] of generation 0, which no
//! slot of the world ever has, numbered over the world's life so a version's id is never reused.
//!
//! A frame whose draws saw one state of each material, which is almost every frame, makes no copy
//! and names its materials as before. A material edited several times between two draws is copied
//! once, since the second edit finds no draw to protect.
use std::collections::BTreeMap;

use super::{MaterialId, OffscreenWork, WorldMaterial};
use crate::{LimitKind, World, WorldError};

/// A state of a material that draws named before the script changed it.
#[derive(Clone, Debug)]
struct Version {
    of: MaterialId,
    state: WorldMaterial,
}

/// The materials the frame's draws name.
#[derive(Clone, Debug)]
pub(super) struct MaterialVersions {
    /// The draws that name each material by its own id, as indices into the frame's work list.
    live: BTreeMap<MaterialId, Vec<usize>>,
    copies: BTreeMap<MaterialId, Version>,
    /// How many versions the world made in its life.
    made: u32,
}

impl MaterialVersions {
    pub(super) const fn new() -> Self {
        Self {
            live: BTreeMap::new(),
            copies: BTreeMap::new(),
            made: 0,
        }
    }

    /// Forgets the frame's draws and copies; the count of versions made stays.
    pub(super) fn clear(&mut self) {
        self.live.clear();
        self.copies.clear();
    }
}

/// The material a draw record names.
fn material_slot(work: &mut OffscreenWork) -> Option<&mut MaterialId> {
    match work {
        OffscreenWork::Blit {
            material: Some(material),
            ..
        }
        | OffscreenWork::DrawMesh { material, .. }
        | OffscreenWork::DrawMeshBatch { material, .. }
        | OffscreenWork::DrawProcedural { material, .. } => Some(material),
        _ => None,
    }
}

impl World {
    /// Notes that the record about to be appended draws with a material, by the material's id.
    pub(super) fn note_material_draw(&mut self, work: &mut OffscreenWork) {
        if let Some(material) = material_slot(work).map(|slot| *slot) {
            let index = self.graphics.offscreen.len();
            self.graphics
                .versions
                .live
                .entry(material)
                .or_default()
                .push(index);
        }
    }

    /// Gives the frame's draws that name the material by its own id a copy of the state they saw,
    /// before an edit changes it. A rejected edit leaves them naming a copy that equals the
    /// material. Draws of a destroyed material keep its stale id.
    pub(super) fn freeze_material_draws(&mut self, id: MaterialId) -> Result<(), WorldError> {
        if !self.graphics.versions.live.contains_key(&id) {
            return Ok(());
        }
        let Some(state) = self.graphics.materials.get(id.0).cloned() else {
            return Ok(());
        };
        let versions = &mut self.graphics.versions;
        let made = versions
            .made
            .checked_add(1)
            .ok_or(WorldError::CapacityExceeded(LimitKind::Graphics))?;
        versions.made = made;
        let version = MaterialId::from_bits(u64::from(made));
        let draws = versions.live.remove(&id).unwrap_or_default();
        versions.copies.insert(version, Version { of: id, state });
        for index in draws {
            if let Some(slot) = self
                .graphics
                .offscreen
                .get_mut(index)
                .and_then(material_slot)
            {
                *slot = version;
            }
        }
        Ok(())
    }

    /// Ends the frame's versions. The draws of a version whose material was destroyed name the
    /// destroyed material again, so a draw reads as stale whether or not the script edited the
    /// material before destroying it. Returns the versions the records still name, with their
    /// states.
    pub(super) fn settle_material_versions(
        &mut self,
        work: &mut [OffscreenWork],
    ) -> BTreeMap<MaterialId, WorldMaterial> {
        self.graphics.versions.live.clear();
        let copies = std::mem::take(&mut self.graphics.versions.copies);
        let mut kept = BTreeMap::new();
        let mut destroyed = BTreeMap::new();
        for (version, Version { of, state }) in copies {
            if self.graphics.materials.get(of.0).is_some() {
                kept.insert(version, state);
            } else {
                destroyed.insert(version, of);
            }
        }
        if !destroyed.is_empty() {
            for slot in work.iter_mut().filter_map(material_slot) {
                if let Some(material) = destroyed.get(slot) {
                    *slot = *material;
                }
            }
        }
        kept
    }
}
