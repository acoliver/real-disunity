//! Complete stock configuration replacement, including effect bindings and history.
mod encoding;
use super::{FrameOutput, FrameRenderer};
use crate::device::GpuContext;
pub use encoding::FrameEncoding;
use worldrunner_render_gpu::configuration::{
    AllocationBudget, AllocationSet, Configuration, ConfigurationError,
};
use worldrunner_render_post::{
    FrameConfig, HdrPostSettings, PostChain, PostError, Resources, TimingMode, ToneMap,
    ToneMapOutput,
};

/// One stock replacement description. Physical effect storage retains three slots;
/// `frame_slots` selects how many slots the host may admit, without aliasing them.
#[derive(Clone, Debug)]
pub struct RendererConfiguration {
    pub frame: FrameConfig,
    pub clear_color: wgpu::Color,
    pub output_format: wgpu::TextureFormat,
    pub hdr: Option<(f64, ToneMap)>,
    pub post: HdrPostSettings,
    pub ssao: Option<worldrunner_render_plan::SceneSsao>,
    pub ssr: Option<worldrunner_render_plan::SceneSsr>,
    pub water: Vec<worldrunner_render_plan::SceneWaterBody>,
    pub frame_slots: usize,
    pub timing: TimingMode,
    /// Host-owned persistent storage, staging, readbacks and CPU accumulation.
    pub additional: AllocationSet,
}
impl RendererConfiguration {
    pub fn legacy(size: [u32; 2], output_format: wgpu::TextureFormat) -> Self {
        Self {
            frame: FrameConfig::new(size, wgpu::TextureFormat::Rgba8UnormSrgb, 1),
            clear_color: wgpu::Color::BLACK,
            output_format,
            hdr: None,
            post: HdrPostSettings::default(),
            ssao: None,
            ssr: None,
            water: Vec::new(),
            frame_slots: 3,
            timing: TimingMode::Disabled,
            additional: AllocationSet::default(),
        }
    }
    pub fn allocations(&self) -> AllocationSet {
        let mut set = worldrunner_render_post::allocation::target_allocations(self.frame);
        set.extend(worldrunner_render_post::allocation::post_allocations(
            self.frame, self.post,
        ));
        screen_allocations(self, &mut set);
        if !self.water.is_empty() || self.ssr.is_some_and(|ssr| ssr.enabled) {
            let [w, h] = self.frame.size;
            set.texture(
                "snapshot color",
                [w, h, 1],
                wgpu::TextureFormat::Rgba16Float,
                [1, 1],
                3,
            );
            set.texture(
                "snapshot depths",
                [w, h, 1],
                wgpu::TextureFormat::R32Float,
                [1, 1],
                6,
            );
            set.buffer("snapshot uniforms", 304, 3);
            set.buffer("snapshot staging", 192, 3);
            if !self.water.is_empty() {
                set.texture(
                    "water receivers",
                    [w, h, 1],
                    wgpu::TextureFormat::Rgba16Float,
                    [1, self.frame.samples],
                    9,
                );
                set.texture(
                    "empty water environment",
                    [1, 1, 6],
                    wgpu::TextureFormat::Rgba16Float,
                    [1, 1],
                    1,
                );
                set.texture(
                    "empty water BRDF",
                    [1, 1, 1],
                    wgpu::TextureFormat::Rgba16Float,
                    [1, 1],
                    1,
                );
            }
            for water in &self.water {
                set.buffer("water staging", 480, 3);
                set.buffer("water body uniforms", 672, 3);
                set.buffer("water surface vertices", water_vertex_bytes(water), 1);
            }
        }
        if self.timing != TimingMode::Disabled {
            set.extend(worldrunner_render_post::allocation::timing_allocations());
        }
        set.extend(self.additional.clone());
        set
    }
    fn allocation_plan(&self, context: &GpuContext) -> AllocationSet {
        if self.timing == TimingMode::Optional
            && !context
                .device()
                .features()
                .contains(wgpu::Features::TIMESTAMP_QUERY)
        {
            let mut effective = self.clone();
            effective.timing = TimingMode::Disabled;
            effective.allocations()
        } else {
            self.allocations()
        }
    }
    fn validate(&self, context: &GpuContext) -> Result<(), PostError> {
        super::validate_clear(self.clear_color)?;
        self.frame.validate_adapter(context.adapter())?;
        self.frame.validate(&context.device().limits())?;
        self.post.validate()?;
        if self.timing == TimingMode::Required
            && !context
                .device()
                .features()
                .contains(wgpu::Features::TIMESTAMP_QUERY)
        {
            return Err(PostError::UnsupportedProfile("timestamp-query capability"));
        }
        if self.hdr.is_none() {
            worldrunner_render_post::CopyOutput::new(context.device(), self.output_format)
                .validate_config(context.device(), self.frame)?;
        }
        if !(1..=3).contains(&self.frame_slots) {
            return Err(PostError::UnsupportedProfile("frame slot count"));
        }
        if self.hdr.is_none()
            && (self.post.enabled()
                || self.ssao.is_some()
                || self.ssr.is_some()
                || !self.water.is_empty())
        {
            return Err(PostError::UnsupportedProfile("effects require HDR"));
        }
        if let Some((exposure, tone)) = self.hdr {
            self.post.validate_frame(self.frame)?;
            ToneMapOutput::new(self.output_format, exposure, tone)?;
            if self.frame.format != wgpu::TextureFormat::Rgba16Float {
                return Err(PostError::CopySourceFormat(self.frame.format));
            }
        }
        if let Some(ssao) = &self.ssao {
            crate::ssao::validate_settings(ssao)?;
        }
        if let Some(ssr) = &self.ssr {
            crate::ssr::validate_settings(ssr)?;
        }
        for water in &self.water {
            crate::water::validate_settings(water)?;
        }
        Ok(())
    }
}
fn screen_allocations(settings: &RendererConfiguration, set: &mut AllocationSet) {
    let [w, h] = settings.frame.size;
    let reflection = settings.ssr.filter(|ssr| ssr.enabled);
    if settings.ssao.is_some() || reflection.is_some() {
        set.texture(
            "prepass normals",
            [w, h, 1],
            wgpu::TextureFormat::Rgba16Float,
            [1, 1],
            1,
        );
        set.texture(
            "prepass depth",
            [w, h, 1],
            wgpu::TextureFormat::Depth32Float,
            [1, 1],
            1,
        );
        set.texture(
            "AO intermediates",
            [w, h, 1],
            wgpu::TextureFormat::R32Float,
            [1, 1],
            if settings.ssao.is_some_and(|ao| {
                ao.filter == worldrunner_render_plan::SceneSsaoFilter::SeparableBilateral
            }) {
                3
            } else {
                2
            },
        );
        screen_hierarchy_allocations(settings, set);
        set.buffer("screen uniform", 224, 1);
        set.buffer("screen staging", 224, 3);
    }
    reflection_allocations(settings, set);
}
fn screen_hierarchy_allocations(settings: &RendererConfiguration, set: &mut AllocationSet) {
    let [w, h] = settings.frame.size;
    let mut size = [w, h];
    let mut level = 0;
    loop {
        set.texture(
            "depth hierarchy",
            [size[0], size[1], 1],
            wgpu::TextureFormat::R32Float,
            [1, 1],
            1,
        );
        if level <= 6
            && settings
                .ssao
                .is_some_and(|ao| ao.kernel == worldrunner_render_plan::SceneSsaoKernel::Spiral)
        {
            set.texture(
                "averaged AO metadata",
                [size[0], size[1], 1],
                wgpu::TextureFormat::Rgba16Float,
                [1, 1],
                1,
            );
        }
        if size.contains(&0) || size == [1, 1] {
            break;
        }
        size = size.map(|side| side.div_ceil(2));
        level += 1;
    }
}
fn reflection_allocations(settings: &RendererConfiguration, set: &mut AllocationSet) {
    let [w, h] = settings.frame.size;
    if let Some(ssr) = settings.ssr.filter(|ssr| ssr.enabled) {
        set.texture(
            "reflection material",
            [w, h, 1],
            wgpu::TextureFormat::Rgba16Float,
            [1, 1],
            1,
        );
        set.texture(
            "reflection depth",
            [w, h, 1],
            wgpu::TextureFormat::Depth32Float,
            [1, 1],
            1,
        );
        let mut size = if ssr.resolution_scale_micro == 500_000 {
            [w.div_ceil(2), h.div_ceil(2)]
        } else {
            [w, h]
        };
        for _ in 0..6 {
            set.texture(
                "reflection radiance",
                [size[0], size[1], 1],
                wgpu::TextureFormat::Rgba16Float,
                [1, 1],
                1,
            );
            if size == [1, 1] {
                break;
            }
            size = size.map(|side| side.div_ceil(2));
        }
        set.buffer("reflection uniform", 160, 1);
        set.buffer("reflection staging", 160, 3);
    }
}

