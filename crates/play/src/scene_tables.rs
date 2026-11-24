//! The retained viewer's mesh, texture, material and object tables for a running player.
//!
//! A frame lists the mesh renderers it shows. The tables keep one scene handle for each mesh,
//! texture and material the frame has used, never reusing or moving a handle, and the objects
//! the viewer holds, so each frame becomes the smallest edit that turns the viewer's scene into
//! the frame's: an append of the meshes, textures and materials that are new, a replacement by
//! handle of each mesh or texture whose contents changed, and one batch of object additions,
//! replacements and removals. None of these copies or validates what the viewer already holds,
//! so a stream of new meshes costs what the new meshes weigh. The viewer applies the edits in
//! order, so an object may use a mesh appended in the same frame.
//!
//! A mesh renderer with several material slots draws its whole mesh with slot 0, because the
//! viewer's object holds one material. The slot's texture picks the material and the slot's
//! colour tints the object, so two objects sharing a texture share a material however they are
//! coloured. Alpha is not drawn, and the other slots are counted in the census as not drawn.

use std::collections::{BTreeMap, BTreeSet};

use worldrunner_disunity_runtime::MeshObjectView;
use worldrunner_render_plan::{
    MaterialHandle, MeshHandle, RenderScene, SceneAlphaMode, SceneAppend, SceneEffects,
    SceneGeometry, SceneMaterial, SceneMaterialParts, SceneMesh, SceneObject, SceneParts,
    SceneResources, SceneTexture, SceneTint, SceneValidationError, TexelFormat, TextureColorSpace,
    TextureHandle,
};
use worldrunner_scene_viewer::{ObjectId, RetainedUpdate, SceneObjectEdit};

use crate::{
    player_meshes::round_to_i64,
    scene_census::{FrameCensus, LeftOut, SubstitutionCensus, WhiteReason},
    scene_transform::scene_transform,
};

const MICRO: f64 = 1_000_000.0;
/// The handle of the 1x1 white texture every untextured slot draws with, and of the material
/// built on it.
const WHITE: u32 = 0;
const WHITE_TINT: u32 = 1_000_000;

/// Where the tables read the contents of the meshes and textures a frame names.
pub trait FrameContent {
    fn mesh(&self, identity: u64) -> Option<&SceneMesh>;
    fn texture(&self, identity: u64) -> Option<&SceneTexture>;

    /// Why [`FrameContent::texture`] has no texture for `identity`: the texels were never stored
    /// or their conversion failed.
    fn missing_texture(&self, _identity: u64) -> WhiteReason {
        WhiteReason::TexelsNotStored
    }
}

/// The identities whose contents changed since the last frame.
#[derive(Clone, Debug, Default)]
pub struct Changes {
    pub meshes: BTreeSet<u64>,
    pub textures: BTreeSet<u64>,
}

/// An object the frame lists that the viewer does not show, and why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Skipped {
    pub object: u64,
    pub reason: String,
}

/// What turns the viewer's scene into the frame's.
#[derive(Debug, Default)]
pub struct FrameUpdate {
    /// The meshes, textures and materials the frame is the first to use. They take the next
    /// handles in the viewer's tables.
    pub append: Option<SceneAppend>,
    /// The textures the viewer holds whose contents changed, by handle.
    pub replaced_textures: Vec<(TextureHandle, SceneTexture)>,
    /// The meshes the viewer holds whose contents changed, by handle.
    pub replaced_meshes: Vec<(MeshHandle, SceneMesh)>,
    pub edits: Vec<SceneObjectEdit>,
    pub skipped: Vec<Skipped>,
}

impl FrameUpdate {
    /// The viewer updates that apply this frame, in the order that keeps every handle valid:
    /// new resources, changed resources, then objects.
    #[must_use]
    pub fn into_updates(self) -> Vec<RetainedUpdate> {
        let mut updates = Vec::new();
        updates.extend(self.append.map(RetainedUpdate::AppendResources));
        updates.extend(
            self.replaced_textures
                .into_iter()
                .map(|(handle, texture)| RetainedUpdate::ReplaceTexture { handle, texture }),
        );
        updates.extend(
            self.replaced_meshes
                .into_iter()
                .map(|(handle, mesh)| RetainedUpdate::ReplaceMesh { handle, mesh }),
        );
        if !self.edits.is_empty() {
            updates.push(RetainedUpdate::Objects(self.edits));
        }
        updates
    }
}

pub struct SceneTables {
    /// The scene handle of each player texture, mesh and material the viewer holds or is about
    /// to be sent. The white texture and material are handle 0 and are in no map.
    texture_index: BTreeMap<u64, u32>,
    /// The material built on each texture handle.
    material_index: BTreeMap<u32, u32>,
    mesh_index: BTreeMap<u64, u32>,
    /// The sizes of the viewer's tables once the pending items are sent.
    textures: u32,
    materials: u32,
    meshes: u32,
    /// The items the current update appends.
    pending: SceneAppend,
    /// What the viewer shows for each object identity.
    placed: BTreeMap<u64, (ObjectId, SceneObject)>,
    next_id: u64,
    /// Meshes and textures the viewer holds whose contents changed since the last send.
    stale: Changes,
    census: SubstitutionCensus,
}

