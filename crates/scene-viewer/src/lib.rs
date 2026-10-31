//! Desktop and request-driven windowless presentation for a caller-owned scene.
//!
//! A GPU starts only when a run mode is requested. Geometry, materials,
//! pipelines and optional shadows remain resident across frames. The caller
//! owns scene construction, camera framing and shadow fit; this host adds no
//! asset lookup, terrain policy or command-line parsing.
//!
//! Controls: left-drag orbit, wheel zoom, R/Home reset, Escape/close exit.

mod adapter;
pub use adapter::{AdapterCallback, AdapterClass, AdapterIdentity};
mod app;
pub mod cpu_profile;
mod live;
mod reflection_settings;
pub use live::{
    IngressOrigin, LiveCapture, LiveClock, LiveDriver, LiveFrame, LiveInputEvent, LiveInputSample,
    LiveOptions, LiveReceipt, LiveTick, ScheduledInput,
};
mod clock;
pub use clock::RenderClock;
mod camera;
mod capture_pool;
mod presentation;
mod presentation_budget;
pub use capture_pool::CapturePoolPolicy;
pub use presentation_budget::{PresentationBudget, PresentationBudgetPolicy};
mod window_size;
pub use presentation::planning::{PresentationCapture, PresentationPlanning, plan_presentation};
pub use window_size::WindowSizeUnit;
mod renderer;
mod retained;
mod world_edits;
pub use retained::{
    CaptureJitter, CaptureOutput, CaptureRequestError, CursorError, CursorGrabMode, KeyCode,
    LogicalKey, MouseButton, NativeKeyCode, ObjectId, PhysicalKey, RetainedCaptureOutcome,
    RetainedCaptureProducts, RetainedCaptureRecipe, RetainedDriver, RetainedEvent,
    RetainedEventHost, RetainedHost, RetainedInput, RetainedTick, RetainedUpdate, SceneObjectEdit,
    WheelDelta, WindowlessRetained,
};
pub use world_edits::{ObjectIdMap, edits_from_world_changes, edits_from_world_changes_with};

pub use camera::{OrbitCamera, OrbitLimits};
pub use wgpu::PresentMode;
use worldrunner_render_plan::{RenderScene, SceneCamera};
pub use worldrunner_render_post::{NativeRcasSettings, RcasDomain, ToneMap};
pub use worldrunner_render_scene::compositor::FrameProfile;
use worldrunner_render_scene::{CameraPose, shadows::DirectionalShadowSettings};

/// Explicit caller-owned desktop presentation settings.
#[derive(Clone, Debug)]
pub struct ViewerOptions {
    /// Presentation descriptor-byte limit, including replacement overlap. Default: 1 GiB.
    pub presentation_budget: PresentationBudgetPolicy,
    /// Retained recipe storage policy, shared by desktop and windowless hosts.
    pub capture_pool: CapturePoolPolicy,
    /// Raster camera convention; use the shared right-handed default unless selected explicitly.
    pub camera_convention: worldrunner_camera::ViewConvention,
    /// Optional startup SSR override. Requires authored scene effects and HDR.
    /// Later retained settings replace the startup policy.
    pub ssr: Option<worldrunner_render_plan::SceneSsr>,
    /// Window title; no engine/game default is inserted.
    pub title: String,
    /// Unit of the initial desktop size request. Resize events remain physical.
    pub window_size_unit: WindowSizeUnit,
    /// Initial window dimensions; each must be in `1..=4096`.
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
    /// Opt-in asynchronous GPU pass timestamps. Unsupported adapters keep rendering
    /// and report `GpuTiming::Unavailable`; disabled requests allocate no queries.
    pub gpu_timing: bool,
    /// Explicit surface mode: `Fifo`, `FifoRelaxed`, `Mailbox` or `Immediate`. Default: `Fifo`.
    /// Unsupported surface modes fail at startup. Ignored in windowless runs.
    pub present_mode: wgpu::PresentMode,
    /// Positive desired latency, default 2. wgpu clamps this hint to backend limits.
    pub desired_maximum_frame_latency: u32,
    /// Observes the actual selected adapter before GPU scene preparation.
    pub adapter_callback: Option<AdapterCallback>,
    /// Presentation scheduling; defaults to a 16 ms fixed interval.
    pub cadence: PresentationCadence,
    /// Shader clock, wall time by default.
    pub render_clock: RenderClock,
}

/// Presentation policy, independent of GPU submission admission.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PresentationCadence {
    Fixed(std::time::Duration),
    /// Redraw whenever the event loop runs, including input wakeups.
    Uncapped,
}

impl Default for PresentationCadence {
    fn default() -> Self {
        Self::Fixed(std::time::Duration::from_millis(16))
    }
}