/// Stock renderer whose allocation changes require `reconfigure`.
/// Low-level resize, timing setup and custom registration are intentionally unavailable.
///
/// ```compile_fail
/// # use worldrunner_render_scene::compositor::ConfiguredFrameRenderer;
/// fn resize(renderer: &mut ConfiguredFrameRenderer, device: &wgpu::Device) {
///     renderer.resize(device, [1024, 1024]);
/// }
/// ```
/// ```compile_fail
/// # use worldrunner_render_scene::compositor::ConfiguredFrameRenderer;
/// fn timing(renderer: &ConfiguredFrameRenderer, device: &wgpu::Device, queue: &wgpu::Queue) {
///     renderer.configure_timing(device, queue, worldrunner_render_post::TimingMode::Required);
/// }
/// ```
/// ```compile_fail
/// # use worldrunner_render_scene::compositor::ConfiguredFrameRenderer;
/// fn register(renderer: &mut ConfiguredFrameRenderer, device: &wgpu::Device,
///             feature: Box<dyn worldrunner_render_post::FrameFeature>) {
///     renderer.register(device, feature);
/// }
/// ```
/// ```compile_fail
/// # use worldrunner_render_scene::compositor::{ConfiguredFrameRenderer, FrameRenderer};
/// fn replace(renderer: &mut ConfiguredFrameRenderer, replacement: FrameRenderer) {
///     fn low_level(renderer: &mut FrameRenderer, replacement: FrameRenderer) {
///         *renderer = replacement;
///     }
///     low_level(renderer, replacement);
/// }
/// ```
pub struct ConfiguredFrameRenderer {
    state: Configuration<FrameRenderer>,
    settings: RendererConfiguration,
    history: Option<ToneMapOutput>,
}
impl ConfiguredFrameRenderer {
    pub fn new(
        context: &GpuContext,
        settings: RendererConfiguration,
        budget: AllocationBudget,
    ) -> Result<Self, ConfigurationError<PostError>> {
        Self::new_with(context, settings, budget, |settings| {
            allocate(context, settings, None, None)
        })
    }
    fn new_with(
        context: &GpuContext,
        settings: RendererConfiguration,
        budget: AllocationBudget,
        allocate: impl FnOnce(&RendererConfiguration) -> Result<FrameRenderer, PostError>,
    ) -> Result<Self, ConfigurationError<PostError>> {
        settings
            .validate(context)
            .map_err(ConfigurationError::Allocation)?;
        let allocations = settings.allocation_plan(context);
        let required = allocations
            .validate(&context.device().limits())
            .map_err(ConfigurationError::Invalid)?;
        if required > budget.0 {
            return Err(ConfigurationError::Budget {
                required,
                budget: budget.0,
            });
        }
        let _guard = context.submission().try_replacement()?;
        let renderer = allocate(&settings);
        context.submission().check_device()?;
        let renderer = renderer.map_err(ConfigurationError::Allocation)?;
        let history = history(&renderer);
        let state = Configuration::new(
            context.submission(),
            renderer,
            allocations,
            &context.device().limits(),
            budget,
        )
        .map_err(|error| match error {
            ConfigurationError::Budget { required, budget } => {
                ConfigurationError::Budget { required, budget }
            }
            ConfigurationError::Invalid(message) => ConfigurationError::Invalid(message),
            ConfigurationError::ForeignDevice => ConfigurationError::ForeignDevice,
            ConfigurationError::Submission(error) => ConfigurationError::Submission(error),
            ConfigurationError::Allocation(never) => match never {},
        })?;
        Ok(Self {
            state,
            settings,
            history,
        })
    }
    pub fn configuration(&self) -> &RendererConfiguration {
        &self.settings
    }
    pub fn generation(&self) -> u64 {
        self.state.generation()
    }
    pub fn collect(&mut self) {
        self.state.collect();
    }
    pub fn reconfigure(
        &mut self,
        context: &GpuContext,
        settings: RendererConfiguration,
    ) -> Result<(), ConfigurationError<PostError>> {
        // Contexts pair one device with one authority; wgpu IDs may repeat across instances.
        if !self.state.uses_authority(context.submission()) {
            return Err(ConfigurationError::ForeignDevice);
        }
        if settings.timing != self.settings.timing {
            return Err(ConfigurationError::Invalid(
                "timing capability is fixed at device setup",
            ));
        }
        settings
            .validate(context)
            .map_err(ConfigurationError::Allocation)?;
        let mut allocations = settings.allocation_plan(context);
        if self
            .history
            .as_ref()
            .is_some_and(ToneMapOutput::has_effect_history)
            && !settings.post.enabled()
        {
            allocations.extend(worldrunner_render_post::allocation::exposure_allocations());
            if settings.hdr.is_some() {
                allocations.buffer("exposure staging", 16, 3);
            }
        }
        let previous = self.state.current();
        if previous.chain.registered_feature_count() != usize::from(previous.water.is_some()) {
            return Err(ConfigurationError::Invalid(
                "custom registrations require a custom replacement builder",
            ));
        }
        let post_time = previous.post_time.clone();
        let reset = previous.exposure_reset.clone();
        let timing = previous.chain.timing_state();
        let elapsed = previous.elapsed_seconds;
        let delta = previous.frame_delta;
        let history_bank = self.history.as_ref();
        let water = previous.water.clone();
        self.state
            .replace(&context.device().limits(), allocations, || {
                let mut next = allocate(context, &settings, history_bank, Some(timing))?;
                retain_water_inputs(&mut next, water.as_ref());
                next.post_time = post_time;
                next.exposure_reset = reset;
                next.elapsed_seconds = elapsed;
                next.frame_delta = delta;
                Ok(next)
            })?;
        if let Some(next) = history(self.state.current()) {
            self.history = Some(next);
        }
        self.settings = settings;
        Ok(())
    }
    pub fn select_frame_slot(&mut self, slot: usize) {
        assert!(slot < self.settings.frame_slots, "unconfigured frame slot");
        self.state.current_mut().select_frame_slot(slot);
    }
}
fn history(renderer: &FrameRenderer) -> Option<ToneMapOutput> {
    match renderer.chain.factory() {
        FrameOutput::Hdr(factory) => Some(factory.clone()),
        FrameOutput::Copy(_) => None,
    }
}
fn allocate(
    context: &GpuContext,
    settings: &RendererConfiguration,
    history: Option<&ToneMapOutput>,
    timing: Option<worldrunner_render_post::TimingState>,
) -> Result<FrameRenderer, PostError> {
    let device = context.device();
    let scope = worldrunner_render_gpu::configuration::AllocationScope::new(device);
    let result = build(context, settings, history, timing);
    scope.finish()?;
    result
}
fn build(
    context: &GpuContext,
    settings: &RendererConfiguration,
    history: Option<&ToneMapOutput>,
    timing: Option<worldrunner_render_post::TimingState>,
) -> Result<FrameRenderer, PostError> {
    let device = context.device();
    let factory = if let Some((exposure, tone)) = settings.hdr {
        FrameOutput::Hdr(
            ToneMapOutput::new(settings.output_format, exposure, tone)?.with_retained_effects(
                device,
                settings.post,
                history,
            )?,
        )
    } else {
        FrameOutput::Copy(worldrunner_render_post::CopyOutput::new(
            device,
            settings.output_format,
        ))
    };
    let mut renderer = FrameRenderer {
        chain: PostChain::new(device, settings.frame, Resources::default(), factory)?,
        water: None,
        water_state: std::rc::Rc::default(),
        snapshots: None,
        snapshot_ready: std::rc::Rc::default(),
        frame_slot: 0,
        clear_color: settings.clear_color,
        post_requested: settings.post,
        elapsed_seconds: 0.0,
        frame_delta: 0.0,
        post_time: std::rc::Rc::default(),
        exposure_reset: std::rc::Rc::default(),
        ssao: settings
            .ssao
            .map(|ssao| {
                crate::scene_metadata::SceneMetadata::new(device, settings.frame.size, ssao)
            })
            .transpose()?,
        ssr: None,
        ssao_requested: settings.ssao,
        ssr_requested: settings.ssr,
    };
    renderer.install_reflections(device, settings.frame.size)?;
    renderer.install_water_bodies(device, settings.water.clone())?;
    if renderer.ssr.is_some() && renderer.snapshots.is_none() {
        renderer.snapshots = Some(std::array::from_fn(|_| {
            crate::water::snapshots::Snapshots::new(device, settings.frame)
        }));
    }
    if let Some(timing) = timing {
        renderer.chain.restore_timing(timing);
    } else {
        renderer
            .configure_timing(device, context.queue(), settings.timing)
            .map_err(|error| PostError::Allocation(error.to_string()))?;
    }
    Ok(renderer)
}

#[cfg(test)]
#[path = "configuration_tests.rs"]
mod tests;

fn water_vertex_bytes(body: &worldrunner_render_plan::SceneWaterBody) -> u64 {
    use worldrunner_render_plan::SceneWaterSurface;
    let vertices = match &body.surface {
        SceneWaterSurface::Rectangle => 6,
        SceneWaterSurface::Mesh { indices, .. } => {
            u64::try_from(indices.len()).expect("bounded geometry")
        }
        // Each bed triangle clips to at most two surface triangles.
        SceneWaterSurface::Basin { columns, rows, .. } => u64::from(columns.saturating_sub(1))
            .saturating_mul(u64::from(rows.saturating_sub(1)))
            .saturating_mul(12),
    };
    vertices.saturating_mul(12).max(12)
}

fn retain_water_inputs(next: &mut FrameRenderer, previous: Option<&crate::water::Water>) {
    if let (Some(next), Some(previous)) = (&mut next.water, previous) {
        next.environment.clone_from(&previous.environment);
        if next.settings == previous.settings {
            next.surface_inputs.clone_from(&previous.surface_inputs);
        }
    }
}
