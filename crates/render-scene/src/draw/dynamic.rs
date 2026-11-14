#[cfg(test)]
#[path = "dynamic_tests.rs"]
mod tests;

use super::{Device, DrawError, RenderScene, SceneGpu, ScenePipelines};

fn dynamic_refused(error: &crate::dynamic::DynamicError) -> worldrunner_render_post::FrameError {
    worldrunner_render_post::FrameError::FeaturePreparation {
        feature: "dynamic quads",
        reason: error.to_string(),
    }
}

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
    /// Atomically replaces the runtime quad contribution. The quads are prepared for the pass
    /// `pipelines` describe, which is the pass the frames that draw them must open.
    pub fn set_dynamic_quads(
        &mut self,
        device: &Device,
        authority: &worldrunner_render_gpu::submission::SubmissionAuthority,
        pipelines: &ScenePipelines,
        quads: &[crate::dynamic::DynamicQuad],
    ) -> Result<(), crate::dynamic::DynamicError> {
        validate_scene_quads(quads)?;
        self.dynamic.set_quads(
            device,
            authority.queue(),
            crate::dynamic::DynamicBindings {
                placeholder: &self.retained.placeholder,
                sampler: &self.retained.sampler,
                target: crate::dynamic::QuadTarget::Scene(crate::dynamic::SceneStaging {
                    pipelines,
                    frames: self
                        .frame_slots
                        .iter()
                        .map(|(frame, _)| frame.clone())
                        .collect(),
                    runtime: &mut self.unlit,
                    authority,
                }),
            },
            quads,
        )
    }

    /// Atomically replaces the texture table and geometry of one native frame.
    pub fn set_dynamic_contribution(
        &mut self,
        device: &Device,
        authority: &worldrunner_render_gpu::submission::SubmissionAuthority,
        pipelines: &ScenePipelines,
        textures: &[worldrunner_render_plan::SceneTexture],
        quads: &[crate::dynamic::DynamicQuad],
    ) -> Result<(), crate::dynamic::DynamicError> {
        validate_scene_quads(quads)?;
        let mut staged = crate::dynamic::DynamicGpu::default();
        staged.set_textures(device, authority.queue(), textures)?;
        staged.set_quads(
            device,
            authority.queue(),
            crate::dynamic::DynamicBindings {
                placeholder: &self.retained.placeholder,
                sampler: &self.retained.sampler,
                target: crate::dynamic::QuadTarget::Scene(crate::dynamic::SceneStaging {
                    pipelines,
                    frames: self
                        .frame_slots
                        .iter()
                        .map(|(frame, _)| frame.clone())
                        .collect(),
                    runtime: &mut self.unlit,
                    authority,
                }),
            },
            quads,
        )?;
        self.dynamic = staged;
        Ok(())
    }

    /// Prepares the resident quads for the pass `pipelines` describe, when staging did not, and
    /// makes the frame `encoder` records hold their draws until the GPU has completed it. A
    /// frame that draws the quads calls this before it opens its pass, where a failure is still
    /// an error rather than a lost draw.
    pub(crate) fn prepare_dynamic(
        &mut self,
        encoder: &wgpu::CommandEncoder,
        pipelines: &ScenePipelines,
    ) -> Result<(), worldrunner_render_post::FrameError> {
        self.dynamic
            .prepare_for(&mut self.unlit, pipelines)
            .map_err(|error| dynamic_refused(&error))?;
        self.retain_dynamic(encoder, pipelines)
    }

    /// Makes the frame `encoder` records hold the resident quads' draws for the pass `pipelines`
    /// describe, or refuses by name when they were staged for another pass.
    pub(crate) fn retain_dynamic(
        &self,
        encoder: &wgpu::CommandEncoder,
        pipelines: &ScenePipelines,
    ) -> Result<(), worldrunner_render_post::FrameError> {
        self.dynamic
            .retain(encoder, pipelines)
            .map_err(|error| dynamic_refused(&error))
    }

    #[cfg(test)]
    pub(crate) fn dynamic_probes(&self) -> Vec<std::sync::Weak<dyn std::any::Any + Send + Sync>> {
        self.dynamic.probes()
    }

    /// Releases every runtime texture, draw buffer and retained quad index buffer on unload.
    pub fn clear_dynamic(&mut self) {
        self.dynamic = crate::dynamic::DynamicGpu::default();
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

fn validate_scene_quads(
    quads: &[crate::dynamic::DynamicQuad],
) -> Result<(), crate::dynamic::DynamicError> {
    if let Some(index) = quads.iter().position(|quad| quad.depth_test) {
        return Err(crate::dynamic::DynamicError::InvalidQuad {
            index,
            field: "annotation depth requires retained UI",
        });
    }
    if let Some(index) = quads.iter().position(|quad| quad.corner_colors.is_some()) {
        return Err(crate::dynamic::DynamicError::CornerColorsRequireRetainedUi { index });
    }
    Ok(())
}
