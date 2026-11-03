//! The retained viewer's mesh, texture, material and object tables for a running player.
//!
//! A frame lists the mesh renderers it shows. The tables keep one scene handle for each mesh,
//! texture and material the frame has used, never reusing or moving a handle, and the objects
//! the viewer holds, so each frame becomes the smallest edit that turns the viewer's scene into
//! the frame's: one replacement of the resource tables when a mesh, texture or material is new
//! or changed and the interval since the last replacement has passed, and one batch of object
//! additions, replacements and removals. Replacing the tables clones and validates all of them,
//! so a stream of new meshes would cost that on every frame; the objects that need a resource
//! the viewer lacks wait for the next replacement.
//!
//! A mesh renderer with several material slots draws its whole mesh with slot 0, as the
//! engine's choice for now. The slot's texture picks the material and the slot's colour tints
//! the object, so two objects sharing a texture share a material however they are coloured.
//! Alpha and the other slots are not drawn.

use std::collections::{BTreeMap, BTreeSet};

use worldrunner_disunity_runtime::MeshObjectView;
use worldrunner_render_plan::{
    MaterialHandle, MeshHandle, RenderScene, SceneAlphaMode, SceneGeometry, SceneMaterial,
    SceneMaterialParts, SceneMesh, SceneObject, SceneParts, SceneResources, SceneTexture,
    SceneTint, SceneValidationError, TexelFormat, TextureColorSpace, TextureHandle,
};
use worldrunner_scene_viewer::{ObjectId, SceneObjectEdit};

use crate::{player_meshes::round_to_i64, scene_transform::scene_transform};

const MICRO: f64 = 1_000_000.0;
/// The handle of the 1x1 white texture every untextured slot draws with, and of the material
/// built on it.
const WHITE: u32 = 0;
const WHITE_TINT: u32 = 1_000_000;
/// The frames between two replacements of the viewer's resource tables.
pub const RESOURCE_INTERVAL: u64 = 8;

/// Where the tables read the contents of the meshes and textures a frame names.
pub trait FrameContent {
    fn mesh(&self, identity: u64) -> Option<&SceneMesh>;
    fn texture(&self, identity: u64) -> Option<&SceneTexture>;
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
#[derive(Debug)]
pub struct FrameUpdate {
    /// The full texture, material and mesh tables, when the interval allows a send and any of
    /// them gained or changed an entry since the last one.
    pub resources: Option<(SceneResources, Vec<SceneMesh>)>,
    pub edits: Vec<SceneObjectEdit>,
    pub skipped: Vec<Skipped>,
}

pub struct SceneTables {
    textures: Vec<SceneTexture>,
    texture_index: BTreeMap<u64, u32>,
    materials: Vec<SceneMaterial>,
    /// The material built on each texture handle.
    material_index: BTreeMap<u32, u32>,
    meshes: Vec<SceneMesh>,
    mesh_index: BTreeMap<u64, u32>,
    /// What the viewer shows for each object identity.
    placed: BTreeMap<u64, (ObjectId, SceneObject)>,
    next_id: u64,
    /// How many materials and meshes the viewer holds. Handles at or past these are in the
    /// tables only. A new texture always comes with a new material, so no texture count is kept.
    sent: SentCounts,
    /// Meshes and textures the viewer holds whose contents changed since the last send.
    stale: Changes,
    /// Whether any resource table has gone to the viewer yet.
    sent_any: bool,
    /// The updates since the last send.
    frames_since_send: u64,
}

#[derive(Clone, Copy, Debug)]
struct SentCounts {
    materials: usize,
    meshes: usize,
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
        Ok(Self {
            textures: vec![white_texture()],
            texture_index: BTreeMap::new(),
            materials: vec![material_on(WHITE)?],
            material_index: BTreeMap::from([(WHITE, WHITE)]),
            meshes: Vec::new(),
            mesh_index: BTreeMap::new(),
            placed: BTreeMap::new(),
            next_id: 0,
            sent: SentCounts {
                materials: 1,
                meshes: 0,
            },
            stale: Changes::default(),
            sent_any: false,
            frames_since_send: 0,
        })
    }

    /// The smallest scene the viewer accepts: the white texture and material, no meshes and no
    /// objects.
    pub fn initial_scene(&self) -> Result<RenderScene, SceneValidationError> {
        RenderScene::new(SceneParts {
            resources: SceneResources::new(self.textures.clone(), self.materials.clone())?,
            geometry: SceneGeometry::new(Vec::new(), Vec::new())?,
            terrain: None,
            effects: None,
        })
    }

