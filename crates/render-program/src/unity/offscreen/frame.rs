//! What the host hands over for one frame of offscreen work, and what it registers once.
//!
//! These are this crate's own types. A host adapter maps the world's export onto them; nothing
//! here depends on the world.

use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;
use std::sync::Arc;

use crate::ids::{CameraId, MaterialId, MeshId, RendererId, ShaderId, TextureId};
use crate::layers::PropertyLayer;
use crate::resources::BufferHandle;
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

impl std::fmt::Display for RenderDimension {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::D2 => f.write_str("a 2D texture"),
            Self::D2Array(layers) => write!(f, "a 2D array of {layers} layers"),
            Self::D3(depth) => write!(f, "a 3D texture of {depth} slices"),
            Self::Cube => f.write_str("a cube"),
            Self::CubeArray(cubes) => write!(f, "a cube array of {cubes} cubes"),
            Self::Unknown(code) => write!(f, "a texture of unknown dimension {code}"),
        }
    }
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
/// creates the texture again, empty, when a field that shapes its storage changes (see
/// [`same_storage`](Self::same_storage)). A rise of `generation` alone, which is what a sampler
/// edit makes, keeps the texture and its contents.
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

    /// Whether a texture made from `self` can serve `other`: every field but `generation` is
    /// the same, so the storage has the same shape, format and size and keeps what it holds.
    /// `generation` is how the host says the texture was edited, and a sampler edit changes no
    /// storage. The one rule that decides when storage is made again, for the state's creation
    /// of the texture and for its count of the bytes a frame would add.
    #[must_use]
    pub fn same_storage(&self, other: &Self) -> bool {
        let without_generation = |spec: &Self| Self {
            generation: 0,
            ..spec.clone()
        };
        without_generation(self) == without_generation(other)
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
    /// The declared defaults of the names the host fills in the globals, such as the main light.
    /// They sit below every global, so a value the world, the script or the host puts in the
    /// globals wins over the default the shader declares.
    pub below_globals: PropertyLayer,
    pub render_queue: i32,
}

/// A renderer: a mesh with a material for each sub-mesh slot. A sub-mesh without a slot is not
/// drawn.
#[derive(Clone, Debug)]
pub struct Renderer {
    pub mesh: MeshId,
    pub materials: Vec<MaterialId>,
    pub layer: u32,
    /// The renderer's property block, which every slot's draws read.
    pub block: PropertyLayer,
    /// The property block of single slots, by slot index. A slot's draws read its block above
    /// `block`, so it wins over `block` for a name both set. A slot without an entry reads
    /// `block` alone.
    pub slot_blocks: BTreeMap<usize, PropertyLayer>,
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
    /// The renderers the camera draws, as the frame fixed them at the camera's record: those of
    /// the set whose layer the mask also admits.
    pub renderers: Arc<BTreeSet<RendererId>>,
}

impl Camera {
    /// Whether the camera draws the renderer `id` on `layer`.
    #[must_use]
    pub fn draws(&self, id: RendererId, layer: u32) -> bool {
        layer < 32 && self.culling_mask & (1 << layer) != 0 && self.renderers.contains(&id)
    }
}

/// One mip of a render texture that a pass draws into.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Attachment {
    pub texture: TextureId,
    pub mip: u32,
}

/// What a pass draws into: the color storage of one to eight textures, in the order of the
/// fragment stage's outputs, and the depth storage of one texture, which may be a texture none
/// of the colors is of. A texture with no color has none to offer, and a depth buffer under a
/// color texture has one mip, so a pass whose depth does not fit the colors is drawn without it. Every color must have
/// the size of the first at its mip, and no two colors are the same mip of one texture.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Attachments {
    pub colors: Vec<Attachment>,
    pub depth: Option<Attachment>,
}

impl Attachments {
    /// A pass over at most one color, which a camera or a script names with one texture.
    #[must_use]
    pub fn single(color: Option<Attachment>, depth: Option<Attachment>) -> Self {
        Self {
            colors: color.into_iter().collect(),
            depth,
        }
    }

    /// The textures the attachments are of: the colors', then the depth's.
    pub fn textures(&self) -> impl Iterator<Item = TextureId> + '_ {
        self.colors
            .iter()
            .chain(&self.depth)
            .map(|attachment| attachment.texture)
    }
}

