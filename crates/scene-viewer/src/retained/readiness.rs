use super::{
    BTreeMap, CameraPose, ObjectId, RenderScene, RetainedHost, RetainedUpdate, SceneCamera,
    SceneEffects, SceneObject, ViewerError, ViewerOptions,
};
use std::{collections::VecDeque, sync::mpsc};
use worldrunner_render_plan::{SceneGeometry, SceneResources};
use worldrunner_render_scene::draw::PreparedScene;

pub(crate) struct Preparation {
    pub generation: u64,
    pub update: RetainedUpdate,
    pub scene: Option<mpsc::Receiver<Result<PreparedScene, ViewerError>>>,
    pub ids: Vec<ObjectId>,
    pub pages_enabled: bool,
    pub capture_features: [bool; 4],
    pub require_pages: bool,
}

pub(crate) struct Readiness {
    pub queue: VecDeque<Preparation>,
    pub limit: usize,
    pub accepted: u64,
    pub ready: u64,
    pub published: u64,
    pub pages_enabled: bool,
    pub capture_features: [bool; 4],
    pub page_wait: bool,
    pub camera: (CameraPose, SceneCamera),
    #[cfg(test)]
    pub preparation_hook: Option<Box<dyn FnOnce() + Send>>,
}

impl Readiness {
    pub(crate) fn pages_enabled_for(&self, generation: u64) -> bool {
        if generation == self.published {
            self.pages_enabled
        } else {
            self.queue
                .iter()
                .find(|p| p.generation == generation)
                .expect("accepted, unsuperseded generation")
                .pages_enabled
        }
    }

    pub(crate) fn publish_front(&mut self) -> RetainedUpdate {
        let front = self.queue.pop_front().expect("published front generation");
        if let RetainedUpdate::Camera { pose, lens } = &front.update {
            self.camera = (*pose, *lens);
        }
        assert_eq!(front.generation, self.published + 1);
        self.published = front.generation;
        self.pages_enabled = front.pages_enabled;
        self.capture_features = front.capture_features;
        self.page_wait = front.require_pages;
        if !self.page_wait {
            self.ready = front.generation;
        }
        front.update
    }
}

impl RetainedHost {
    /// Bounds accepted updates, including completed preparations awaiting publication.
    /// A full queue returns `ViewerError::BackPressure` without changing scene state.
    /// CPU texture, geometry and terrain preparation run off-thread. GPU allocation and upload still
    /// run on the event thread at an admitted frame boundary, without GPU waits.
    pub fn with_preparation_limit(
        scene: RenderScene,
        options: ViewerOptions,
        limit: usize,
    ) -> Result<Self, ViewerError> {
        if limit == 0 {
            return Err(ViewerError::InvalidOptions(
                "preparation limit must be positive",
            ));
        }
        let mut host = Self::new(scene, options)?;
        host.readiness.limit = limit;
        Ok(host)
    }

    pub fn ready_generation(&self) -> u64 {
        self.readiness.ready
    }

    pub fn accepted_generation(&self) -> u64 {
        self.readiness.accepted
    }

    pub fn preparations_in_flight(&self) -> usize {
        self.readiness.queue.len() + usize::from(self.readiness.page_wait)
    }

    /// Accepts an edit whose generation becomes ready only when the current view's
    /// selected terrain material pages are resident. Paging must be enabled.
    pub fn apply_with_pages(&mut self, update: RetainedUpdate) -> Result<(), ViewerError> {
        let terrain = match &update {
            RetainedUpdate::Terrain(terrain) => terrain.as_deref(),
            _ => self.scene.terrain(),
        };
        if terrain.is_some_and(|terrain| terrain.streaming_policy().is_some()) {
            return Err(ViewerError::UnsupportedScene(
                "streaming terrain page dependencies",
            ));
        }
        if terrain
            .and_then(worldrunner_render_plan::SceneTerrain::page_policy)
            .is_none()
        {
            return Err(ViewerError::InvalidOptions(
                "page dependency requires paging",
            ));
        }
        self.apply(update)?;
        self.readiness
            .queue
            .back_mut()
            .expect("accepted update")
            .require_pages = true;
        Ok(())
    }

    pub(super) fn validate_streamed_effects(
        &self,
        effects: &SceneEffects,
    ) -> Result<(), ViewerError> {
        worldrunner_render_scene::post_settings(effects)?;
        if let Some(water) = effects.water() {
            worldrunner_render_scene::water::validate_settings(water)?;
        }
        if matches!(self.options.frame_profile, crate::FrameProfile::Legacy)
            && (effects.ssao().is_some() || effects.ssr().is_some() || effects.water().is_some())
        {
            return Err(ViewerError::UnsupportedScene(
                "water, SSAO and SSR streaming require HDR",
            ));
        }
        Ok(())
    }

    pub(super) fn snapshot(
        &self,
        resources: SceneResources,
        meshes: Vec<worldrunner_render_plan::SceneMesh>,
        objects: &BTreeMap<ObjectId, SceneObject>,
        effects: Option<SceneEffects>,
    ) -> Result<RenderScene, ViewerError> {
        self.scene
            .with_resources(
                resources,
                SceneGeometry::new(meshes, objects.values().copied().collect())
                    .map_err(|error| ViewerError::Driver(error.to_string()))?,
                effects,
            )
            .map_err(|error| ViewerError::Driver(error.to_string()))
    }

    pub(super) fn enqueue(&mut self, update: RetainedUpdate, rebuild: bool) {
        self.readiness.accepted += 1;
        let scene = rebuild.then(|| {
            let scene = self.scene.clone();
            let (send, receive) = mpsc::sync_channel(1);
            // GPU objects remain on the event thread. CPU content preparation
            // runs here; publication never joins or waits for this worker.
            #[cfg(test)]
            let hook = self.readiness.preparation_hook.take();
            std::thread::spawn(move || {
                #[cfg(test)]
                if let Some(hook) = hook {
                    hook();
                }
                let result = prepare(&scene);
                // A closed receiver means the host exited and cancelled this update.
                let _ = send.send(result);
            });
            receive
        });
        self.readiness.queue.push_back(Preparation {
            generation: self.readiness.accepted,
            capture_features: super::capture::features(self.scene.effects()),
            pages_enabled: self
                .scene
                .terrain()
                .and_then(worldrunner_render_plan::SceneTerrain::page_policy)
                .is_some(),
            require_pages: false,
            update,
            scene,
            ids: if rebuild {
                self.objects.keys().copied().collect()
            } else {
                Vec::new()
            },
        });
    }
}

fn prepare(scene: &RenderScene) -> Result<PreparedScene, ViewerError> {
    Ok(PreparedScene::new(scene)?)
}

pub(super) fn resource_settings_changed(a: Option<&SceneEffects>, b: &SceneEffects) -> bool {
    a.is_none_or(|a| {
        a.ssao() != b.ssao()
            || a.ssr() != b.ssr()
            || a.water_bodies() != b.water_bodies()
            || a.atmosphere() != b.atmosphere()
            || a.sky() != b.sky()
    })
}

#[cfg(test)]
mod tests;
