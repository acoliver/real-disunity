//! Errors of offscreen work. Each names what was missing or unsupported.

use worldrunner_render_post::contracts::ContractError;

use crate::error::ProgramError;
use crate::ids::{MaterialId, MeshId, ShaderId, TextureId};
use crate::unity::constants::ConstantError;
use crate::unity::layout::LayoutError;
use crate::unity::shader::ResolveError;
use crate::unity::state::StateError;

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
    #[error(
        "mesh {mesh:?} has no channel {channel} for the shader input {input}, which the shader \
         reads"
    )]
    MissingChannel {
        mesh: MeshId,
        channel: i8,
        input: i8,
    },
    #[error("mesh {mesh:?}: {reason}")]
    BadMesh { mesh: MeshId, reason: String },
    #[error("a blit without a material needs the same size and format; {reason}")]
    BlitNeedsMaterial { reason: String },
    #[error("a blit with a material needs the frame to name a blit mesh")]
    NoBlitMesh,
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
    #[error("the cells of {texture:?} that a draw samples were written at different points")]
    MixedVersions { texture: TextureId },
    #[error("set {set} binding {binding} is {what}, which offscreen draws do not bind")]
    UnsupportedBinding {
        set: u32,
        binding: u32,
        what: &'static str,
    },
    #[error("texture {0:?} cannot be sampled and rendered into in one operation")]
    FeedbackLoop(TextureId),
}
