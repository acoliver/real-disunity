//! What the host hands over for one frame of offscreen work, and what it registers once.
//!
//! These are this crate's own types. A host adapter maps the world's export onto them; nothing
//! here depends on the world.

use std::collections::BTreeMap;
use std::ops::Range;

use crate::ids::{CameraId, MaterialId, MeshId, RendererId, ShaderId, TextureId};
use crate::layers::PropertyLayer;
use crate::sampler::SamplerState;
use crate::unity::shader::PassSelector;

/// The name a blit binds its source under unless the frame says otherwise.
pub const DEFAULT_MAIN_TEXTURE: &str = "_MainTex";

/// A render texture or texture array the frame draws into, copies into or samples. The state
/// creates the texture again when anything but the id changes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderTextureSpec {
    pub id: TextureId,
    pub size: [u32; 2],
    /// Array layers; 1 for a plain render texture.
    pub layers: u32,
    /// The engine's `GraphicsFormat` value of the color format.
    pub graphics_format: u32,
    /// 0 for no depth, else 16, 24 or 32.
    pub depth_bits: u8,
    pub mips: u32,
    /// Must agree with the format.
    pub srgb: bool,
    pub generation: u64,
}

/// A material: a shader, keywords and properties.
#[derive(Clone, Debug, Default)]
pub struct Material {
    pub shader: Option<ShaderId>,
    pub keywords: Vec<String>,
    pub properties: PropertyLayer,
    pub render_queue: i32,
}

/// A renderer: a mesh with a material for each sub-mesh slot. A sub-mesh without a slot is not
/// drawn.
#[derive(Clone, Debug)]
pub struct Renderer {
    pub mesh: MeshId,
    pub materials: Vec<MaterialId>,
    pub layer: u32,
    /// The renderer's property block.
    pub block: PropertyLayer,
    /// The values the caller supplies for this renderer's draws, such as its matrices, under
    /// the names its shaders use.
    pub per_draw: PropertyLayer,
}

/// A camera that draws the frame's renderers into a render texture.
#[derive(Clone, Debug)]
pub struct Camera {
    /// A renderer is drawn when bit `layer` of this mask is set.
    pub culling_mask: u32,
    /// The color the target is cleared to, or `None` to keep what it holds.
    pub clear_color: Option<[f64; 4]>,
    pub clear_depth: bool,
    /// Which pass of each material's shader the camera draws.
    pub pass: PassSelector,
    /// The camera's own values, such as its view and projection, under the names its shaders
    /// use. They sit below the renderer's per-draw values.
    pub values: PropertyLayer,
    /// The winding that counts as a front face, in framebuffer space.
    pub front_face: wgpu::FrontFace,
}

/// One call, in call order.
#[derive(Clone, Debug)]
pub enum Record {
    CameraRender {
        camera: CameraId,
        target: TextureId,
    },
    Blit {
        source: TextureId,
        target: TextureId,
        material: Option<MaterialId>,
        /// The pass of the material's shader.
        pass: usize,
    },
    CopyTexture {
        source: TextureId,
        source_element: u32,
        source_mip: u32,
        target: TextureId,
        target_element: u32,
        target_mip: u32,
    },
}

/// A range of a texture the scene samples after the offscreen work.
#[derive(Clone, Debug)]
pub struct ReadRange {
    pub texture: TextureId,
    pub mips: Range<u32>,
    pub layers: Range<u32>,
}

/// One frame of offscreen work.
#[derive(Clone, Debug)]
pub struct OffscreenFrame {
    /// The property a blit binds its source under.
    pub main_texture: String,
    /// The mesh a blit draws: triangles with no index buffer, in clip space.
    pub blit_mesh: Option<MeshId>,
    pub globals: PropertyLayer,
    pub global_keywords: Vec<String>,
    pub render_textures: Vec<RenderTextureSpec>,
    pub materials: BTreeMap<MaterialId, Material>,
    pub renderers: BTreeMap<RendererId, Renderer>,
    pub cameras: BTreeMap<CameraId, Camera>,
    pub records: Vec<Record>,
    pub later_reads: Vec<ReadRange>,
}

impl Default for OffscreenFrame {
    fn default() -> Self {
        Self {
            main_texture: DEFAULT_MAIN_TEXTURE.to_owned(),
            blit_mesh: None,
            globals: PropertyLayer::default(),
            global_keywords: Vec::new(),
            render_textures: Vec::new(),
            materials: BTreeMap::new(),
            renderers: BTreeMap::new(),
            cameras: BTreeMap::new(),
            records: Vec::new(),
            later_reads: Vec::new(),
        }
    }
}

/// A texture the host uploads once: uncompressed, one mip.
#[derive(Clone, Debug)]
pub struct SourceTexture {
    pub id: TextureId,
    pub size: [u32; 2],
    pub graphics_format: u32,
    pub bytes: Vec<u8>,
    /// The state the texture is read with unless a property overrides it.
    pub sampler: Option<SamplerState>,
}

/// The format of one vertex channel's data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChannelFormat {
    Float32x2,
    Float32x3,
    Float32x4,
    Unorm8x4,
}

impl ChannelFormat {
    pub(crate) fn vertex_format(self) -> wgpu::VertexFormat {
        match self {
            Self::Float32x2 => wgpu::VertexFormat::Float32x2,
            Self::Float32x3 => wgpu::VertexFormat::Float32x3,
            Self::Float32x4 => wgpu::VertexFormat::Float32x4,
            Self::Unorm8x4 => wgpu::VertexFormat::Unorm8x4,
        }
    }

    pub(crate) fn stride(self) -> u64 {
        self.vertex_format().size()
    }
}

/// A vertex channel's tightly packed data. `source` is the channel number the shader's vertex
/// input table names.
#[derive(Clone, Debug)]
pub struct MeshChannel {
    pub source: i8,
    pub format: ChannelFormat,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug)]
pub enum MeshIndices {
    None,
    U16(Vec<u16>),
    U32(Vec<u32>),
}

/// A range of a mesh's indices, or of its vertices when it has no indices.
#[derive(Clone, Copy, Debug)]
pub struct SubMesh {
    pub first: u32,
    pub count: u32,
    pub base_vertex: i32,
}

#[derive(Clone, Debug)]
pub struct Mesh {
    pub vertex_count: u32,
    pub channels: Vec<MeshChannel>,
    pub indices: MeshIndices,
    pub sub_meshes: Vec<SubMesh>,
}