fn white_texture() -> SceneTexture {
    SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Srgb,
        width: 1,
        height: 1,
        mips: 1,
        bytes: vec![255; 4].into(),
    }
}

fn material_on(texture: u32) -> Result<SceneMaterial, SceneValidationError> {
    SceneMaterial::new(SceneMaterialParts {
        albedo: TextureHandle::from_raw(texture),
        normal: None,
        roughness_metallic: None,
        ambient_occlusion: None,
        self_illumination: None,
        roughness_micro: 900_000,
        metalness_micro: 0,
        alpha_mode: SceneAlphaMode::Opaque,
        double_sided: true,
    })
}

/// A colour channel as a tint multiplier in millionths, within the tint's range.
fn tint_channel(channel: f32) -> u32 {
    if channel.is_nan() {
        return 0;
    }
    let micro = round_to_i64(f64::from(channel).clamp(0.0, 4.0) * MICRO).unwrap_or(0);
    u32::try_from(micro).unwrap_or(0)
}

impl SceneTables {
    /// Tables holding the white texture, the white material and nothing else.
    pub fn new() -> Result<Self, SceneValidationError> {
        material_on(WHITE)?;
        Ok(Self {
            texture_index: BTreeMap::new(),
            material_index: BTreeMap::from([(WHITE, WHITE)]),
            mesh_index: BTreeMap::new(),
            textures: 1,
            materials: 1,
            meshes: 0,
            pending: SceneAppend::default(),
            placed: BTreeMap::new(),
            next_id: 0,
            stale: Changes::default(),
            census: SubstitutionCensus::default(),
        })
    }

    /// What the last frame drew with the white texture and left out, and the worst frame so far.
    #[must_use]
    pub fn census(&self) -> &SubstitutionCensus {
        &self.census
    }

    /// The smallest scene the viewer accepts: the white texture and material, no meshes and no
    /// objects, lit by `effects` when the first frame has a sun.
    pub fn initial_scene(
        effects: Option<SceneEffects>,
    ) -> Result<RenderScene, SceneValidationError> {
        RenderScene::new(SceneParts {
            resources: SceneResources::new(vec![white_texture()], vec![material_on(WHITE)?])?,
            geometry: SceneGeometry::new(Vec::new(), Vec::new())?,
            terrain: None,
            effects,
        })
    }

    /// The edits that turn the viewer's scene into the one `objects` list, reading the contents
    /// of new or changed meshes and textures from `content`.
    ///
    /// Every resource the objects need goes to the viewer in the same update, ahead of the
    /// object edits, so no object waits. An object whose mesh has no converted content yet is
    /// left out until the content arrives.
    pub fn update(
        &mut self,
        objects: &[MeshObjectView],
        content: &impl FrameContent,
        changes: &Changes,
    ) -> FrameUpdate {
        self.note_changes(changes);
        let mut resolved = BTreeMap::new();
        let mut skipped = Vec::new();
        let mut census = FrameCensus::default();
        for view in objects {
            match self.place(view, content, &mut census) {
                Ok(Some(object)) => {
                    resolved.insert(view.object, object);
                }
                Ok(None) => {}
                Err(reason) => skipped.push(Skipped {
                    object: view.object,
                    reason,
                }),
            }
        }
        self.census.end_frame(census);
        let append = std::mem::take(&mut self.pending);
        FrameUpdate {
            append: (!append.is_empty()).then_some(append),
            replaced_textures: self.replaced_textures(content),
            replaced_meshes: self.replaced_meshes(content),
            edits: self.diff(&resolved),
            skipped,
        }
    }

    /// Records which of the meshes and textures the viewer holds have new contents.
    fn note_changes(&mut self, changes: &Changes) {
        self.stale.meshes.extend(
            changes
                .meshes
                .iter()
                .filter(|identity| self.mesh_index.contains_key(identity)),
        );
        self.stale.textures.extend(
            changes
                .textures
                .iter()
                .filter(|identity| self.texture_index.contains_key(identity)),
        );
    }

    /// The latest contents of the changed textures, each under its handle.
    fn replaced_textures(
        &mut self,
        content: &impl FrameContent,
    ) -> Vec<(TextureHandle, SceneTexture)> {
        std::mem::take(&mut self.stale.textures)
            .into_iter()
            .filter_map(|identity| {
                let index = *self.texture_index.get(&identity)?;
                let texture = content.texture(identity)?.clone();
                Some((TextureHandle::from_raw(index), texture))
            })
            .collect()
    }