/// The parts of the attachments a clear sets. A part that is `None` keeps what it holds. A clear
/// of the depth also clears the stencil of a depth buffer that has one to 0, so `stencil` is
/// carried for the record and sets nothing of its own.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Clear {
    pub color: Option<[f64; 4]>,
    pub depth: Option<f32>,
    pub stencil: Option<u32>,
}

/// The instances a list draw makes.
#[derive(Clone, Debug)]
pub enum Instances {
    /// The sub-mesh repeated this many times with the renderer's own values: 1 draws it once and
    /// 0 draws nothing. What differs between instances is read from the instance index.
    Count(u32),
    /// Instances whose data is in a raw buffer, drawn through the shader's DOTS instancing variant.
    Dots(DotsBatch),
}

/// A batch of instances a game shader reads from a raw buffer, as the game's own code laid it out.
/// The host copies the words and indexes below and binds the buffer; it does not read the buffer.
#[derive(Clone, Debug)]
pub struct DotsBatch {
    /// The raw buffer the shader reads as `unity_DOTSInstanceData`.
    pub buffer: BufferHandle,
    /// The bytes of `buffer` the shader reads: the metadata offsets count from the window's
    /// start, which a device binds only at its storage offset alignment.
    pub window: Range<u64>,
    /// The metadata word of each per-instance property the game set, by the property's name
    /// (`unity_ObjectToWorld`, `_Color`). The word is the value the game gave the property's
    /// metadata, copied as it is.
    pub metadata: Vec<(String, u32)>,
    /// The index of each instance to draw, in the order they draw.
    pub visible: Vec<u32>,
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
    /// The instances of the sub-mesh the draw makes.
    pub instances: Instances,
    /// The values the list gives this draw alone, such as a light factor that depends on the
    /// culling result the draw came from. They win over the renderer's own `per_draw` values.
    pub per_draw: PropertyLayer,
}

/// The raster state a script set for the draws of a record, over the state their passes resolve.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DrawRaster {
    /// The rectangle in texels of the attachments that clips the draws: x, y, width and height
    /// from column 0 and row 0 at the attachments' mip, the corner a script calls the bottom
    /// left (the engine draws a scene's lowest part in row 0). `None` clips nothing. The part of
    /// the rectangle outside the attachments is dropped, and a rectangle with no texel left draws
    /// nothing.
    pub scissor: Option<[f32; 4]>,
    /// The rectangle in texels of the attachments that the draws' clip space maps to: x, y, width
    /// and height from column 0 and row 0 at the attachments' mip, the corner a script calls the
    /// bottom left. `None` maps clip space to the whole attachments. A rectangle that is not
    /// inside the attachments, or that has no area, draws nothing.
    pub viewport: Option<[f32; 4]>,
    /// The constant and the slope-scaled terms of the depth bias, as the script gave them, in the
    /// depth convention of an unreversed buffer: a positive value moves the stored depth away from
    /// the viewer, so the reversed buffer takes the negated values. The constant is rounded to whole units of the buffer's
    /// resolution. `[0.0, 0.0]` draws with the bias of the pass, which is none. The bias applies
    /// to triangles only; a line list is drawn without it.
    pub depth_bias: [f32; 2],
    /// Whether the draws blend their alpha channel as coverage: a colour blend of source alpha
    /// and one minus source alpha leaves the target holding the colour premultiplied by the
    /// alpha and the alpha as the coverage of everything drawn (source one, destination one
    /// minus source alpha), as it does over a backdrop that is cleared to transparent. A host
    /// that later composites the target over another image needs that alpha. A colour blend
    /// of any other kind keeps the pass's own alpha blend.
    pub coverage_alpha: bool,
}