    /// The edits that turn the viewer's scene into the one `objects` list, reading the contents
    /// of new or changed meshes and textures from `content`.
    ///
    /// New and changed resources go to the viewer on the first frame with any, and after that
    /// once [`RESOURCE_INTERVAL`] frames have passed since the last send. Until then an object
    /// that needs a resource the viewer lacks is held back: it is not added, and one already
    /// shown stays as it is. Objects that need nothing new are edited on every frame.
    pub fn update(
        &mut self,
        objects: &[MeshObjectView],
        content: &impl FrameContent,
        changes: &Changes,
    ) -> Result<FrameUpdate, SceneValidationError> {
        self.frames_since_send = self.frames_since_send.saturating_add(1);
        self.note_changes(changes);
        let mut resolved = Vec::new();
        let mut skipped = Vec::new();
        for view in objects {
            match self.place(view, content) {
                Ok(Some(object)) => resolved.push((view.object, object)),
                Ok(None) => {}
                Err(reason) => skipped.push(Skipped {
                    object: view.object,
                    reason,
                }),
            }
        }
        let resources = self.send_if_due(content)?;
        let wanted = self.shown(resolved);
        let edits = self.diff(&wanted);
        Ok(FrameUpdate {
            resources,
            edits,
            skipped,
        })
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

    fn unsent_changes(&self) -> bool {
        !self.stale.meshes.is_empty()
            || !self.stale.textures.is_empty()
            || self.materials.len() > self.sent.materials
            || self.meshes.len() > self.sent.meshes
    }

    /// The whole resource tables, when something is unsent and the interval allows a send.
    fn send_if_due(
        &mut self,
        content: &impl FrameContent,
    ) -> Result<Option<(SceneResources, Vec<SceneMesh>)>, SceneValidationError> {
        let waiting = self.sent_any && self.frames_since_send < RESOURCE_INTERVAL;
        if waiting || !self.unsent_changes() {
            return Ok(None);
        }
        self.take_stale_contents(content);
        let resources = SceneResources::new(self.textures.clone(), self.materials.clone())?;
        self.sent = SentCounts {
            materials: self.materials.len(),
            meshes: self.meshes.len(),
        };
        self.sent_any = true;
        self.frames_since_send = 0;
        Ok(Some((resources, self.meshes.clone())))
    }

    /// Puts the latest contents of the changed meshes and textures into their table entries.
    fn take_stale_contents(&mut self, content: &impl FrameContent) {
        for identity in std::mem::take(&mut self.stale.meshes) {
            if let (Some(index), Some(mesh)) =
                (self.mesh_index.get(&identity), content.mesh(identity))
            {
                self.meshes[*index as usize] = mesh.clone();
            }
        }
        for identity in std::mem::take(&mut self.stale.textures) {
            if let (Some(index), Some(texture)) =
                (self.texture_index.get(&identity), content.texture(identity))
            {
                self.textures[*index as usize] = texture.clone();
            }
        }
    }

    /// The objects the viewer shows after this frame: those whose mesh and material it holds, and
    /// the held-back ones it already shows as they are.
    fn shown(&self, resolved: Vec<(u64, SceneObject)>) -> BTreeMap<u64, SceneObject> {
        let mut wanted = BTreeMap::new();
        for (identity, object) in resolved {
            if self.viewer_holds(&object) {
                wanted.insert(identity, object);
            } else if let Some((_, held)) = self.placed.get(&identity) {
                wanted.insert(identity, *held);
            }
        }
        wanted
    }

    /// Whether the viewer has the mesh and material of `object`. A material is built only on
    /// textures already in the tables and all three tables go out together, so the material
    /// covers the texture.
    fn viewer_holds(&self, object: &SceneObject) -> bool {
        (object.mesh().raw() as usize) < self.sent.meshes
            && (object.material().raw() as usize) < self.sent.materials
    }

    /// The scene object a frame's renderer becomes. `None` while its mesh has no scene mesh yet.
    fn place(
        &mut self,
        view: &MeshObjectView,
        content: &impl FrameContent,
    ) -> Result<Option<SceneObject>, String> {
        let transform = scene_transform(&view.local_to_world).map_err(|error| error.to_string())?;
        let Some(mesh) = self.mesh_handle(view.mesh, content) else {
            return Ok(None);
        };
        let slot = view.slots.first();
        let texture = self.texture_handle(slot.map_or(0, |slot| slot.texture), content);
        let material = self
            .material_handle(texture)
            .map_err(|error| error.to_string())?;
        let tint = slot.map_or([WHITE_TINT; 3], |slot| {
            [slot.color[0], slot.color[1], slot.color[2]].map(tint_channel)
        });
        let tint = SceneTint::new(tint).map_err(|error| error.to_string())?;
        Ok(Some(
            SceneObject::new(
                transform,
                MeshHandle::from_raw(mesh),
                MaterialHandle::from_raw(material),
            )
            .with_tint(tint),
        ))
    }

    fn mesh_handle(&mut self, identity: u64, content: &impl FrameContent) -> Option<u32> {
        if let Some(index) = self.mesh_index.get(&identity) {
            return Some(*index);
        }
        let mesh = content.mesh(identity)?.clone();
        let index = u32::try_from(self.meshes.len()).ok()?;
        self.meshes.push(mesh);
        self.mesh_index.insert(identity, index);
        Some(index)
    }

    /// The texture handle of player texture `identity`, white for no texture and for one whose
    /// texels have not arrived.
    fn texture_handle(&mut self, identity: u64, content: &impl FrameContent) -> u32 {
        if identity == 0 {
            return WHITE;
        }
        if let Some(index) = self.texture_index.get(&identity) {
            return *index;
        }
        let Some(texture) = content.texture(identity).cloned() else {
            return WHITE;
        };
        let Ok(index) = u32::try_from(self.textures.len()) else {
            return WHITE;
        };
        self.textures.push(texture);
        self.texture_index.insert(identity, index);
        index
    }

    fn material_handle(&mut self, texture: u32) -> Result<u32, SceneValidationError> {
        if let Some(index) = self.material_index.get(&texture) {
            return Ok(*index);
        }
        let index = u32::try_from(self.materials.len()).map_err(|_| {
            SceneValidationError::ArithmeticOverflow {
                part: "material table",
            }
        })?;
        self.materials.push(material_on(texture)?);
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
