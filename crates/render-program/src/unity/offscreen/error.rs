//! Errors of offscreen work. Each names what was missing or unsupported.

use worldrunner_render_post::contracts::ContractError;

use crate::error::ProgramError;
use crate::ids::{MaterialId, MeshId, ShaderId, TextureId};
use crate::unity::constants::ConstantError;
use crate::unity::layout::LayoutError;
use crate::unity::shader::ResolveError;
use crate::unity::state::StateError;

use super::frame::RenderDimension;
use super::slot_dimension;

#[derive(Debug, thiserror::Error)]
pub enum OffscreenError {
    #[error(transparent)]
    Program(#[from] ProgramError),
    #[error(transparent)]
    Resolve(#[from] ResolveError),
    #[error(transparent)]
    Contract(#[from] ContractError),
    #[error(transparent)]
    Constant(#[from] ConstantError),
    #[error(transparent)]
    State(#[from] StateError),
    #[error(transparent)]
    Layout(#[from] LayoutError),
    #[error("GraphicsFormat {code} is not one this renderer draws into or samples")]
    UnsupportedFormat { code: u32 },
    #[error("render texture {0:?} has no color attachment, so it cannot be drawn over by a blit")]
    NoColorAttachment(TextureId),
    #[error(
        "{format:?} of texture {texture:?} cannot be copied into a buffer, so it cannot be read back"
    )]
    UnreadableFormat {
        texture: TextureId,
        format: wgpu::TextureFormat,
    },
    #[error("{bits} depth bits is not 0, 16, 24 or 32")]
    UnsupportedDepth { bits: u8 },
    #[error("render texture {id:?} says sRGB is {srgb} but its format is {linear} sRGB")]
    SrgbMismatch {
        id: TextureId,
        srgb: bool,
        linear: bool,
    },
    #[error("render texture {id:?} has no size, layers or mips")]
    EmptyTexture { id: TextureId },
    #[error("render texture {id:?} is 3D and {reason}")]
    VolumeShape { id: TextureId, reason: &'static str },
    #[error("render texture {id:?} is a cube and {reason}")]
    CubeShape { id: TextureId, reason: &'static str },
    #[error("render texture {id:?} has dimension code {code}, which the host has no shape for")]
    UnknownDimension { id: TextureId, code: u32 },
    #[error(
        "render texture {id:?} is {depth} deep or has that many layers and the device allows {limit}"
    )]
    DepthLimit {
        id: TextureId,
        depth: u32,
        limit: u32,
    },
    #[error("texture {0:?} is 3D, which only a kernel reads or writes, so no record can name it")]
    VolumeInGraph(TextureId),
    #[error("texture {0:?} is 3D, so its mip levels cannot be generated")]
    MipsOfVolume(TextureId),
    #[error(
        "the mip levels of texture {texture:?} cannot be generated: {format:?} is not a color \
         format the device filters as floating-point or unorm values"
    )]
    MipsOfFormat {
        texture: TextureId,
        format: wgpu::TextureFormat,
    },
    #[error("shader {0:?} is not registered")]
    UnknownShader(ShaderId),
    #[error("material {0:?} is not in the frame")]
    UnknownMaterial(MaterialId),
    #[error("material {0:?} has no shader")]
    NoShader(MaterialId),
    #[error("texture {0:?} is neither registered nor in the frame")]
    UnknownTexture(TextureId),
    #[error("{0} is not in the frame")]
    Unknown(String),
    #[error("mesh {0:?} is not registered")]
    UnknownMesh(MeshId),
    #[error("texture {0:?} is not a render texture of the frame, so it cannot be written")]
    NotRenderTexture(TextureId),
    #[error("no layer names the texture {name}")]
    MissingTexture { name: String },
    #[error("no layer names the structured buffer {name}")]
    MissingBuffer { name: String },
    #[error("the structured buffer {name} at set {set} binding {binding} {why}")]
    UnsupportedBuffer {
        name: String,
        set: u32,
        binding: u32,
        why: String,
    },
    #[error(
        "mesh {mesh:?} has no channel {channel} for the shader input {input}, which the shader \
         reads"
    )]
    MissingChannel {
        mesh: MeshId,
        channel: i8,
        input: i8,
    },
    #[error(
        "the vertex input table feeds inputs {low} and {high}, which are numbered two ways \
         (below 13 as the module's locations, from 13 as the locations plus 13)"
    )]
    MixedInputNumbering { low: i8, high: i8 },
    #[error("mesh {mesh:?}: {reason}")]
    BadMesh { mesh: MeshId, reason: String },
    #[error("a blit without a material cannot draw this source; {reason}")]
    BlitNeedsMaterial { reason: String },
    #[error("a blit with a material needs the frame to name a blit mesh")]
    NoBlitMesh,
    #[error(
        "shader {shader:?} pass {pass} reads {inputs} vertex inputs, but a procedural draw has no mesh to feed them"
    )]
    ProceduralVertexInputs {
        shader: ShaderId,
        pass: usize,
        inputs: usize,
    },
    #[error("the copy from {from:?} to {to:?} cannot be made: {reason}")]
    CopyMismatch {
        from: TextureId,
        to: TextureId,
        reason: String,
    },
    #[error("texture {texture:?} has no mip {mip} of element {element}")]
    NoSuchSubresource {
        texture: TextureId,
        mip: u32,
        element: u32,
    },
    #[error("no operation of the frame writes mip {mip} of element {element} of {texture:?}")]
    NotWritten {
        texture: TextureId,
        mip: u32,
        element: u32,
    },
    #[error("{0:?} is exported twice")]
    ExportedTwice(worldrunner_render_post::ResourceRef),
    #[error(
        "{resource:?} names mip {} of layer {}, but the export is mip {mip} of element {element}",
        .resource.mip,
        .resource.layer
    )]
    ExportNamesAnotherSubresource {
        resource: worldrunner_render_post::ResourceRef,
        mip: u32,
        element: u32,
    },
    #[error(
        "the shader reads {name} as a 2D texture and texture {texture:?} is a 2D array of {layers} layers"
    )]
    FlatSlotOverArray {
        name: String,
        texture: TextureId,
        layers: u32,
    },
    #[error(
        "the shader reads {name} as {} and texture {texture:?} is {stored}",
        slot_dimension::view_name(*.declared)
    )]
    SlotDimensionMismatch {
        name: String,
        texture: TextureId,
        declared: wgpu::TextureViewDimension,
        stored: RenderDimension,
    },
    #[error("the cells of {texture:?} that a draw samples were written at different points")]
    MixedVersions { texture: TextureId },
    #[error("set {set} binding {binding} is {what}, which offscreen draws do not bind")]
    UnsupportedBinding {
        set: u32,
        binding: u32,
        what: &'static str,
    },
    #[error(
        "a block of {len} constant bytes is larger than the {limit} bytes a buffer or a binding \
         may hold, and no split of the frame's constants makes it fit"
    )]
    ConstantsTooLarge { len: u64, limit: u64 },
    #[error(
        "the frame's constants take {needed} bytes against the runtime's budget of {budget}, and \
         no split of them makes them fit"
    )]
    ConstantsOverBudget { needed: u64, budget: u64 },
    #[error("texture {0:?} cannot be sampled and rendered into in one operation")]
    FeedbackLoop(TextureId),
    #[error("a pass names {count} color attachments and this renderer draws into one to eight")]
    ColorTargets { count: usize },
    #[error(
        "color attachment {texture:?} is {size:?} at its mip and the first color attachment, \
         {first:?}, is {first_size:?}, and the colors of a pass have one size"
    )]
    ColorTargetsDiffer {
        first: TextureId,
        first_size: [u32; 2],
        texture: TextureId,
        size: [u32; 2],
    },
    #[error("texture {texture:?} mip {mip} is more than one color attachment of one pass")]
    ColorTargetRepeated { texture: TextureId, mip: u32 },
    #[error("texture {0:?} has no color storage, so it cannot be one of several color attachments")]
    ColorTargetWithoutColor(TextureId),
}

impl OffscreenError {
    /// Whether the frame as a whole is at fault: a refusal no record or render texture causes, so
    /// building them one at a time cannot find a part to leave out.
    #[must_use]
    pub fn is_frame_wide(&self) -> bool {
        matches!(
            self,
            Self::ConstantsTooLarge { .. }
                | Self::ConstantsOverBudget { .. }
                | Self::Program(ProgramError::DeviceLost(_))
        )
    }
}

/// A frame the state refused, with the record of the frame that caused it when one did.
///
/// A refusal names a record when the state met the error while planning that record, so the record
/// is among those the frame lists. A refusal from anywhere else (a render texture, the frame's
/// constants, a read after the records) names none.
#[derive(Debug)]
pub struct Refusal {
    pub error: OffscreenError,
    /// The index in `OffscreenFrame::records` of the record the state was planning.
    pub record: Option<usize>,
}

impl Refusal {
    pub(super) fn at(record: usize, error: OffscreenError) -> Self {
        Self {
            error,
            record: Some(record),
        }
    }
}

impl From<OffscreenError> for Refusal {
    fn from(error: OffscreenError) -> Self {
        Self {
            error,
            record: None,
        }
    }
}
