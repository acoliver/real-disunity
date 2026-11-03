#[cfg(test)]
mod tests;

use super::{RenderScene, RetainedHost, ViewerError, ViewerOptions, capture, readiness};
use crate::{InitialContent, InitialContext};

pub(super) type Supplier =
    Box<dyn FnOnce(InitialContext<'_>) -> Result<InitialContent, String> + Send>;

impl RetainedHost {
    pub fn new(scene: RenderScene, options: ViewerOptions) -> Result<Self, ViewerError> {
        let scene = crate::reflection_settings::startup_scene(scene, &options)?;
        crate::validate(&scene, &options)?;
        worldrunner_render_scene::draw::validate_decal_materials(&scene)?;
        Self::build(Some(scene), options, None)
    }

    /// Defers content creation until the rendering adapter and physical surface
    /// extent are known. The supplier runs once, before device creation.
    /// It cannot be combined with an adapter callback.
    /// Until then edits fail with `InitialSceneUnavailable`, `view` returns `None`,
    /// object queries are empty, and generation-zero captures queue for validation.
    pub fn with_initial_content(
        options: ViewerOptions,
        supplier: impl FnOnce(InitialContext<'_>) -> Result<InitialContent, String> + Send + 'static,
    ) -> Result<Self, ViewerError> {
        if options.adapter_callback.is_some() {
            return Err(ViewerError::InvalidOptions(
                "initial content supplier replaces adapter callback",
            ));
        }
        Self::build(None, options, Some(Box::new(supplier)))
    }

    fn build(
        scene: Option<RenderScene>,
        options: ViewerOptions,
        supplier: Option<Supplier>,
    ) -> Result<Self, ViewerError> {
        let capture_pool = match &scene {
            Some(scene) => options.capture_pool.effective_bytes(
                options.frame_profile.target().1,
                capture::ao_format(scene.effects()),
            )?,
            None => 0,
        };
        let mut host = Self {
            statistics: None,
            gpu_timing: if options.gpu_timing {
                super::GpuTiming::Pending
            } else {
                super::GpuTiming::Disabled
            },
            captures: capture::Captures::default(),
            capture_pool,
            readiness: initial_readiness(scene.as_ref(), &options),
            initial_scene: scene.clone(),
            scene,
            initial_content: supplier,
            objects: super::BTreeMap::new(),
            objects_stale: false,
            decals: super::BTreeMap::new(),
            input_state: super::input::InputState::default(),
            size: options.size,
            options,
            exit: false,
            occluded: false,
        };
        host.derive_maps();
        Ok(host)
    }

    fn derive_maps(&mut self) {
        if let Some(scene) = &self.scene {
            self.objects = scene
                .geometry()
                .objects()
                .iter()
                .copied()
                .enumerate()
                .map(|(index, object)| (super::ObjectId(index as u64), object))
                .collect();
            self.decals = scene
                .decals()
                .iter()
                .copied()
                .enumerate()
                .map(|(index, decal)| (index as u64, decal))
                .collect();
        }
    }

    pub(crate) fn resolve_initial_content(
        &mut self,
        context: InitialContext<'_>,
    ) -> Result<(), ViewerError> {
        if let Some(scene) = &self.initial_scene {
            return crate::adapter::resolve_startup(
                context.adapter,
                scene,
                &mut self.options,
                Some((&mut self.capture_pool, &mut self.captures, &self.readiness)),
            );
        }
        let supplier = self
            .initial_content
            .take()
            .ok_or(ViewerError::InitialSceneUnavailable)?;
        let selected = supplier(context).map_err(ViewerError::InitialContent)?;
        let mut options = self.options.clone();
        if let Some(settings) = selected.settings {
            crate::adapter::apply_settings(&mut options, settings);
        }
        let scene = crate::reflection_settings::startup_scene(selected.scene, &options)?;
        crate::validate(&scene, &options)?;
        worldrunner_render_scene::draw::validate_decal_materials(&scene)?;
        let pool = options.capture_pool.effective_bytes(
            options.frame_profile.target().1,
            capture::ao_format(scene.effects()),
        )?;
        self.readiness = initial_readiness(Some(&scene), &options);
        self.options = options;
        self.capture_pool = pool;
        self.validate_initial_captures()?;
        self.size = context.surface_extent;
        self.input_state.surface_extent = Some(context.surface_extent);
        self.initial_scene = Some(scene.clone());
        self.scene = Some(scene);
        self.derive_maps();
        Ok(())
    }

    pub(super) fn require_initial_scene(&self) -> Result<(), ViewerError> {
        self.initial_scene
            .as_ref()
            .map(|_| ())
            .ok_or(ViewerError::InitialSceneUnavailable)
    }

    pub(crate) fn scene(&self) -> &RenderScene {
        self.scene
            .as_ref()
            .expect("scene resolved before internal access")
    }

    pub(crate) fn initial_scene(&self) -> &RenderScene {
        self.initial_scene
            .as_ref()
            .expect("scene resolved before renderer creation")
    }
}

fn initial_readiness(scene: Option<&RenderScene>, options: &ViewerOptions) -> readiness::Readiness {
    let effects = scene.and_then(RenderScene::effects);
    readiness::Readiness {
        queue: std::collections::VecDeque::new(),
        limit: 64,
        accepted: 0,
        ready: 0,
        published: 0,
        capture_features: capture::features(effects),
        capture_ao_format: capture::ao_format(effects),
        pages_enabled: scene
            .and_then(RenderScene::terrain)
            .and_then(worldrunner_render_plan::SceneTerrain::page_policy)
            .is_some(),
        page_wait: false,
        geometry_wait: false,
        camera: (options.pose, options.camera, options.camera_convention),
        #[cfg(test)]
        preparation_hook: None,
    }
}