/// Startup, presentation or scene rendering failed.
#[derive(Debug, thiserror::Error)]
pub enum ViewerError {
    #[error(transparent)]
    Capture(#[from] CaptureRequestError),
    #[error(transparent)]
    Environment(#[from] worldrunner_render_scene::sky::preparation::PreparationError),
    #[error(transparent)]
    Configuration(
        #[from]
        worldrunner_render_gpu::configuration::ConfigurationError<
            worldrunner_render_post::PostError,
        >,
    ),
    #[error("retained preparation bound {limit} reached")]
    BackPressure { limit: usize },
    #[error("live driver failed: {0}")]
    Driver(String),
    #[error(transparent)]
    Timing(#[from] worldrunner_render_post::TimingError),
    #[error(transparent)]
    Dynamic(#[from] worldrunner_render_scene::dynamic::DynamicError),
    #[error(transparent)]
    Readback(#[from] worldrunner_render_post::ReadbackError),
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
    #[error("surface does not support requested present mode {0:?}")]
    UnsupportedPresentMode(wgpu::PresentMode),
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
    let scene = reflection_settings::startup_scene(scene, &options)?;
    validate(&scene, &options)?;
    app::run(scene, options, Box::new(update))
}

pub(crate) type SceneUpdate = Box<
    dyn FnMut(
        &mut worldrunner_render_scene::draw::SceneGpu,
    ) -> Result<(), worldrunner_render_scene::draw::InstanceError>,
>;

/// Runs a caller-owned live simulation through the shared desktop viewer.
/// Call once on the main thread. The driver retains its state after this returns.
pub fn run_live<D: LiveDriver>(driver: &mut D, options: LiveOptions) -> Result<(), ViewerError> {
    if options.size.iter().any(|&side| side == 0 || side > 4096) {
        return Err(ViewerError::InvalidOptions(
            "window dimensions must be in 1..=4096",
        ));
    }
    if options.frame_interval.is_zero()
        || options.frame_interval > std::time::Duration::from_secs(1)
    {
        return Err(ViewerError::InvalidOptions(
            "frame interval must be positive and at most one second",
        ));
    }
    if options.max_frames == Some(0) {
        return Err(ViewerError::InvalidOptions("frame limit must be positive"));
    }
    if driver.clock().real_time() && options.fixed_dt.is_some() {
        return Err(ViewerError::InvalidOptions(
            "acquisition clock cannot use a fixed replay delta",
        ));
    }
    app::run_live(driver, options)
}

fn validate(scene: &RenderScene, options: &ViewerOptions) -> Result<(), ViewerError> {
    validate_presentation(options)?;
    if let PresentationCadence::Fixed(interval) = options.cadence
        && (interval.is_zero() || std::time::Instant::now().checked_add(interval).is_none())
    {
        return Err(ViewerError::InvalidOptions(
            "frame interval must be positive and representable",
        ));
    }
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

    if let Some(ssr) = scene.effects().and_then(|effects| effects.ssr()) {
        ssr.validate()
            .map_err(|error| ViewerError::Driver(error.to_string()))?;
        if matches!(options.frame_profile, FrameProfile::Legacy) {
            return Err(ViewerError::UnsupportedScene("SSR requires HDR"));
        }
    }
    if let Some(water) = scene.effects().and_then(|effects| effects.water()) {
        if matches!(options.frame_profile, FrameProfile::Legacy) {
            return Err(ViewerError::UnsupportedScene("water requires HDR"));
        }
        worldrunner_render_scene::water::validate_settings(water)?;
    }
    OrbitCamera::new(options.pose, options.orbit)?;
    worldrunner_render_scene::SceneView::with_convention(
        options.pose,
        options.camera,
        worldrunner_render_scene::Viewport {
            width: options.size[0],
            height: options.size[1],
        },
        options.camera_convention,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests;

pub use retained::DecalUpdate;

fn validate_presentation(options: &ViewerOptions) -> Result<(), ViewerError> {
    options.presentation_budget.validate()?;
    options.capture_pool.validate()?;
    if !matches!(
        options.present_mode,
        wgpu::PresentMode::Fifo
            | wgpu::PresentMode::FifoRelaxed
            | wgpu::PresentMode::Mailbox
            | wgpu::PresentMode::Immediate
    ) {
        return Err(ViewerError::InvalidOptions(
            "present mode must be Fifo, FifoRelaxed, Mailbox or Immediate",
        ));
    }
    if options.desired_maximum_frame_latency == 0 {
        return Err(ViewerError::InvalidOptions(
            "desired maximum frame latency must be positive",
        ));
    }
    Ok(())
}

pub use retained::{DrawCount, ResourceStatistics, RetainedFrameStatistics, SceneStatistics};
pub use retained::{
    FrameTiming, GpuTiming, GpuTimingUnavailable, PassId, PassSlot, PassTiming, StageId,
};
