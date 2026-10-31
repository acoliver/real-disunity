//! Complete stock configuration replacement, including effect bindings and history.
mod encoding;
use super::{FrameOutput, FrameRenderer};
use crate::device::GpuContext;
pub use encoding::FrameEncoding;
use worldrunner_render_gpu::configuration::{
    AllocationBudget, AllocationBudgetPolicy, AllocationSet, Configuration, ConfigurationError,
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
    /// Request-owned replay targets, charged to the host capture reservation.
    pub fn capture_term_allocations(size: [u32; 2]) -> AllocationSet {
        super::terms::TermCapture::allocations(size)
    }

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
                crate::water::allocations(water, &mut set);
            }
        }
        if self.timing != TimingMode::Disabled {
            set.extend(worldrunner_render_post::allocation::timing_allocations());
        }
        set.extend(self.additional.clone());
        set
    }
    /// Includes exposure storage preserved by an earlier post-processing generation.
    pub fn allocations_with_history(&self, retained_effect_history: bool) -> AllocationSet {
        let mut allocations = self.allocations();
        if retained_effect_history && !self.post.enabled() {
            allocations.extend(worldrunner_render_post::allocation::exposure_allocations());
            if self.hdr.is_some() {
                allocations.buffer("exposure staging", 16, 3);
            }
        }
        allocations
    }
    fn allocation_plan(
        &self,
        context: &GpuContext,
        retained_effect_history: bool,
    ) -> AllocationSet {
        if self.timing == TimingMode::Optional
            && !context
                .device()
                .features()
                .contains(wgpu::Features::TIMESTAMP_QUERY)
        {
            let mut effective = self.clone();
            effective.timing = TimingMode::Disabled;
            effective.allocations_with_history(retained_effect_history)
        } else {
            self.allocations_with_history(retained_effect_history)
        }
    }
    fn validate(&self, context: &GpuContext) -> Result<(), PostError> {
        super::validate_clear(self.clear_color)?;
        self.frame.validate_adapter(context.adapter())?;
        self.frame.validate_device(context.device())?;
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
            if ssr.enabled {
                crate::ssr::validate_extent(self.frame, ssr, &context.device().limits())?;
            }
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
        occlusion_allocations(settings, set);
        screen_hierarchy_allocations(settings, set);
        set.buffer("screen uniform", 224, 1);
        set.buffer("screen staging", 224, 3);
    }
    reflection_allocations(settings, set);
}
fn occlusion_allocations(settings: &RendererConfiguration, set: &mut AllocationSet) {
    let [w, h] = settings.frame.size;
    let scalable = settings.ssao.is_some_and(|ao| {
        matches!(
            ao.kernel,
            worldrunner_render_plan::SceneSsaoKernel::Scalable { .. }
        )
    });
    let footprint = settings.ssao.map(|ao| ao.depth_footprint);
    let size = if scalable {
        let [width, height] = footprint
            .expect("scalable settings")
            .rounding
            .half_extent([w, h]);
        [width, height, 1]
    } else {
        [w, h, 1]
    };
    let gather = settings
        .ssao
        .is_some_and(|ao| crate::ssao::receiver::gathers(ao.receiver));
    set.texture(
        "AO intermediates",
        size,
        if scalable {
            wgpu::TextureFormat::Rg16Float
        } else {
            wgpu::TextureFormat::R32Float
        },
        [1, 1],
        (if settings.ssao.is_some_and(|ao| {
            !matches!(
                ao.filter,
                worldrunner_render_plan::SceneSsaoFilter::DepthAwareBox
            )
        }) {
            3
        } else {
            2
        }) - u32::from(gather),
    );
    if gather {
        set.texture(
            "AO receiver visibility and depth",
            size,
            wgpu::TextureFormat::Rg32Float,
            [1, 1],
            1,
        );
    }
    if scalable {
        for extent in footprint.expect("scalable settings").extents([w, h]) {
            set.texture(
                "averaged AO depth",
                [extent[0], extent[1], 1],
                wgpu::TextureFormat::R32Float,
                [1, 1],
                1,
            );
        }
    }
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
            "empty reflection environment",
            [1, 1, 6],
            wgpu::TextureFormat::Rgba16Float,
            [1, 1],
            1,
        );
        set.texture(
            "empty reflection LUT",
            [1, 1, 1],
            wgpu::TextureFormat::Rgba16Float,
            [1, 1],
            1,
        );
        set.texture(
            "forward reflection products",
            [w, h, 1],
            wgpu::TextureFormat::Rgba16Float,
            [1, settings.frame.samples],
            2,
        );
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
        radiance_allocations(settings.frame, &ssr, set);
        reflection_depth_allocations([w, h], crate::ssr::depth_level_count(&ssr, [w, h]), set);
        set.buffer("reflection uniform", 192, 1);
        set.buffer("environment intensity", 16, 1);
        set.buffer("reflection staging", 192, 3);
    }
}

