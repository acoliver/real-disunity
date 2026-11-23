//! Complete stock configuration replacement, including effect bindings and history.
mod build;
mod encoding;
use build::{Carried, allocate};
mod retained;
mod snapshots;
use super::{FrameOutput, FrameRenderer};
use crate::device::GpuContext;
pub use encoding::FrameEncoding;
use retained::{ReflectionInputs, retain_reflection_inputs, retain_water_inputs};
use worldrunner_render_gpu::configuration::{
    AllocationBudget, AllocationBudgetPolicy, AllocationSet, Configuration, ConfigurationError,
};
use worldrunner_render_post::{
    FrameConfig, HdrPostSettings, PostError, TimingMode, ToneMap, ToneMapOutput,
};

/// One stock replacement description. Physical effect storage retains three slots;
/// `frame_slots` selects how many slots the host may admit, without aliasing them.
#[derive(Clone, Debug)]
pub struct RendererConfiguration {
    pub frame: FrameConfig,
    pub clear_color: wgpu::Color,
    pub output_format: wgpu::TextureFormat,
    /// The fixed exposure and the display curve the output stage applies to the
    /// `Rgba16Float` frame.
    pub hdr: (f64, ToneMap),
    pub post: HdrPostSettings,
    pub ssao: Option<worldrunner_render_plan::SceneSsao>,
    pub ssr: Option<worldrunner_render_plan::SceneSsr>,
    /// Shadow depth fitting reduces the depth and normal prepass, which this frame then owns
    /// and pairs with Opaque, as for SSAO and SSR.
    pub depth_fitting: Option<worldrunner_render_plan::SceneDepthFitting>,
    /// Upper bound for dynamically present shadow receiver displacements.
    pub receiver_gradients: super::ReceiverGradientReservation,
    pub water: Vec<worldrunner_render_plan::SceneWaterBody>,
    /// Reserve the existing frame-depth snapshots for depth-tested UI.
    pub ui_depth: bool,
    /// Reserve slot-local visible-object ID targets. Storage is allocated only for live IDs.
    pub contour_ids: bool,
    pub frame_slots: usize,
    pub timing: TimingMode,
    /// Host-owned persistent storage, staging, readbacks and CPU accumulation.
    pub additional: AllocationSet,
    /// The game's inventoried feature factories. Each is registered on every renderer generation
    /// this configuration builds, on allocation and after every replacement. Empty by default,
    /// which leaves every frame as the stock frame.
    pub game_passes: super::GamePasses,
}
impl RendererConfiguration {
    /// Request-owned replay targets, charged to the host capture reservation.
    pub fn capture_term_allocations(size: [u32; 2]) -> AllocationSet {
        super::terms::TermCapture::allocations(size)
    }

