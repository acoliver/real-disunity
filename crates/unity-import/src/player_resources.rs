//! A player's `Resources` as the managed engine loads them.
//!
//! [`plan_player_resources`] lists every path of the player's resource container with the
//! class of the object it names. Objects of classes the engine can build become plan
//! assets: a `TextAsset` names the byte range of its contents in the player's file, and a
//! script asset (a `ScriptableObject`) has its decoded fields written to a file beside the
//! plan, with references to other planned resources as `{"$asset": i}`, since a few hold
//! tables of many megabytes. Textures, sprites and sprite atlases are planned too, along
//! with the ones such an object references even when the container does not list them (see
//! [`crate::player_sprites`]), and so are shaders (see [`crate::player_shader_refs`]) and materials
//! (see [`crate::player_materials`]). A `GameObject` is a prefab: a scene plan of the object and its
//! descendants in a file beside the plan (see [`crate::player_prefabs`]); a prefab that does not
//! plan keeps its error and the rest of the plan is unaffected. Entries of other classes keep
//! only their class, so loading one names the class the engine does not build yet.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

use serde::Serialize;
use serde_json::{Value as Json, json};
use worldrunner_unity_serialized::{
    ClassId, graphics::Material, managers::ResourceManager, object::PPtr,
    render_texture::RenderTexture, script_data::ScriptLayouts, text_asset::TextAsset,
};

use crate::{
    PlayerError,
    player_files::PlayerFiles,
    player_materials::{
        PlanMaterial, decode_material, plan_material, unplanned_reference as unplanned_material,
    },
    player_pipeline::{GraphicsAssets, admit_graphics_assets},
    player_plan::{PlanSource, tag_manager_names},
    player_prefabs::{is_root, plan_prefab, prefab_name},
    player_render_textures::{
        decode_render_texture, plan_render_texture, unplanned_reference as unplanned_render_texture,
    },
    player_scripts::script_fields,
    player_shader_refs::{ShaderDecl, decode_shader, unplanned_reference},
    player_sprites::{self, AssetRef, TextureData},
    player_values::{FieldWriter, not_held},
};

