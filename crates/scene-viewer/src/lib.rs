//! Desktop and request-driven windowless presentation for a caller-owned scene.
//!
//! A GPU starts only when a run mode is requested. Geometry, materials,
//! pipelines and optional shadows remain resident across frames. The caller
//! owns scene construction, camera framing, shadow fit and input handling; this
//! host adds no asset lookup, terrain policy or command-line parsing. A caller
//! drives it through [`RetainedHost`] and [`RetainedDriver`]; [`OrbitNavigation`]
//! turns the window's pointer and keys into orbit camera poses.

mod application_paths;
pub use application_paths::{ApplicationPathError, application_root};
mod activity;
pub use activity::ActivityReason;
mod adapter;
pub use adapter::{
    AdapterCallback, AdapterClass, AdapterIdentity, AdapterMemory, AdapterMemorySource,
    FrameSettings, InitialContent, InitialContext,
};
mod app;
mod clock;
pub mod cpu_profile;
mod reflection_settings;
pub use clock::RenderClock;
mod game_screen;
pub use game_screen::{GAME_SCREEN, GameScreen};
mod camera;
mod capture_pool;
mod navigation;
mod presentation;
mod presentation_budget;
pub use capture_pool::CapturePoolPolicy;
pub use presentation_budget::{PresentationBudget, PresentationBudgetPolicy};
mod window_size;
pub use presentation::planning::{PresentationCapture, PresentationPlanning, plan_presentation};
pub use window_size::{DisplayInfo, WindowMode, WindowSizeUnit};
mod renderer;
mod retained;
mod start;
pub use start::{SharedGpu, ViewerStart};
mod world_edits;
pub use retained::{
    AudioCommand, AudioError, AudioReply, AudioService, AudioServiceError, AudioStatus,
    AudioUnavailable, BusConfig, CameraContinuity, CameraShadowPolicy, CancelToken, CaptureJitter,
    CaptureOutput, CaptureRequestError, ClipHandle, CursorError, CursorGrabMode, DuckingDetector,
    DuckingRule, EnvelopeMode, ExistingVoices, KeyCode, LogicalKey, MAX_HOST_TIMERS, MouseButton,
    NativeKeyCode, ObjectId, PhysicalKey, PickHit, PlaybackOptions, RetainedCaptureObservation,
    RetainedCaptureOutcome, RetainedCaptureProducts, RetainedCaptureRecipe, RetainedDriver,
    RetainedEvent, RetainedEventHost, RetainedHost, RetainedInput, RetainedPickHit, RetainedTick,
    RetainedUpdate, SceneObjectEdit, TemporalHistoryReset, TimerError, TimerHandle, VoiceHandle,
    WheelDelta, WindowControlError, WindowlessRetained, WorkCompletion, WorkFailure, WorkQueue,
    WorkQueueError, WorkSubmitError, WorkTicket,
};
pub use world_edits::{ObjectIdMap, edits_from_world_changes};

pub use camera::{OrbitCamera, OrbitLimits};
pub use navigation::OrbitNavigation;
pub use wgpu::PresentMode;
use worldrunner_render_plan::{RenderScene, SceneCamera};
pub use worldrunner_render_post::{NativeRcasSettings, RcasDomain, ToneMap};
pub use worldrunner_render_scene::compositor::FrameProfile;
use worldrunner_render_scene::{CameraPose, shadows::DirectionalShadowSettings};