    /// The latest contents of the changed meshes, each under its handle.
    fn replaced_meshes(&mut self, content: &impl FrameContent) -> Vec<(MeshHandle, SceneMesh)> {
        std::mem::take(&mut self.stale.meshes)
            .into_iter()
            .filter_map(|identity| {
                let index = *self.mesh_index.get(&identity)?;
                let mesh = content.mesh(identity)?.clone();
                Some((MeshHandle::from_raw(index), mesh))
            })
            .collect()
    }

    /// The scene object a frame's renderer becomes. `None` while its mesh has no scene mesh yet.
    /// `census` counts the renderer as drawn, with the reason it is white, or as left out.
    fn place(
        &mut self,
        view: &MeshObjectView,
        content: &impl FrameContent,
        census: &mut FrameCensus,
    ) -> Result<Option<SceneObject>, String> {
        let transform = scene_transform(&view.local_to_world).map_err(|error| error.to_string())?;
        let mesh = match self.mesh_handle(view.mesh, content) {
            Ok(mesh) => mesh,
            Err(reason) => {
                census.left_out(reason);
                return Ok(None);
            }
        };
        let slot = view.slots.first();
        let (texture, white) = match slot {
            Some(slot) => self.texture_handle(slot.texture, content),
            None => (WHITE, Some(WhiteReason::NoMaterialSlot)),
        };
        let material = self
            .material_handle(texture)
            .map_err(|error| error.to_string())?;
        let tint = slot.map_or([WHITE_TINT; 3], |slot| {
            [slot.color[0], slot.color[1], slot.color[2]].map(tint_channel)
        });
        let tint = SceneTint::new(tint).map_err(|error| error.to_string())?;
        census.drawn(white);
        census.slots_not_drawn(view.slots.len().saturating_sub(1));
        Ok(Some(
            SceneObject::new(
                transform,
                MeshHandle::from_raw(mesh),
                MaterialHandle::from_raw(material),
            )
            .with_tint(tint),
        ))
    }

    /// The mesh handle of player mesh `identity`, or why the renderer cannot be shown.
    fn mesh_handle(&mut self, identity: u64, content: &impl FrameContent) -> Result<u32, LeftOut> {
        if let Some(index) = self.mesh_index.get(&identity) {
            return Ok(*index);
        }
        let mesh = content
            .mesh(identity)
            .ok_or(LeftOut::MeshNotArrived)?
            .clone();
        let index = self.meshes;
        self.meshes = index.checked_add(1).ok_or(LeftOut::MeshHandleOverflow)?;
        self.pending.meshes.push(mesh);
        self.mesh_index.insert(identity, index);
        Ok(index)
    }

    /// The texture handle of player texture `identity`, and the reason it is the white texture
    /// when it is: white for no texture and for one whose texels have not arrived.
    fn texture_handle(
        &mut self,
        identity: u64,
        content: &impl FrameContent,
    ) -> (u32, Option<WhiteReason>) {
        if identity == 0 {
            return (WHITE, Some(WhiteReason::TextureZero));
        }
        if let Some(index) = self.texture_index.get(&identity) {
            return (*index, None);
        }
        let Some(texture) = content.texture(identity).cloned() else {
            return (WHITE, Some(content.missing_texture(identity)));
        };
        let index = self.textures;
        let Some(next) = index.checked_add(1) else {
            return (WHITE, Some(WhiteReason::HandleOverflow));
        };
        self.textures = next;
        self.pending.textures.push(texture);
        self.texture_index.insert(identity, index);
        (index, None)
    }

    fn material_handle(&mut self, texture: u32) -> Result<u32, SceneValidationError> {
        if let Some(index) = self.material_index.get(&texture) {
            return Ok(*index);
        }
        let material = material_on(texture)?;
        let index = self.materials;
        self.materials = index
            .checked_add(1)
            .ok_or(SceneValidationError::ArithmeticOverflow {
                part: "material table",
            })?;
        self.pending.materials.push(material);
        self.material_index.insert(texture, index);
        Ok(index)
    }

    /// The additions, replacements and removals that turn the placed objects into `wanted`, which
    /// then become the placed objects.
    fn diff(&mut self, wanted: &BTreeMap<u64, SceneObject>) -> Vec<SceneObjectEdit> {
        let mut edits = Vec::new();
        self.placed.retain(|identity, (id, _)| {
            let kept = wanted.contains_key(identity);
            if !kept {
                edits.push(SceneObjectEdit::Remove(*id));
            }
            kept
        });
        for (identity, object) in wanted {
            match self.placed.get_mut(identity) {
                None => {
                    let id = ObjectId(self.next_id);
                    self.next_id += 1;
                    self.placed.insert(*identity, (id, *object));
                    edits.push(SceneObjectEdit::Add {
                        id,
                        object: *object,
                    });
                }
                Some((id, held)) if held != object => {
                    *held = *object;
                    edits.push(SceneObjectEdit::Replace {
                        id: *id,
                        object: *object,
                    });
                }
                Some(_) => {}
            }
        }
        edits
    }
}

#[cfg(test)]
#[path = "scene_tables_tests.rs"]
mod tests;
