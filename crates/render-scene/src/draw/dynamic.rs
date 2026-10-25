use super::{Device, DrawError, RenderScene, SceneGpu, ScenePipelines};

impl SceneGpu {
    /// An empty resident scene for runtime contributions.
    pub fn empty(
        device: &Device,
        authority: &worldrunner_render_gpu::submission::SubmissionAuthority,
        pipelines: &ScenePipelines,
    ) -> Result<Self, DrawError> {
        let scene = RenderScene::new(worldrunner_render_plan::SceneParts {
            resources: worldrunner_render_plan::SceneResources::new(vec![], vec![])
                .expect("empty resources"),
            geometry: worldrunner_render_plan::SceneGeometry::new(vec![], vec![])
                .expect("empty geometry"),
            terrain: None,
            effects: None,
        })
        .expect("empty scene");
        Self::upload(device, authority, &scene, pipelines)
    }
    /// Atomically replaces the runtime quad contribution.
    pub fn set_dynamic_quads(
        &mut self,
        device: &Device,
        authority: &worldrunner_render_gpu::submission::SubmissionAuthority,
        pipelines: &ScenePipelines,
        quads: &[crate::dynamic::DynamicQuad],
    ) -> Result<(), crate::dynamic::DynamicError> {
        self.dynamic.set_quads(
            device,
            authority.queue(),
            crate::dynamic::DynamicBindings {
                pipelines,
                placeholder: &self.retained.placeholder,
                sampler: &self.retained.sampler,
            },
            quads,
        )
    }

    /// Replaces runtime textures and clears runtime geometry on success.
    pub fn set_dynamic_textures(
        &mut self,
        device: &Device,
        authority: &worldrunner_render_gpu::submission::SubmissionAuthority,
        textures: &[worldrunner_render_plan::SceneTexture],
    ) -> Result<(), crate::dynamic::DynamicError> {
        self.dynamic
            .set_textures(device, authority.queue(), textures)
    }
}