/// Explicit caller-owned desktop presentation settings.
#[derive(Clone, Debug)]
pub struct ViewerOptions {
    /// Platform activity reason for retained runs; defaults to "Retained scene execution".
    pub activity_reason: ActivityReason,
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
    /// Window placement: windowed (default), maximized or borderless fullscreen.
    pub window_mode: WindowMode,
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
    /// HDR frame with fixed scene exposure and tone mapping; `ToneMap::Clamp` shows the scene
    /// colour unchanged.
    pub frame_profile: FrameProfile,
    /// Maximum admitted submissions, in 1..=3. Select two for interactive presentation.
    pub frames_in_flight: usize,
    /// Opt-in asynchronous GPU pass timestamps. Unsupported adapters keep rendering
    /// and report `GpuTiming::Unavailable`; disabled requests allocate no queries.
    pub gpu_timing: bool,
    /// Surface mode. `AutoVsync` and `AutoNoVsync` let the surface choose a supported mode; a concrete
    /// mode the surface lacks fails at startup. Default: `Fifo`. Ignored in windowless runs.
    pub present_mode: wgpu::PresentMode,
    /// Positive desired latency, default 2. wgpu clamps this hint to backend limits.
    pub desired_maximum_frame_latency: u32,
    /// Drops the top mip levels of the scene's declared reducible textures to fit a share of the
    /// adapter's device-local memory (GitHub #376). `None` uploads every texture as authored.
    pub texture_tiers: Option<worldrunner_render_scene::texture::TextureTierPolicy>,
    /// Resolves optional frame settings from the selected adapter before GPU scene preparation.
    pub adapter_callback: Option<AdapterCallback>,
    /// Presentation scheduling; defaults to a 16 ms fixed interval.
    pub cadence: PresentationCadence,
    /// Shader clock, wall time by default.
    pub render_clock: RenderClock,
    /// What the game adds to the stock render: its own passes and targets, and whether the stock
    /// passes draw the scene's terrain and water. The default adds nothing.
    pub game: GameRendering,
}

/// A game's additions to the stock render of a retained scene.
///
/// The viewer registers the inventoried factories on every renderer it builds, at startup and
/// after every reconfiguration, so the game's passes take part in each frame the viewer draws and
/// its persistent targets survive replacement. The viewer itself knows nothing of what they draw.
#[derive(Clone, Debug, Default)]
pub struct GameRendering {
    /// The game's feature factories and the targets they declare. A factory that draws the
    /// scene's terrain surface or water itself declares it, and the stock passes then skip it.
    pub passes: worldrunner_render_scene::compositor::GamePasses,
}

/// Presentation policy, independent of GPU submission admission.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PresentationCadence {
    /// Start a frame once per interval, counted from frame start to frame start. A frame that
    /// takes longer than the interval is followed at once by the next.
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
    #[error("initial content supplier failed: {0}")]
    InitialContent(String),
    #[error("initial scene is unavailable before selected-adapter startup")]
    InitialSceneUnavailable,
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
    #[error("driver failed: {0}")]
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
    #[error(
        "selected adapter backend {backend:?} is excluded by the effective instance backend mask {enabled:?}"
    )]
    SelectedBackendDisallowed {
        backend: wgpu::Backend,
        enabled: wgpu::Backends,
    },
    #[error("selected adapter backend {backend:?} was absent from presentability observations")]
    SelectedBackendMissing { backend: wgpu::Backend },
    #[error(transparent)]
    Device(#[from] worldrunner_render_scene::device::DeviceError),
    #[error("the player's offscreen work could not join the frame: {0}")]
    Offscreen(String),
    #[error("the shared GPU adapter {adapter} cannot present to the window's surface")]
    SharedSurface { adapter: String },
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

fn validate(scene: &RenderScene, options: &ViewerOptions) -> Result<(), ViewerError> {
    validate_presentation(options)?;
    if options.activity_reason.as_ref().is_empty() {
        return Err(ViewerError::InvalidOptions(
            "activity reason must be nonempty",
        ));
    }
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
    }
    if let Some(water) = scene.effects().and_then(|effects| effects.water()) {
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
    if options.desired_maximum_frame_latency == 0 {
        return Err(ViewerError::InvalidOptions(
            "desired maximum frame latency must be positive",
        ));
    }
    Ok(())
}

pub use retained::{
    CensusSnapshot, DrawCount, EffectEnabled, EffectStatistics, ObjectDraws, ObjectRecording,
    PassExecution, PassInstallation, ReceiverBinding, ReceiverBindings, ResourceClass,
    ResourceStatistics, RetainedFrameStatistics, SceneStatistics, ShadowStatistics, SsaoStatistics,
    StagingStatistics, TerrainGeometryStatistics, TerrainLeafIdentity, TerrainPageStatistics,
    TerrainStreamingStatistics, TextureTierPolicy, TierChoice,
};
pub use retained::{
    FrameTiming, GpuTiming, GpuTimingUnavailable, PassEntry, PassId, PassSlot, PassTiming, StageId,
};
