//! Optional desktop presentation for a caller-owned, terrain-free scene.
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
use worldrunner_render_plan::RenderScene;
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
    /// Camera bounds; the initial pose must be within these bounds.
    pub orbit: OrbitLimits,
    /// Opt-in single-map directional shadows with an explicit caller-owned fit.
    pub shadows: Option<DirectionalShadowSettings>,
}

/// Startup, presentation or scene rendering failed.
#[derive(Debug, thiserror::Error)]
pub enum ViewerError {
    #[error("invalid viewer options: {0}")]
    InvalidOptions(&'static str),
    #[error("the terrain-free viewer does not support {0}")]
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
    validate(&scene, &options)?;
    app::run(scene, options)
}

fn validate(scene: &RenderScene, options: &ViewerOptions) -> Result<(), ViewerError> {
    if options.size.iter().any(|&side| side == 0 || side > 4096) {
        return Err(ViewerError::InvalidOptions(
            "window dimensions must be in 1..=4096",
        ));
    }
    if scene.terrain().is_some() {
        return Err(ViewerError::UnsupportedScene("terrain"));
    }
    if scene.effects().water().is_some() {
        return Err(ViewerError::UnsupportedScene("water"));
    }
    OrbitCamera::new(options.pose, options.orbit)?.view(
        *scene.effects().camera(),
        worldrunner_render_scene::Viewport {
            width: options.size[0],
            height: options.size[1],
        },
    )?;
    Ok(())
}

#[cfg(test)]
mod tests;