#[derive(Debug, Serialize)]
pub struct ResourcesPlan {
    /// Container entries in container order, except those that name no object. Paths are
    /// lower case, without extensions.
    pub entries: Vec<ResourceEntry>,
    pub assets: Vec<ResourceAsset>,
    /// The render pipeline assets of the graphics and quality settings, by asset index.
    pub graphics: GraphicsAssets,
    pub differences: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct ResourceEntry {
    pub path: String,
    /// The Unity class of the object, such as `TextAsset`.
    pub class: String,
    /// The planned asset, when the engine builds this class.
    pub asset: Option<usize>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
pub enum ResourceAsset {
    /// The contents are `length` bytes at `offset` in the player file `file`.
    TextAsset {
        name: String,
        file: PathBuf,
        offset: u64,
        length: u64,
    },
    /// A script asset of `class` in `assembly`, with its fields in the JSON file `fields`,
    /// relative to the plan's directory.
    Script {
        class: String,
        assembly: String,
        name: String,
        fields: String,
    },
    /// A prefab: the scene plan in the JSON file `plan`, relative to the plan's directory,
    /// holds the root game object and its descendants as template objects.
    Prefab { name: String, plan: String },
    /// A prefab the planner could not plan, with the reason loading it reports.
    UnplannedPrefab { name: String, error: String },
    /// A 2D texture. `format` is the `TextureFormat` value and `color_space` the decoded
    /// value; `filter_mode` and the wrap modes are the sampler's values.
    Texture2D {
        name: String,
        width: i32,
        height: i32,
        format: i32,
        mip_count: i32,
        readable: bool,
        color_space: i32,
        filter_mode: i32,
        wrap_u: i32,
        wrap_v: i32,
        wrap_w: i32,
        aniso_level: i32,
        mip_bias: f32,
        data: TextureData,
    },
    /// A rectangle of a texture. Rectangles and the border are in pixels (x, y, width,
    /// height and left, bottom, right, top); the pivot is a fraction of the rectangle.
    /// `packing_rotation` and `mesh_type` are the sprite's packed settings bits.
    Sprite {
        name: String,
        rect: [f32; 4],
        pivot: [f32; 2],
        pixels_per_unit: f32,
        border: [f32; 4],
        extrude: u32,
        texture: Option<AssetRef>,
        texture_rect: [f32; 4],
        texture_rect_offset: [f32; 2],
        packed: bool,
        packing_rotation: u32,
        mesh_type: u32,
    },
    /// Sprites packed together, in the atlas's order.
    SpriteAtlas {
        name: String,
        tag: String,
        is_variant: bool,
        sprites: Vec<AssetRef>,
    },
    /// A shader, by the name its source declares, with the render queue of its first
    /// sub-shader and the numbers of sub-shaders and of passes in the first one. Programs are
    /// not planned.
    Shader {
        name: String,
        render_queue: i32,
        pass_count: usize,
        subshader_count: usize,
    },
    /// A shader the planner could not decode, with the reason loading it reports.
    UnplannedShader { error: String },
    /// A material, with its shader and textures as asset indexes.
    Material {
        #[serde(flatten)]
        material: PlanMaterial,
    },
    /// A material the planner could not decode, with the reason loading it reports.
    UnplannedMaterial { error: String },
    /// A texture a camera or script draws into. `depth_format` is the `GraphicsFormat` of the
    /// depth buffer (0 for none) and `color_format` that of the color buffer; `dimension`,
    /// `filter_mode` and the wrap modes are the engine's enum values.
    RenderTexture {
        name: String,
        width: i32,
        height: i32,
        depth_format: i32,
        color_format: i32,
        anti_aliasing: i32,
        mip_maps: bool,
        generate_mips: bool,
        dimension: i32,
        filter_mode: i32,
        aniso_level: i32,
        wrap_u: i32,
        wrap_v: i32,
        wrap_w: i32,
    },
    /// A render texture the planner could not decode, with the reason loading it reports.
    UnplannedRenderTexture { error: String },
}

/// An object of a player file, by file path and path ID.
pub(crate) type ObjectKey = (PathBuf, i64);

/// Plans the resources of the player in `data_dir`, writing script asset fields and inline
/// texture data under `output/resources`. Script asset fields decode with `layouts`.
///
/// # Errors
/// Fails when a file cannot be read or written, the resource manager, a text asset or a
/// sprite object does not decode, a texture's streamed data cannot be found, or a script
/// asset's fields do not decode with its layout.
pub fn plan_player_resources(
    data_dir: &Path,
    layouts: &ScriptLayouts,
    output: &Path,
) -> Result<ResourcesPlan, PlayerError> {
    let mut files = PlayerFiles::new(data_dir);
    let managers = data_dir.join("globalgamemanagers");
    let container = resource_container(&mut files, &managers)?;
    let mut planner = ResourcePlanner {
        registry: Registry::new(files),
        output: output.to_owned(),
        tags: tag_manager_names(data_dir)?.0,
        assets: Vec::new(),
        differences: Vec::new(),
        unplanned_prefabs: 0,
    };
    let mut entries = Vec::with_capacity(container.len());
    for (path, pptr) in container {
        if pptr.is_null() {
            planner
                .differences
                .push(format!("resource {path} names no object"));
            continue;
        }
        let file = planner.registry.files.referenced_file(&managers, pptr)?;
        let class = planner.registry.class(&file, pptr.path_id)?;
        let asset = planner.registry.admit(&(file, pptr.path_id), class)?;
        entries.push(ResourceEntry {
            path,
            class: class.to_string(),
            asset,
        });
    }
    let graphics = admit_graphics_assets(&mut planner.registry, &managers)?;
    // Objects an asset references join the queue while it is planned, so the queue grows
    // as it is walked and asset `i` is always `queue[i]`.
    let mut next = 0;
    while let Some((key, class)) = planner.registry.queue.get(next).cloned() {
        let asset = planner.plan_asset(&key, class, layouts)?;
        planner.assets.push(asset);
        next += 1;
    }
    planner.differences.append(&mut planner.registry.notes);
    if planner.unplanned_prefabs > 0 {
        planner.differences.push(format!(
            "{} prefabs did not plan",
            planner.unplanned_prefabs
        ));
    }
    Ok(ResourcesPlan {
        entries,
        assets: planner.assets,
        graphics,
        differences: planner.differences,
    })
}

/// Whether the plan holds objects of `class` as assets.
fn buildable(class: ClassId) -> bool {
    class == ClassId::TEXT_ASSET
        || class == ClassId::MONO_BEHAVIOUR
        || class == ClassId::GAME_OBJECT
        || referenced_on_demand(class)
}

/// Classes a reference plans even when the container does not list the object.
fn referenced_on_demand(class: ClassId) -> bool {
    class == ClassId::TEXTURE_2D
        || class == ClassId::SPRITE
        || class == ClassId::SPRITE_ATLAS
        || class == ClassId::SHADER
        || class == ClassId::MATERIAL
        || class == ClassId::RENDER_TEXTURE
        || class == ClassId::GAME_OBJECT
}

fn resource_container(
    files: &mut PlayerFiles,
    managers: &Path,
) -> Result<Vec<(String, PPtr)>, PlayerError> {
    let open = files.open_path(managers)?;
    let Some(path_id) = open
        .file
        .objects()
        .iter()
        .find(|object| object.class_id == ClassId::RESOURCE_MANAGER)
        .map(|object| object.path_id)
    else {
        return Ok(Vec::new());
    };
    let manager: ResourceManager = open.decode(path_id)?;
    Ok(manager.container)
}

/// The objects admitted to the plan, by the index of their asset.
pub(crate) struct Registry {
    pub(crate) files: PlayerFiles,
    planned: BTreeMap<ObjectKey, usize>,
    /// Admitted objects in asset order.
    pub(crate) queue: Vec<(ObjectKey, ClassId)>,
    /// Textures the engine does not build, which are noted once.
    rejected: BTreeSet<ObjectKey>,
    /// Each admitted shader as decoded, or why it did not.
    shaders: BTreeMap<ObjectKey, Result<ShaderDecl, String>>,
    /// Each admitted material as decoded, or why it did not.
    materials: BTreeMap<ObjectKey, Result<Material, String>>,
    /// Each admitted render texture as decoded, or why it did not.
    render_textures: BTreeMap<ObjectKey, Result<RenderTexture, String>>,
    /// Differences found while admitting and while planning textures, sprites and atlases.
    pub(crate) notes: Vec<String>,
}

impl Registry {
    fn new(files: PlayerFiles) -> Self {
        Self {
            files,
            planned: BTreeMap::new(),
            queue: Vec::new(),
            rejected: BTreeSet::new(),
            shaders: BTreeMap::new(),
            materials: BTreeMap::new(),
            render_textures: BTreeMap::new(),
            notes: Vec::new(),
        }
    }

    pub(crate) fn class(&mut self, file: &Path, path_id: i64) -> Result<ClassId, PlayerError> {
        self.files
            .open_path(file)?
            .file
            .object(path_id)
            .map(|object| object.class_id)
            .ok_or_else(|| PlayerError::MissingObject {
                path: file.to_owned(),
                path_id,
            })
    }

    /// The asset index of the object, planning it first when it is new and the engine builds
    /// its class; `None` when it is not built.
    pub(crate) fn admit(
        &mut self,
        key: &ObjectKey,
        class: ClassId,
    ) -> Result<Option<usize>, PlayerError> {
        if let Some(&index) = self.planned.get(key) {
            return Ok(Some(index));
        }
        if !buildable(class) || self.rejected.contains(key) {
            return Ok(None);
        }
        if class == ClassId::TEXTURE_2D
            && let Some(flaw) = player_sprites::texture_flaw(&mut self.files, key)?
        {
            self.notes.push(flaw);
            self.rejected.insert(key.clone());
            return Ok(None);
        }
        if class == ClassId::SHADER {
            let decoded = decode_shader(&mut self.files, key);
            self.shaders.insert(key.clone(), decoded);
        }
        if class == ClassId::MATERIAL {
            let decoded = decode_material(&mut self.files, key);
            self.materials.insert(key.clone(), decoded);
        }
        if class == ClassId::RENDER_TEXTURE {
            let decoded = decode_render_texture(&mut self.files, key);
            self.render_textures.insert(key.clone(), decoded);
        }
        self.planned.insert(key.clone(), self.queue.len());
        self.queue.push((key.clone(), class));
        Ok(Some(self.queue.len() - 1))
    }

    /// `{"$asset": i}` for a reference from `owner` to a planned object, or why there is none.
    pub(crate) fn reference(&mut self, owner: &Path, pptr: PPtr) -> Result<Json, String> {
        self.asset_index(owner, pptr)
            .map(|index| json!({ "$asset": index }))
    }

    /// The asset index of the object a reference from `owner` names. A texture, sprite or
    /// atlas or prefab root is planned by being referenced; objects of other classes must be
    /// planned already. A game object that is not a root has no asset.
    pub(crate) fn asset_index(&mut self, owner: &Path, pptr: PPtr) -> Result<usize, String> {
        let file = self
            .files
            .referenced_file(owner, pptr)
            .map_err(|_| not_held(pptr))?;
        let key = (file, pptr.path_id);
        let class = self.class(&key.0, key.1).map_err(|_| not_held(pptr))?;
        if class == ClassId::GAME_OBJECT && !is_root(&mut self.files, &key) {
            return Err(format!(
                "references object {} of file {}, which is not a prefab root",
                pptr.path_id, pptr.file_id
            ));
        }
        if let Some(&index) = self.planned.get(&key) {
            return self.usable(&key, index, pptr);
        }
        if !referenced_on_demand(class) {
            return Err(not_held(pptr));
        }
        match self.admit(&key, class) {
            Ok(Some(index)) => self.usable(&key, index, pptr),
            Ok(None) => Err(not_held(pptr)),
            Err(error) => Err(error.to_string()),
        }
    }

    /// The asset index of an admitted object, unless it is a shader, material or render texture
    /// that did not decode.
    fn usable(&self, key: &ObjectKey, index: usize, pptr: PPtr) -> Result<usize, String> {
        if let Some(Err(cause)) = self.render_textures.get(key) {
            return Err(unplanned_render_texture(pptr, cause));
        }
        match (self.shaders.get(key), self.materials.get(key)) {
            (Some(Err(cause)), _) => Err(unplanned_reference(pptr, cause)),
            (_, Some(Err(cause))) => Err(unplanned_material(pptr, cause)),
            _ => Ok(index),
        }
    }
}

/// References from a prefab to anything outside it are resource assets.
impl PlanSource for Registry {
    fn files(&mut self) -> &mut PlayerFiles {
        &mut self.files
    }

    fn asset(&mut self, owner: &Path, pptr: PPtr) -> Result<usize, String> {
        self.asset_index(owner, pptr)
    }
}

struct ResourcePlanner {
    registry: Registry,
    output: PathBuf,
    /// Tag names by tag index, for the objects of prefabs.
    tags: BTreeMap<u16, String>,
    assets: Vec<ResourceAsset>,
    differences: Vec<String>,
    unplanned_prefabs: usize,
}

impl ResourcePlanner {
    fn plan_asset(
        &mut self,
        key: &ObjectKey,
        class: ClassId,
        layouts: &ScriptLayouts,
    ) -> Result<ResourceAsset, PlayerError> {
        let (file, path_id) = key;
        let index = self.assets.len();
        match class {
            ClassId::TEXT_ASSET => self.text_asset(file, *path_id),
            ClassId::TEXTURE_2D => {
                player_sprites::plan_texture(&mut self.registry, key, &self.output, index)
            }
            ClassId::SPRITE => player_sprites::plan_sprite(&mut self.registry, key),
            ClassId::SPRITE_ATLAS => player_sprites::plan_atlas(&mut self.registry, key),
            ClassId::GAME_OBJECT => self.prefab(key, layouts),
            ClassId::SHADER => Ok(self.shader(key)),
            ClassId::MATERIAL => Ok(self.material(key)),
            ClassId::RENDER_TEXTURE => Ok(self.render_texture(key)),
            _ => self.script_asset(key, layouts),
        }
    }

    /// The shader `key` as admitted: its declarations, or an error entry that names why it did
    /// not decode.
    fn shader(&mut self, key: &ObjectKey) -> ResourceAsset {
        let decoded = (self.registry.shaders.get(key).cloned())
            .unwrap_or_else(|| Err("it was not admitted".to_owned()));
        match decoded {
            Ok(decl) => ResourceAsset::Shader {
                name: decl.name,
                render_queue: decl.render_queue,
                pass_count: decl.pass_count,
                subshader_count: decl.subshader_count,
            },
            Err(error) => {
                let (file, path_id) = key;
                let error = format!("shader object {path_id} of {}: {error}", file.display());
                self.differences
                    .push(format!("shader did not plan: {error}"));
                ResourceAsset::UnplannedShader { error }
            }
        }
    }

    /// The render texture `key` as admitted: its settings, or an error entry that names why it
    /// did not decode.
    fn render_texture(&mut self, key: &ObjectKey) -> ResourceAsset {
        let decoded = (self.registry.render_textures.get(key).cloned())
            .unwrap_or_else(|| Err("it was not admitted".to_owned()));
        plan_render_texture(key, decoded, &mut self.differences)
    }

    /// The material `key` as admitted, with its shader and textures planned as assets, or an
    /// error entry that names why it did not decode.
    fn material(&mut self, key: &ObjectKey) -> ResourceAsset {
        let (file, path_id) = key;
        let decoded = (self.registry.materials.get(key).cloned())
            .unwrap_or_else(|| Err("it was not admitted".to_owned()));
        match decoded {
            Ok(material) => {
                let registry = &mut self.registry;
                let planned = plan_material(
                    &material,
                    |pptr| registry.asset_index(file, pptr),
                    &mut self.differences,
                );
                ResourceAsset::Material { material: planned }
            }
            Err(error) => {
                let error = format!("material object {path_id} of {}: {error}", file.display());
                self.differences
                    .push(format!("material did not plan: {error}"));
                ResourceAsset::UnplannedMaterial { error }
            }
        }
    }

    /// The prefab whose root is `key`, or the reason it does not plan.
    fn prefab(
        &mut self,
        key: &ObjectKey,
        layouts: &ScriptLayouts,
    ) -> Result<ResourceAsset, PlayerError> {
        let planned = plan_prefab(&mut self.registry, key, &self.tags, layouts);
        let mut scene = match planned {
            Ok(scene) => scene,
            Err(error) => {
                let name = prefab_name(&mut self.registry.files, key);
                self.unplanned_prefabs += 1;
                self.differences
                    .push(format!("prefab {name} did not plan: {error}"));
                return Ok(ResourceAsset::UnplannedPrefab {
                    name,
                    error: error.to_string(),
                });
            }
        };
        let name = scene.scene.clone();
        self.differences.extend(
            scene
                .differences
                .drain(..)
                .map(|line| format!("prefab {name}: {line}")),
        );
        let relative = format!("prefabs/{}.json", self.assets.len());
        let bytes = serde_json::to_vec(&scene).expect("plans serialize");
        player_sprites::write_resource(&self.output, &relative, &bytes)?;
        Ok(ResourceAsset::Prefab {
            name,
            plan: relative,
        })
    }

    fn script_asset(
        &mut self,
        key: &ObjectKey,
        layouts: &ScriptLayouts,
    ) -> Result<ResourceAsset, PlayerError> {
        let (file, path_id) = key;
        let data = script_fields(&mut self.registry.files, file, *path_id, layouts)?.map_err(
            |(script, reason)| PlayerError::InvalidHierarchy {
                path: file.clone(),
                reason: format!("resource object {path_id}: script {script}: {reason}"),
            },
        )?;
        let Some(script) = data.class else {
            // A script asset whose script the build stripped holds no data; Unity loads it
            // as a plain ScriptableObject.
            return Ok(ResourceAsset::Script {
                class: "UnityEngine.ScriptableObject".to_owned(),
                assembly: "UnityEngine.CoreModule".to_owned(),
                name: data.name,
                fields: self.write_fields(&json!({}))?,
            });
        };
        let place = format!("resource {}", data.name);
        let registry = &mut self.registry;
        let fields = FieldWriter::new(|pptr| registry.reference(file, pptr), &mut self.differences)
            .fields(&place, &data.fields);
        Ok(ResourceAsset::Script {
            class: script.full_name,
            assembly: script.assembly,
            name: data.name,
            fields: self.write_fields(&fields)?,
        })
    }

    /// Writes the fields of the asset being planned and returns the file's relative path.
    fn write_fields(&self, fields: &Json) -> Result<String, PlayerError> {
        let relative = format!("resources/{}.json", self.assets.len());
        let bytes = serde_json::to_vec(fields).expect("JSON values serialize");
        player_sprites::write_resource(&self.output, &relative, &bytes)?;
        Ok(relative)
    }

    fn text_asset(&mut self, file: &Path, path_id: i64) -> Result<ResourceAsset, PlayerError> {
        let open = self.registry.files.open_path(file)?;
        let asset: TextAsset = open.decode(path_id)?;
        let object = *open.file.object(path_id).expect("decoded objects exist");
        let at = |position: usize| object.offset + position as u64;
        Ok(ResourceAsset::TextAsset {
            name: asset.name,
            file: std::path::absolute(file).map_err(|error| PlayerError::Io {
                path: file.to_owned(),
                source: error,
            })?,
            offset: at(asset.contents.start),
            length: (asset.contents.end - asset.contents.start) as u64,
        })
    }
}