/// A list of draws into attachments.
#[derive(Clone, Debug)]
pub struct DrawList {
    pub target: Attachments,
    /// The raster state of the whole list.
    pub raster: DrawRaster,
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
    /// The raster state of the draw.
    pub raster: DrawRaster,
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
    /// Generates mips 1 to N of a render texture from its mip 0: each level is the 2x2 box
    /// average of the level above it, made by a linear downsample of every layer or cube face on
    /// its own. The texture must be a color render texture of a format the device filters; a 3D
    /// texture and any other format are refused ([`MipsOfVolume`](super::OffscreenError::MipsOfVolume),
    /// [`MipsOfFormat`](super::OffscreenError::MipsOfFormat)). Afterwards every level of every layer counts as written
    /// at that point, so a later draw that samples the texture binds all of its levels.
    GenerateMips {
        texture: TextureId,
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

impl Record {
    /// The kind of the record, as a report names it.
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::CameraRender { .. } => "CameraRender",
            Self::Blit { .. } => "Blit",
            Self::CopyTexture { .. } => "CopyTexture",
            Self::GenerateMips { .. } => "GenerateMips",
            Self::Clear { .. } => "Clear",
            Self::Draws(_) => "Draws",
            Self::Procedural(_) => "Procedural",
            Self::Globals(_) => "Globals",
        }
    }

    /// The texture the record writes: a camera's, blit's or copy's target, the texture mips are
    /// generated for, or the first colour attachment of a clear or a draw (its depth when it has
    /// no colour). A record of globals writes none.
    #[must_use]
    pub fn target(&self) -> Option<TextureId> {
        let attached = |target: &Attachments| {
            target
                .colors
                .first()
                .or(target.depth.as_ref())
                .map(|attachment| attachment.texture)
        };
        match self {
            Self::CameraRender { target, .. }
            | Self::Blit { target, .. }
            | Self::CopyTexture { target, .. } => Some(*target),
            Self::GenerateMips { texture } => Some(*texture),
            Self::Clear { target, .. } => attached(target),
            Self::Draws(list) => attached(&list.target),
            Self::Procedural(draw) => attached(&draw.target),
            Self::Globals(_) => None,
        }
    }

    /// The texture whose depth buffer the record rendered with: the depth attachment a clear, a
    /// draw list or a procedural draw binds, which may be a texture of its own or the colour
    /// target's, and the depth buffer of a camera's target. A blit, a copy, a mip generation and
    /// a record of globals have none, and neither has a clear or a draw that binds no depth.
    #[must_use]
    pub fn depth_target(&self) -> Option<TextureId> {
        let attached = |target: &Attachments| target.depth.as_ref().map(|depth| depth.texture);
        match self {
            Self::CameraRender { target, .. } => Some(*target),
            Self::Clear { target, .. } => attached(target),
            Self::Draws(list) => attached(&list.target),
            Self::Procedural(draw) => attached(&draw.target),
            Self::Blit { .. }
            | Self::CopyTexture { .. }
            | Self::GenerateMips { .. }
            | Self::Globals(_) => None,
        }
    }
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
    /// The values the host gives every blit with a material, above the material and the
    /// globals: what `Graphics.Blit` itself sets, such as the transform of its quad.
    pub blit_values: PropertyLayer,
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
    /// The textures a texture slot reads when no layer sets it and the shader does not declare
    /// it: this engine binds a grey default texture of the slot's dimension there, so a draw
    /// the Old World player makes with such a slot is drawn and counted instead of left out. The
    /// host uploads each as it does any source texture. A draw that reads one is counted by
    /// property and reason in [`OffscreenOperations::stand_in_draws`](super::OffscreenOperations::stand_in_draws).
    /// Only a single-sampled float slot of a dimension that has a default here reads it. A
    /// property the shader declares, and a depth, multisampled or array slot, are missing as
    /// before. Without a default for a dimension, which is the default, every slot of that
    /// dimension no layer sets is missing.
    pub undeclared_textures: UndeclaredTextures,
    /// The indices into `records` after which the host reads the record's target, in colour and
    /// depth. A read leaves the frame's work as it was; see
    /// [`OffscreenOperations::take_taps`](super::OffscreenOperations::take_taps).
    pub taps: Vec<usize>,
}

/// The texture an unset, undeclared slot reads, by the view dimension of the slot.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UndeclaredTextures {
    /// For a slot that samples a 2D texture. The host's grey 2D default is RGBA 0.5 0.5 0.5 1.
    pub d2: Option<TextureId>,
    /// For a slot that samples a cube texture, such as the `_BlitCubeTexture` the URP blit
    /// program reflects beside its 2D source. Upload it as a [`SourceDimension::Cube`] source.
    pub cube: Option<TextureId>,
    /// For a slot that samples a 3D texture, such as an Old World `_3DNoiseTexture`. Upload it as
    /// a [`SourceDimension::D3`] source of one slice.
    pub d3: Option<TextureId>,
}