fn radiance_allocations(
    config: FrameConfig,
    ssr: &worldrunner_render_plan::SceneSsr,
    set: &mut AllocationSet,
) {
    let extents = ssr.radiance_extents(config.size);
    if ssr.radiance_pyramid == worldrunner_render_plan::SceneSsrPyramid::Full {
        let [w, h] = crate::ssr::pyramid::storage_extent(config, ssr)
            .expect("validated reflection storage extent");
        let levels = u32::try_from(extents.len()).expect("bounded reflection levels");
        set.texture(
            "reflection radiance",
            [w, h, 1],
            wgpu::TextureFormat::Rgba16Float,
            [levels, 1],
            1,
        );
        if levels > 1 {
            set.buffer("reflection logical extents", 16, levels - 1);
        }
    } else {
        for size in extents {
            set.texture(
                "reflection radiance",
                [size[0] * config.samples, size[1], 1],
                wgpu::TextureFormat::Rgba16Float,
                [1, 1],
                1,
            );
        }
    }
}

fn reflection_depth_allocations(mut size: [u32; 2], count: usize, set: &mut AllocationSet) {
    for _ in 0..count {
        set.texture(
            "final reflection depth",
            [size[0], size[1], 1],
            wgpu::TextureFormat::R32Float,
            [1, 1],
            1,
        );
        if size.contains(&0) || size == [1, 1] {
            break;
        }
        size = size.map(|side| side.div_ceil(2));
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
        Self::new_with_policy(context, settings, budget.into())
    }
    pub fn new_with_policy(
        context: &GpuContext,
        settings: RendererConfiguration,
        budget: AllocationBudgetPolicy,
    ) -> Result<Self, ConfigurationError<PostError>> {
        Self::new_with(context, settings, budget, |settings| {
            allocate(context, settings, None, None)
        })
    }
    fn new_with(
        context: &GpuContext,
        settings: RendererConfiguration,
        budget: impl Into<AllocationBudgetPolicy>,
        allocate: impl FnOnce(&RendererConfiguration) -> Result<FrameRenderer, PostError>,
    ) -> Result<Self, ConfigurationError<PostError>> {
        let budget = budget.into();
        settings
            .validate(context)
            .map_err(ConfigurationError::Allocation)?;
        let allocations = settings.allocation_plan(context, false);
        let required = allocations
            .validate(&context.device().limits())
            .map_err(ConfigurationError::Invalid)?;
        budget.resolve::<PostError>(required)?;
        let _guard = context.submission().try_replacement()?;
        let renderer = allocate(&settings);
        context.submission().check_device()?;
        let renderer = renderer.map_err(ConfigurationError::Allocation)?;
        let history = history(&renderer);
        let state = Configuration::new_with_policy(
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
            ConfigurationError::TextureArrayLayers {
                required,
                available,
            } => ConfigurationError::TextureArrayLayers {
                required,
                available,
            },
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
    /// Test harnesses that rasterise their own scene need the stock encoder and `prepare_uniform`,
    /// which the restricted public surface does not expose.
    #[cfg(test)]
    pub(super) fn renderer_mut(&mut self) -> &mut FrameRenderer {
        self.state.current_mut()
    }
    pub fn configuration(&self) -> &RendererConfiguration {
        &self.settings
    }
    pub fn budget(&self) -> AllocationBudget {
        self.state.budget()
    }
    /// Admits independently owned work without replacing targets or temporal history.
    pub fn reserve(
        &mut self,
        context: &GpuContext,
        allocations: AllocationSet,
    ) -> Result<
        worldrunner_render_gpu::configuration::AllocationReservation,
        ConfigurationError<PostError>,
    > {
        if !self.state.uses_authority(context.submission()) {
            return Err(ConfigurationError::ForeignDevice);
        }
        self.state.reserve(&context.device().limits(), allocations)
    }
    /// Complete live target and independently reserved storage.
    pub fn resident_bytes(&self, limits: &wgpu::Limits) -> Result<u64, &'static str> {
        self.state.resident_bytes(limits)
    }
    pub fn generation(&self) -> u64 {
        self.state.generation()
    }
    /// Whether replacement must retain exposure storage from prior effects.
    pub fn has_effect_history(&self) -> bool {
        self.history
            .as_ref()
            .is_some_and(ToneMapOutput::has_effect_history)
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
        let allocations = settings.allocation_plan(context, self.has_effect_history());
        let previous = self.state.current();
        if previous.chain.registered_feature_count() != usize::from(previous.water.is_some()) {
            return Err(ConfigurationError::Invalid(
                "custom registrations require a custom replacement builder",
            ));
        }
        let post_time = previous.post_time.clone();
        let reset = previous.exposure_reset.clone();
        let temporal = previous.temporal.clone();
        let timing = previous.chain.timing_state();
        let (elapsed, animation_time) = (previous.elapsed_seconds, previous.animation_time);
        let (selected_time, delta) = (previous.selected_time, previous.frame_delta);
        let history_bank = self.history.as_ref();
        let water = previous.water.clone();
        let environment_conventions = previous.environment_conventions;
        let reflection = previous.ssr.as_ref().map(ReflectionInputs::new);
        self.state
            .replace(&context.device().limits(), allocations, || {
                let mut next = allocate(context, &settings, history_bank, Some(timing))?;
                retain_water_inputs(&mut next, water.as_ref());
                retain_reflection_inputs(context.device(), &mut next, reflection.as_ref());
                next.retain_environment_conventions(environment_conventions);
                next.post_time = post_time;
                next.exposure_reset = reset;
                next.temporal = temporal;
                install_jitter(&mut next, &settings)?;
                next.reset_temporal_history();
                next.elapsed_seconds = elapsed;
                next.animation_time = animation_time;
                next.selected_time = selected_time;
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
/// Selects the jitter the settings imply: the default sequence under native TAA, the sequence
/// derived from both extents under reduced temporal reconstruction, none otherwise.
fn install_jitter(
    renderer: &mut FrameRenderer,
    settings: &RendererConfiguration,
) -> Result<(), PostError> {
    renderer.set_projection_jitter(crate::post::jitter(&settings.post, settings.frame, None)?)
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
        environment_conventions: [None; 2],
        water: None,
        water_state: std::rc::Rc::default(),
        snapshots: None,
        snapshot_ready: std::rc::Rc::default(),
        frame_slot: 0,
        clear_color: settings.clear_color,
        post_requested: settings.post,
        elapsed_seconds: 0.0,
        selected_time: None,
        animation_time: None,
        frame_delta: 0.0,
        post_time: std::rc::Rc::default(),
        exposure_reset: std::rc::Rc::default(),
        temporal: std::rc::Rc::default(),
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
    install_jitter(&mut renderer, settings)?;
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

fn retain_water_inputs(next: &mut FrameRenderer, previous: Option<&crate::water::Water>) {
    if let (Some(next), Some(previous)) = (&mut next.water, previous) {
        next.environment.clone_from(&previous.environment);
        next.environment_intensity_micro = previous.environment_intensity_micro;
        if next.settings == previous.settings {
            next.surface_inputs.clone_from(&previous.surface_inputs);
        }
    }
}

struct ReflectionInputs {
    intensity_micro: u32,
    environment: [wgpu::TextureView; 2],
    composition: worldrunner_render_lighting::reflections::Composition,
    selected: worldrunner_render_plan::SceneSsrComposition,
}
impl ReflectionInputs {
    fn new(ssr: &crate::ssr::Ssr) -> Self {
        Self {
            environment: ssr.environment.clone(),
            intensity_micro: ssr.environment_intensity_micro,
            composition: ssr.composition,
            selected: ssr.settings.composition,
        }
    }
}
fn retain_reflection_inputs(
    device: &wgpu::Device,
    next: &mut FrameRenderer,
    previous: Option<&ReflectionInputs>,
) {
    if let (Some(ssr), Some(screen), Some(previous)) = (&mut next.ssr, &mut next.ssao, previous) {
        ssr.environment.clone_from(&previous.environment);
        ssr.environment_intensity_micro = previous.intensity_micro;
        if ssr.settings.composition == previous.selected {
            ssr.composition = previous.composition;
        }
        let group = crate::ssr::forward::group(
            device,
            screen,
            &previous.environment,
            previous.intensity_micro,
        );
        screen
            .geometry
            .forward
            .as_mut()
            .expect("forward reflection metadata")
            .1 = group;
    }
}
