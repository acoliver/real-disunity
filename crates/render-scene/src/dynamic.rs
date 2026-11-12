//! Pathless runtime geometry contributed to the resident canonical scene.
//!
//! Quads share canonical scene attachments, material bindings and vertex transforms.
//! They are unlit, straight-alpha blended, depth tested without depth writes, and
//! sorted by ascending layer/order with stable submission ties. Adjacent compatible
//! geometry is instanced without reordering. Texture replacement clears quads;
//! rejected contributions preserve the previous resident state.
//!
//! ```no_run
//! use worldrunner_render_scene::{device::GpuContext, draw::SceneGpu, dynamic::DynamicQuad,
//!     frame::FrameUniform, offscreen::{OffscreenTarget, RgbaImage}, renderer::ScenePipelines, SceneView};
//!
//! fn capture(context: &GpuContext, view: &SceneView) -> Result<RgbaImage, Box<dyn std::error::Error>> {
//!     let pipelines = ScenePipelines::new(context.device(), OffscreenTarget::COLOR_FORMAT);
//!     let mut gpu = SceneGpu::empty(context.device(), context.submission(), &pipelines)?;
//!     gpu.set_dynamic_quads(context.device(), context.submission(), &pipelines, &[DynamicQuad {
//!         position: [1.0, 0.0, 0.0], tint: [1.0, 0.0, 0.0, 1.0], ..DynamicQuad::default()
//!     }])?;
//!     gpu.set_frame(context.queue(), FrameUniform::unlit(view));
//!     let mut target = OffscreenTarget::new(context.device(), view.viewport())?;
//!     Ok(target.render(context, &gpu, &pipelines)?)
//! }
//! ```

mod gpu;
mod sampling;
mod unlit;
pub(crate) mod validation;
pub(crate) use gpu::{DynamicBindings, DynamicGpu, QuadTarget, SceneStaging};
pub(crate) use unlit::UnlitRuntime;

/// One unlit alpha-blended quad in canonical Z-up scene space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DynamicQuad {
    /// Canonical translation in metres.
    pub position: [f32; 3],
    /// Optional local vertices preserving full affine contributions, including shear.
    pub corners: Option<[[f32; 3]; 4]>,
    /// Triangle-list UVs, using the first three corners and three indices only.
    pub triangle_uv: Option<[[f32; 2]; 3]>,
    /// Screen overlays follow all world quads, then use their own layer/order keys.
    pub overlay: bool,
    /// Retained UI only: corners Z holds non-reversed raster depth.
    pub depth_test: bool,
    /// Canonical unit quaternion, x/y/z/w.
    pub rotation: [f32; 4],
    /// Nonuniform canonical scale, x/y/z.
    pub scale: [f32; 3],
    /// Local XZ rectangle, left/bottom/right/top in metres.
    pub rect: [f32; 4],
    /// Top-left UV rectangle, u0/v0/u1/v1. Reverse endpoints to flip.
    pub uv_rect: [f32; 4],
    /// Linear straight-alpha multiplier. RGB permits nonnegative HDR light; alpha is 0..=1.
    pub tint: [f32; 4],
    /// Overlay-only linear straight-alpha corner multipliers in BL/BR/TR/TL order.
    /// None is white. Triangles use the first three colours; all channels are 0..=1.
    pub corner_colors: Option<[[f32; 4]; 4]>,
    /// Index into the dynamic texture table; None uses opaque white.
    pub texture: Option<usize>,
    /// Ascending painter layer.
    pub sorting_layer: i32,
    /// Ascending painter order within a layer. Equal keys retain submission order.
    pub sorting_order: i32,
    /// UI hierarchy preorder within equal layer/order keys.
    pub hierarchy_order: usize,
    /// Lower-left pixel clip bounds for the post-processing UI overlay stage.
    pub clip_rect: Option<[u32; 4]>,
}

impl Default for DynamicQuad {
    fn default() -> Self {
        Self {
            position: [0.0; 3],
            corners: None,
            triangle_uv: None,
            overlay: false,
            depth_test: false,
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0; 3],
            rect: [-0.5, -0.5, 0.5, 0.5],
            uv_rect: [0.0, 0.0, 1.0, 1.0],
            tint: [1.0; 4],
            corner_colors: None,
            texture: None,
            sorting_layer: 0,
            sorting_order: 0,
            hierarchy_order: 0,
            clip_rect: None,
        }
    }
}

/// A rejected runtime contribution. Previous resident geometry remains available.
#[derive(Debug, thiserror::Error)]
pub enum DynamicError {
    /// Shared validated upload failure.
    #[error(transparent)]
    Upload(#[from] worldrunner_render_gpu::texture::UploadError),
    /// One quad contains a nonfinite or otherwise invalid field.
    #[error("dynamic quad {index} has invalid {field}")]
    InvalidQuad {
        /// Submission index.
        index: usize,
        /// Rejected field.
        field: &'static str,
    },
    /// A texture violates the shared scene resource contract.
    #[error("dynamic texture {index} violates the scene resource contract: {source}")]
    TextureDomain {
        /// Table index.
        index: usize,
        /// Shared resource validation error.
        #[source]
        source: worldrunner_render_plan::SceneValidationError,
    },
    /// A contribution exceeds an enabled device capability or resource limit.
    #[error("dynamic contribution exceeds device support: {0}")]
    DeviceLimit(&'static str),
    /// The program runtime refused to prepare the quads' draw for the pass.
    #[error("dynamic quads could not be prepared for the pass: {0}")]
    Program(#[from] worldrunner_render_program::error::ProgramError),
    /// The quads were staged for a pass other than the one the frame draws into.
    #[error("dynamic quads were not prepared for the pass they would draw into")]
    NotPrepared,
    /// A texture could not be prepared for upload.
    #[error("dynamic texture {index} could not be prepared: {source}")]
    Texture {
        /// Table index.
        index: usize,
        /// Decoder/format failure.
        #[source]
        source: crate::texture::TextureError,
    },
}

#[cfg(test)]
pub(crate) mod capture_tests;
#[cfg(test)]
mod lifetime_tests;
#[cfg(test)]
mod tests;

#[cfg(test)]
mod validation_tests;