    /// A single-sample `Rgba16Float` frame of `size` with unit exposure, `tone_map` and no
    /// effects, written to `output_format`.
    pub fn new(size: [u32; 2], output_format: wgpu::TextureFormat, tone_map: ToneMap) -> Self {
        Self {
            frame: FrameConfig::new(size, wgpu::TextureFormat::Rgba16Float, 1),
            clear_color: wgpu::Color::BLACK,
            output_format,
            hdr: (1.0, tone_map),
            post: HdrPostSettings::default(),
            ssao: None,
            ssr: None,
            depth_fitting: None,
            receiver_gradients: super::ReceiverGradientReservation::Opposing,
            water: Vec::new(),
            ui_depth: false,
            contour_ids: false,
            frame_slots: 3,
            timing: TimingMode::Disabled,
            additional: AllocationSet::default(),
            game_passes: super::GamePasses::default(),
        }
    }
    pub fn allocations(&self) -> AllocationSet {
        self.allocations_after(None)
    }
    /// The inventory of one generation when it replaces the one whose game targets are `previous`.
    fn allocations_after(&self, previous: Option<&super::game::TargetStore>) -> AllocationSet {
        let mut set = worldrunner_render_post::allocation::target_allocations(self.frame);
        set.extend(worldrunner_render_post::allocation::post_allocations(
            self.frame, self.post,
        ));
        screen_allocations(self, &mut set);
        if self.contour_ids {
            set.texture(
                "contour IDs",
                [self.frame.size[0], self.frame.size[1], 1],
                crate::contours::FORMAT,
                [1, self.frame.samples],
                3,
            );
        }
        snapshots::allocations(self, &mut set);
        if self.timing != TimingMode::Disabled {
            set.extend(worldrunner_render_post::allocation::timing_allocations());
        }
        set.extend(self.game_passes.allocations(self.frame.size, previous));
        set.extend(self.additional.clone());
        set
    }
    /// Includes exposure storage preserved by an earlier post-processing generation.
    pub fn allocations_with_history(&self, retained_effect_history: bool) -> AllocationSet {
        self.inventory_after(retained_effect_history, None)
    }
    fn inventory_after(
        &self,
        retained_effect_history: bool,
        previous: Option<&super::game::TargetStore>,
    ) -> AllocationSet {
        let mut allocations = self.allocations_after(previous);
        if retained_effect_history && !self.post.enabled() {
            allocations.extend(worldrunner_render_post::allocation::exposure_allocations());
            allocations.buffer("exposure staging", 16, 3);
        }
        allocations
    }
    fn allocation_plan(
        &self,
        context: &GpuContext,
        retained_effect_history: bool,
        previous: Option<&super::game::TargetStore>,
    ) -> AllocationSet {
        if self.timing == TimingMode::Optional
            && !context
                .device()
                .features()
                .contains(wgpu::Features::TIMESTAMP_QUERY)
        {
            let mut effective = self.clone();
            effective.timing = TimingMode::Disabled;
            effective.inventory_after(retained_effect_history, previous)
        } else {
            self.inventory_after(retained_effect_history, previous)
        }
    }
    fn prepass_metadata(&self) -> Option<super::depth_reuse::Prepass> {
        super::depth_reuse::prepass_metadata(
            self.ssao,
            self.ssr.as_ref(),
            self.depth_fitting.is_some(),
        )
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
        if !(1..=3).contains(&self.frame_slots) {
            return Err(PostError::UnsupportedProfile("frame slot count"));
        }
        self.game_passes.validate(
            self.frame.size,
            (&context.device().limits(), context.device().features()),
        )?;
        if self.contour_ids {
            crate::contours::validate_format(
                context
                    .adapter()
                    .get_texture_format_features(crate::contours::FORMAT),
                self.frame.samples,
            )?;
        }
        self.post.validate_frame(self.frame)?;
        ToneMapOutput::new(self.output_format, self.hdr.0, self.hdr.1)?;
        if self.frame.format != wgpu::TextureFormat::Rgba16Float {
            return Err(PostError::CopySourceFormat(self.frame.format));
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
    if settings.prepass_metadata().is_some() {
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
        if settings.frame.samples == 1 {
            if let Some(receiver) = settings.receiver_gradients.layout() {
                set.extend(receiver.allocations(settings.frame.size));
            }
            // Combined scene depth costs two float planes; relief also retains its own prepass.
            set.texture(
                "scene depth ownership reserve",
                [w, h, 1],
                wgpu::TextureFormat::Depth32Float,
                [1, 1],
                1,
            );
        }
        // Depth-only frames keep every stage for features that declare reads of them, and
        // add the constant occlusion texel their receivers bind.
        if settings.prepass_metadata() == Some(super::depth_reuse::Prepass::DepthOnly) {
            set.texture(
                "neutral ambient occlusion",
                [1, 1, 1],
                wgpu::TextureFormat::R32Float,
                [1, 1],
                1,
            );
        }
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
        radiance_allocations(settings.frame, &ssr, set);
        reflection_depth_allocations([w, h], crate::ssr::depth_level_count(&ssr, [w, h]), set);
        set.buffer("reflection uniform", 224, 1);
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
            [w, h, crate::ssr::pyramid::bank_count(ssr)],
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
                [
                    size[0] * config.samples,
                    size[1],
                    crate::ssr::pyramid::bank_count(ssr),
                ],
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
/// Low-level resize, timing setup and ad-hoc registration are intentionally unavailable; a game's
/// features enter through [`RendererConfiguration::game_passes`], which registers them on every
/// generation.
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
    history: ToneMapOutput,
}
impl ConfiguredFrameRenderer {
    /// `budget` is a fixed [`AllocationBudget`] or a policy that resolves one from the
    /// required bytes.
    pub fn new(
        context: &GpuContext,
        settings: RendererConfiguration,
        budget: impl Into<AllocationBudgetPolicy>,
    ) -> Result<Self, ConfigurationError<PostError>> {
        Self::new_with(context, settings, budget, |settings| {
            allocate(context, settings, Carried::default())
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
        let allocations = settings.allocation_plan(context, false, None);
        let required = allocations
            .validate(&context.device().limits())
            .map_err(ConfigurationError::Invalid)?;
        budget.resolve::<PostError>(required)?;
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
        })?
        .counted(
            worldrunner_render_gpu::census::ResourceClass::RenderTarget,
            &context.device().limits(),
        );
        Ok(Self {
            state,
            settings,
            history,
        })
    }
    /// Test harnesses that rasterise their own scene need the stock encoder and `prepare_uniform`,
    /// which the restricted public surface does not expose.
    #[cfg(test)]
    pub(crate) fn renderer_mut(&mut self) -> &mut FrameRenderer {
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
    /// What the stock frame plan cache did, across the renderers this configuration replaced.
    pub fn stock_plan_statistics(&self) -> crate::compositor::StockPlanStatistics {
        self.state.current().stock_plan_statistics()
    }
    /// How much opaque snapshot storage the live generation holds. A renderer with no water,
    /// reflections, depth-tested overlay or game that reads snapshots holds none.
    pub fn snapshot_statistics(&self) -> crate::compositor::SnapshotStatistics {
        self.state.current().snapshot_statistics()
    }
    pub fn generation(&self) -> u64 {
        self.state.generation()
    }
    /// Whether replacement must retain exposure storage from prior effects.
    pub fn has_effect_history(&self) -> bool {
        self.history.has_effect_history()
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
        let previous = self.state.current();
        let game = previous.game.store.clone();
        let allocations =
            settings.allocation_plan(context, self.has_effect_history(), game.as_deref());
        if previous.chain.registered_feature_count()
            != usize::from(previous.stock_water().is_some()) + previous.game.features
        {
            return Err(ConfigurationError::Invalid(
                "custom registrations require a custom replacement builder",
            ));
        }
        let plan_cache = previous.plan_cache.clone();
        let post_time = previous.post_time.clone();
        let reset = previous.exposure_reset.clone();
        let temporal = previous.temporal.clone();
        let timing = previous.chain.timing_state();
        let (elapsed, animation_time) = (previous.elapsed_seconds, previous.animation_time);
        let (selected_time, delta) = (previous.selected_time, previous.frame_delta);
        let history_bank = Some(&self.history);
        let water = previous.water.clone();
        let environment_conventions = previous.environment_conventions;
        let reflection = previous.ssr.as_ref().map(ReflectionInputs::new);
        self.state
            .replace(&context.device().limits(), allocations, || {
                let carried = Carried {
                    history: history_bank,
                    timing: Some(timing),
                    game: game.as_deref(),
                };
                let mut next = allocate(context, &settings, carried)?;
                retain_water_inputs(&mut next, water.as_ref());
                retain_reflection_inputs(context.device(), &mut next, reflection.as_ref());
                next.retain_environment_conventions(environment_conventions);
                next.plan_cache = plan_cache;
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
        self.history = history(self.state.current());
        self.settings = settings;
        Ok(())
    }
    pub fn select_frame_slot(&mut self, slot: usize) {
        assert!(slot < self.settings.frame_slots, "unconfigured frame slot");
        self.state.current_mut().select_frame_slot(slot);
    }
}
fn history(renderer: &FrameRenderer) -> ToneMapOutput {
    match renderer.chain.factory() {
        FrameOutput::Hdr(factory) => factory.clone(),
        FrameOutput::Copy(_) => unreachable!("a configured renderer builds an HDR output"),
    }
}
/// Selects the jitter the settings imply: the default sequence under native TAA, the sequence
/// derived from both extents under reduced temporal reconstruction, none otherwise.
fn install_jitter(
    renderer: &mut FrameRenderer,
    settings: &RendererConfiguration,
) -> Result<(), PostError> {
    renderer.set_projection_jitter(crate::post::jitter(&settings.post, settings.frame)?)
}
#[cfg(test)]
#[path = "configuration_tests.rs"]
mod tests;
