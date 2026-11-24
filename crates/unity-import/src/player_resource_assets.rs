//! The assets of a resource plan, one variant per class the managed engine builds. See
//! [`crate::player_resources`] for how they are planned.

use std::path::PathBuf;

use serde::Serialize;

use crate::{
    player_animators::PlanClipOverride,
    player_compute::PlanKernel,
    player_materials::PlanMaterial,
    player_meshes::PlanMesh,
    player_sprites::{self, AssetRef, TextureData},
};

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
    /// A font: its font file is `length` bytes at `offset` in the player file `file`, none when
    /// the font has no file of its own.
    Font {
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
    /// A 2D texture (see [`player_sprites::Texture2DAsset`]).
    Texture2D {
        #[serde(flatten)]
        texture: player_sprites::Texture2DAsset,
    },
    /// A stack of equally sized slices. `graphics_format` is the `GraphicsFormat` value of the
    /// pixels, `depth` the number of slices; the other fields are those of `Texture2D`.
    Texture2DArray {
        name: String,
        width: i32,
        height: i32,
        depth: i32,
        graphics_format: i32,
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
    /// A cube of six square faces, planned as a 2D texture is (see
    /// [`player_sprites::Texture2DAsset`]): `format` is the `TextureFormat` value, and the bytes
    /// are the six faces in the order +X, -X, +Y, -Y, +Z, -Z, each with its whole mip chain.
    Cubemap {
        #[serde(flatten)]
        texture: player_sprites::Texture2DAsset,
    },
    /// A volume of `depth` slices. `graphics_format` is the `GraphicsFormat` value of the pixels
    /// and the bytes are the mip levels from the base level down, each with its slices one after
    /// the other; the other fields are those of `Texture2DArray`.
    Texture3D {
        name: String,
        width: i32,
        height: i32,
        depth: i32,
        graphics_format: i32,
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
    /// A rectangle of a texture (see [`player_sprites::SpriteAsset`]).
    Sprite {
        #[serde(flatten)]
        sprite: player_sprites::SpriteAsset,
    },
    /// Sprites packed together, in the atlas's order.
    SpriteAtlas {
        name: String,
        tag: String,
        is_variant: bool,
        sprites: Vec<AssetRef>,
    },
    /// A shader, by the name its source declares, with the render queue of its first
    /// sub-shader, the number of sub-shaders, the names of the passes in the first one
    /// (`pass_count` of them, the empty string for an unnamed pass), that sub-shader's tags
    /// in declared order and the name of the fallback shader (empty for none). The programs
    /// are not planned; `file` and `path_id` name the serialized shader object they are read
    /// from, `file` relative to the data directory.
    Shader {
        name: String,
        render_queue: i32,
        pass_count: usize,
        pass_names: Vec<String>,
        subshader_count: usize,
        tags: Vec<(String, String)>,
        fallback: String,
        file: String,
        path_id: i64,
    },
    /// A shader the planner could not decode, with the reason loading it reports.
    UnplannedShader { error: String },
    /// A compute shader, by the name its data declares, with the serialized file and object its
    /// compiled kernels are read from (`file` relative to the data directory) and its kernels in
    /// index order. A kernel's thread group size is `null` when the data states none.
    ComputeShader {
        name: String,
        file: String,
        path_id: i64,
        kernels: Vec<PlanKernel>,
    },
    /// A compute shader the planner could not decode, with the reason loading it reports.
    UnplannedComputeShader { error: String },
    /// A material, with its shader and textures as asset indexes.
    Material {
        #[serde(flatten)]
        material: PlanMaterial,
    },
    /// A material the planner could not decode, with the reason loading it reports.
    UnplannedMaterial { error: String },
    /// A mesh: its channels, sub-meshes and bounds, with the vertex and index data in the file
    /// `resource` (see [`crate::player_meshes`]).
    Mesh {
        #[serde(flatten)]
        mesh: PlanMesh,
    },
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
    /// An animation clip, by name only: the engine plays no animation yet.
    AnimationClip { name: String },
    /// An animator controller and the clips it lists, as asset indexes (null where the plan
    /// cannot hold a clip).
    AnimatorController {
        name: String,
        clips: Vec<Option<usize>>,
    },
    /// A controller that replaces some clips of its base controller (see
    /// [`crate::player_animators`]).
    AnimatorOverrideController {
        name: String,
        base_controller: Option<usize>,
        overrides: Vec<PlanClipOverride>,
    },
}
