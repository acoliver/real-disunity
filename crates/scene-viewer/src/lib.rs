//! Optional desktop presentation for a caller-owned scene.
//!
//! No window or GPU is started until [`run`] is called. Geometry, materials,
//! pipelines and optional shadows remain resident across frames. The caller
//! owns scene construction, camera framing and shadow fit; this host adds no
//! asset lookup, terrain policy, command-line parsing or readback.
//!
//! Controls: left-drag orbit, wheel zoom, R/Home reset, Escape/close exit.

mod app;
mod camera;
mod presentation;
mod renderer;

pub use camera::{OrbitCamera, OrbitLimits};
use worldrunner_render_plan::{RenderScene, SceneCamera};
pub use worldrunner_render_post::ToneMap;
pub use worldrunner_render_scene::compositor::FrameProfile;
use worldrunner_render_scene::{CameraPose, shadows::DirectionalShadowSettings};

/// Explicit caller-owned desktop presentation settings.
#[derive(Clone, Debug)]
pub struct ViewerOptions {
    /// Window title; no engine/game default is inserted.
    pub title: String,
    /// Initial logical-pixel dimensions; each must be in `1..=4096`.
    pub size: [u32; 2],
    /// Initial Z-up scene-space camera pose, restored exactly by R/Home.
    pub pose: CameraPose,
    /// Projection intrinsics, independent of an optional environmental profile.
    pub camera: SceneCamera,
    /// Camera bounds; the initial pose must be within these bounds.
    pub orbit: OrbitLimits,
    /// Opt-in directional shadows with scene cascade policy and caller-owned coverage focus.
    pub shadows: Option<DirectionalShadowSettings>,
    /// Legacy copy output or explicit HDR with fixed scene exposure and tone mapping.
    pub frame_profile: FrameProfile,
    /// Maximum admitted submissions, in 1..=3. Select two for interactive presentation.
    pub frames_in_flight: usize,
}

/// Startup, presentation or scene rendering failed.
#[derive(Debug, thiserror::Error)]
pub enum ViewerError {
    #[error(transparent)]
    Post(#[from] worldrunner_render_post::PostError),
    #[error("invalid viewer options: {0}")]
    InvalidOptions(&'static str),
    #[error("the viewer does not support {0}")]
    UnsupportedScene(&'static str),
    #[error(transparent)]
    View(#[from] worldrunner_render_scene::ViewError),
    #[error(transparent)]
    EventLoop(#[from] winit::error::EventLoopError),
    #[error(transparent)]
    Window(#[from] winit::error::OsError),
    #[error(transparent)]
    Surface(#[from] wgpu::CreateSurfaceError),
    #[error("no presentation-compatible GPU adapter: {0}")]
    Adapter(#[from] wgpu::RequestAdapterError),
    #[error(transparent)]
    Device(#[from] worldrunner_render_scene::device::DeviceError),
    #[error("surface has no supported sRGB presentation format")]
    SurfaceFormat,
    #[error("physical surface extent {size:?} exceeds device dimension limit {device_limit}")]
    SurfaceExtent { size: [u32; 2], device_limit: u32 },
    #[error("surface validation failed")]
    SurfaceValidation,
    #[error("GPU error: {0}")]
    Gpu(String),
    #[error(transparent)]
    Schedule(#[from] worldrunner_render_post::FrameError),
    #[error(transparent)]
    Instance(#[from] worldrunner_render_scene::draw::InstanceError),
    #[error(transparent)]
    Draw(#[from] worldrunner_render_scene::draw::DrawError),
    #[error(transparent)]
    Frame(#[from] worldrunner_render_scene::frame::FrameError),
    #[error(transparent)]
    Shadow(#[from] worldrunner_render_scene::shadows::ShadowError),
}

/// Runs a desktop event loop until Escape, close, or a typed failure.
///
/// Call on the main thread, once per process, only for explicit window mode.
/// Invalid options and unsupported scene features fail before display/GPU startup.
/// Native surface extents follow physical pixels; unsupported device dimensions
/// return an error. Scene attachments downscale uniformly to at most 4096 pixels
/// per side. Zero-area windows suspend rendering. Lost surfaces are recreated,
/// outdated surfaces reconfigured, and timeouts retried without busy-spinning. The scene is never reuploaded per frame.
pub fn run(scene: RenderScene, options: ViewerOptions) -> Result<(), ViewerError> {
    run_with_updates(scene, options, |_| Ok(()))
}

/// Runs the viewer with a placement-edit callback before each rendered frame.
/// The callback sees the same resident scene in plain and shadowed modes.
/// Handles obtained on its first call remain valid until explicitly removed.
/// Edits are flushed automatically; returning an error exits the viewer.
pub fn run_with_updates(
    scene: RenderScene,
    options: ViewerOptions,
    update: impl FnMut(
        &mut worldrunner_render_scene::draw::SceneGpu,
    ) -> Result<(), worldrunner_render_scene::draw::InstanceError>
    + 'static,
) -> Result<(), ViewerError> {
    validate(&scene, &options)?;
    app::run(scene, options, Box::new(update))
}

pub(crate) type SceneUpdate = Box<
    dyn FnMut(
        &mut worldrunner_render_scene::draw::SceneGpu,
    ) -> Result<(), worldrunner_render_scene::draw::InstanceError>,
>;

fn validate(scene: &RenderScene, options: &ViewerOptions) -> Result<(), ViewerError> {
    if !(1..=3).contains(&options.frames_in_flight) {
        return Err(ViewerError::InvalidOptions(
            "frames in flight must be in 1..=3",
        ));
    }
    if options.size.iter().any(|&side| side == 0 || side > 4096) {
        return Err(ViewerError::InvalidOptions(
            "window dimensions must be in 1..=4096",
        ));
    }
    if scene
        .effects()
        .is_some_and(|effects| effects.water().is_some())
    {
        return Err(ViewerError::UnsupportedScene("water"));
    }
    OrbitCamera::new(options.pose, options.orbit)?.view(
        options.camera,
        worldrunner_render_scene::Viewport {
            width: options.size[0],
            height: options.size[1],
        },
    )?;
    Ok(())
}

#[cfg(test)]
mod tests;
