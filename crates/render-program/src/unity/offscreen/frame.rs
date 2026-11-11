//! What the host hands over for one frame of offscreen work, and what it registers once.
//!
//! These are this crate's own types. A host adapter maps the world's export onto them; nothing
//! here depends on the world.

use std::collections::BTreeMap;
use std::ops::Range;
use std::sync::Arc;

use crate::ids::{CameraId, MaterialId, MeshId, RendererId, ShaderId, TextureId};
use crate::layers::PropertyLayer;
use crate::sampler::SamplerState;
use crate::unity::shader::PassSelector;

use super::probe::CameraProbe;

/// The name a blit binds its source under unless the frame says otherwise.
pub const DEFAULT_MAIN_TEXTURE: &str = "_MainTex";

/// The dimension of a render texture's storage, with the layers it holds where that is a choice.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RenderDimension {
    /// A plain 2D texture of one layer.
    #[default]
    D2,
    /// A 2D array of this many layers, which may be one.
    D2Array(u32),
    /// A 3D texture this many slices deep. It has one mip and no depth buffer, and only kernels
    /// read and write it: a record of the frame that names it is refused as `VolumeInGraph`, and
    /// a pixel read answers its first slice.
    D3(u32),
    /// A cube of six square faces, which a draw samples through a cube view.
    Cube,
    /// This many cubes, six faces each.
    CubeArray(u32),
    /// A dimension code the host has no shape for; the state refuses it as `UnknownDimension`
    /// and makes nothing.
    Unknown(u32),
}

impl RenderDimension {
    /// The array layers, or the slices of a volume, the storage holds: 1 for a plain texture, 6
    /// for a cube and none for a dimension the host could not map.
    #[must_use]
    pub const fn layers(self) -> u32 {
        match self {
            Self::D2 => 1,
            Self::D2Array(layers) | Self::D3(layers) => layers,
            Self::Cube => 6,
            Self::CubeArray(cubes) => cubes.saturating_mul(6),
            Self::Unknown(_) => 0,
        }
    }

    /// The view dimension a draw samples the texture through, or the code of a dimension the host
    /// could not map.
    ///
    /// # Errors
    /// Fails with the code of an [`Unknown`](Self::Unknown) dimension.
    pub const fn view_dimension(self) -> Result<wgpu::TextureViewDimension, u32> {
        Ok(match self {
            Self::D2 => wgpu::TextureViewDimension::D2,
            Self::D2Array(_) => wgpu::TextureViewDimension::D2Array,
            Self::D3(_) => wgpu::TextureViewDimension::D3,
            Self::Cube => wgpu::TextureViewDimension::Cube,
            Self::CubeArray(_) => wgpu::TextureViewDimension::CubeArray,
            Self::Unknown(code) => return Err(code),
        })
    }

    /// Whether the texture is 3D.
    #[must_use]
    pub const fn is_volume(self) -> bool {
        matches!(self, Self::D3(_))
    }
}

/// A render texture or texture array the frame draws into, copies into or samples. The state
/// creates the texture again when anything but the id changes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderTextureSpec {
    pub id: TextureId,
    pub size: [u32; 2],
    /// The shape of the storage and its layers or depth.
    pub dimension: RenderDimension,
    /// The engine's `GraphicsFormat` value of the color format.
    pub graphics_format: u32,
    /// 0 for no depth, else 16, 24 or 32.
    pub depth_bits: u8,
    pub mips: u32,
    /// Must agree with the format.
    pub srgb: bool,
    pub generation: u64,
}

impl RenderTextureSpec {
    /// The array layers or slices the texture holds.
    #[must_use]
    pub const fn layers(&self) -> u32 {
        self.dimension.layers()
    }
}

/// A material: a shader, keywords and properties.
#[derive(Clone, Debug, Default)]
pub struct Material {
    pub shader: Option<ShaderId>,
    pub keywords: Vec<String>,
    pub properties: PropertyLayer,
    /// The values of properties the material does not set, which the host derives from what its
    /// shader declares and the material's textures. They sit below the material's own.
    pub fallback: PropertyLayer,
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

/// One mip of a render texture that a pass draws into.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Attachment {
    pub texture: TextureId,
    pub mip: u32,
}