impl UndeclaredTextures {
    /// The default for a slot that samples through `view`, if the frame names one.
    pub(super) fn for_view(self, view: wgpu::TextureViewDimension) -> Option<TextureId> {
        match view {
            wgpu::TextureViewDimension::D2 => self.d2,
            wgpu::TextureViewDimension::Cube => self.cube,
            wgpu::TextureViewDimension::D3 => self.d3,
            _ => None,
        }
    }
}

impl Default for OffscreenFrame {
    fn default() -> Self {
        Self {
            main_texture: DEFAULT_MAIN_TEXTURE.to_owned(),
            blit_mesh: None,
            blit_values: PropertyLayer::default(),
            globals: PropertyLayer::default(),
            global_keywords: Vec::new(),
            render_textures: Vec::new(),
            materials: BTreeMap::new(),
            renderers: BTreeMap::new(),
            cameras: BTreeMap::new(),
            records: Vec::new(),
            later_reads: Vec::new(),
            probe: CameraProbe::default(),
            undeclared_textures: UndeclaredTextures::default(),
            taps: Vec::new(),
        }
    }
}

impl OffscreenFrame {
    /// The most draws building `record` plans, which a host that builds a frame in parts uses to
    /// keep a part within a budget of planned draws before it plans any.
    ///
    /// A camera render is bound by the material slots of the renderers its mask selects, a list
    /// by its items and a procedural draw by its passes. A draw left out for want of a pass, a
    /// texture or a mesh channel is planned by none, so the bound is at least what is planned.
    #[must_use]
    pub fn planned_draw_bound(&self, record: &Record) -> usize {
        match record {
            Record::CameraRender { camera, .. } => self.cameras.get(camera).map_or(0, |camera| {
                self.renderers
                    .iter()
                    .filter(|(id, renderer)| camera.draws(**id, renderer.layer))
                    .map(|(_, renderer)| renderer)
                    .map(|renderer| renderer.materials.len())
                    .sum()
            }),
            Record::Draws(list) => list.draws.len(),
            Record::Procedural(draw) => draw.passes.len(),
            Record::Blit { .. } => 1,
            Record::CopyTexture { .. }
            | Record::GenerateMips { .. }
            | Record::Clear { .. }
            | Record::Globals(_) => 0,
        }
    }
}

/// The shape of an uploaded source texture.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SourceDimension {
    /// A plain 2D texture: `bytes` is its one level.
    #[default]
    D2,
    /// A cube of six square faces, sampled through a cube view. `size` is one face, which must be
    /// square, and `bytes` holds the six faces one after the other in the order +X, -X, +Y, -Y,
    /// +Z, -Z, each holding its [`SourceTexture::mips`] levels from the base level down, every
    /// level as a 2D level is.
    Cube,
    /// A 2D array of this many layers, which may be one, sampled through an array view. `size` is
    /// one layer and `bytes` holds the layers one after the other, each holding its
    /// [`SourceTexture::mips`] levels from the base level down, every level as a 2D level is.
    D2Array(u32),
    /// A 3D texture of this many slices, which may be one, sampled through a 3D view. `size` is
    /// one slice and `bytes` holds the [`SourceTexture::mips`] levels from the base level down,
    /// each holding its slices one after the other, and a level has the base level's width,
    /// height and depth shifted down (at least one). The graph tracks one cell per volume, so
    /// all its levels are read together.
    D3(u32),
}

/// A texture the host uploads once.
#[derive(Clone, Debug)]
pub struct SourceTexture {
    pub id: TextureId,
    pub dimension: SourceDimension,
    pub size: [u32; 2],
    pub graphics_format: u32,
    /// The mip levels of the texture. For a [`SourceDimension::D2Array`] or a
    /// [`SourceDimension::Cube`], `bytes` has each layer's or face's whole chain, base level
    /// first, one layer or face after the other. For a [`SourceDimension::D3`] it has the chain
    /// of levels, each with its slices. A [`SourceDimension::D2`] has one level.
    pub mips: u32,
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