/// What a pass draws into: the color storage of one texture and the depth storage of the same or
/// another. A texture with no color has none to offer, and a depth buffer under a color texture
/// has one mip, so a pass whose depth does not fit the color is drawn without it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Attachments {
    pub color: Option<Attachment>,
    pub depth: Option<Attachment>,
}

impl Attachments {
    /// The textures the attachments are of: the color's, then the depth's.
    pub fn textures(&self) -> impl Iterator<Item = TextureId> {
        self.color
            .into_iter()
            .chain(self.depth)
            .map(|attachment| attachment.texture)
    }
}

/// The parts of the attachments a clear sets. A part that is `None` keeps what it holds. The
/// depth formats this renderer draws into have no stencil, so `stencil` is carried for the
/// record and sets nothing.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Clear {
    pub color: Option<[f64; 4]>,
    pub depth: Option<f32>,
    pub stencil: Option<u32>,
}

/// One draw of a list: a sub-mesh slot of a renderer.
#[derive(Clone, Debug)]
pub struct ListDraw {
    pub renderer: RendererId,
    pub slot: usize,
    /// The material that replaces the slot's own, if any.
    pub material: Option<MaterialId>,
    /// Which pass of the material's shader draws.
    pub pass: PassSelector,
}

/// A list of draws into attachments.
#[derive(Clone, Debug)]
pub struct DrawList {
    pub target: Attachments,
    /// The values of the view the list draws with, below the renderer's per-draw values and above
    /// the renderer's property block.
    pub values: PropertyLayer,
    /// The winding that counts as a front face, in framebuffer space.
    pub front_face: wgpu::FrontFace,
    pub draws: Vec<ListDraw>,
}

/// A draw with no mesh: the material's passes draw `vertices` vertices `instances` times, and the
/// vertex stage makes each vertex from its vertex and instance index. A pass whose vertex stage
/// declares vertex inputs is refused, since nothing feeds them. Both counts are nonzero: the
/// record of an empty draw is left out of the frame.
#[derive(Clone, Debug)]
pub struct ProceduralDraw {
    pub target: Attachments,
    /// The values of the view the draw uses, as a list's are.
    pub values: PropertyLayer,
    /// The draw's own values, such as its object matrix, below the view's.
    pub per_draw: PropertyLayer,
    /// The winding that counts as a front face, in framebuffer space.
    pub front_face: wgpu::FrontFace,
    pub material: MaterialId,
    /// The passes that draw, in order.
    pub passes: Vec<PassSelector>,
    pub topology: wgpu::PrimitiveTopology,
    pub vertices: u32,
    pub instances: u32,
}

/// The global values and keywords the records after it read, in place of the frame's own.
#[derive(Clone, Debug, Default)]
pub struct GlobalState {
    pub values: PropertyLayer,
    pub keywords: Vec<String>,
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
    /// Sets the parts of the attachments that `clear` names.
    Clear {
        target: Attachments,
        clear: Clear,
    },
    /// Draws a list into its attachments, keeping what they hold.
    Draws(Arc<DrawList>),
    /// Draws generated vertices with a material into its attachments, keeping what they hold.
    Procedural(Arc<ProceduralDraw>),
    /// Replaces the global values and keywords of the records after it, until the next such
    /// record. Records before the first one read the frame's own.
    Globals(Arc<GlobalState>),
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
    /// The camera renders to examine in detail; none by default.
    pub probe: CameraProbe,
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
            probe: CameraProbe::default(),
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
    Float32,
    Float32x2,
    Float32x3,
    Float32x4,
    Unorm8x4,
}

impl ChannelFormat {
    pub(crate) fn vertex_format(self) -> wgpu::VertexFormat {
        match self {
            Self::Float32 => wgpu::VertexFormat::Float32,
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
