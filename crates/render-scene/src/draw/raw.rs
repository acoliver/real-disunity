//! Raw colour recording with encoder-bound last-visible history publication.
use worldrunner_render_gpu::submission::{Publications, RecordingRenderPass, capture::WriteError};

use super::{SceneGpu, ScenePipelines};

impl SceneGpu {
    /// Records raw colour and publishes last-colour transforms on successful enqueue.
    /// Selection and statistics remain unpublished. The pass must belong to a recording frame.
    ///
    /// # Errors
    /// Returns the capture write error before recording any scene draws.
    ///
    /// # Panics
    /// Panics before drawing when instances need preparation, celestial meshes or physical
    /// skies require composition, or dynamic quads were prepared for another pass.
    pub fn draw(
        &self,
        pass: RecordingRenderPass<'_>,
        pipelines: &ScenePipelines,
    ) -> Result<(), WriteError> {
        Self::draw_many(pass, &[(self, pipelines)])
    }

    /// Records several scenes in one pass, validating every scene before the first draw.
    /// Each scene publishes only its immutable last-colour candidate on successful enqueue.
    pub fn draw_many(
        pass: RecordingRenderPass<'_>,
        scenes: &[(&Self, &ScenePipelines)],
    ) -> Result<(), WriteError> {
        let (mut pass, owner) = pass.into_recording_parts();
        for &(scene, pipelines) in scenes {
            scene.validate_raw(&owner, pipelines);
            owner.write(scene.instance_history_key())?;
        }
        for &(scene, pipelines) in scenes {
            scene.draw_color(&mut pass, pipelines, None);
            scene.stage_instance_history(&owner)?;
        }
        Ok(())
    }

    fn validate_raw(&self, owner: &Publications, pipelines: &ScenePipelines) {
        assert!(
            self.celestial.is_none(),
            "celestial meshes require FrameEncoding::encode_scene"
        );
        assert!(
            !self
                .sky
                .as_ref()
                .is_some_and(crate::sky::Sky::requires_preparation),
            "physical skies require compositor preparation"
        );
        assert!(
            self.instances.ready_for(Some(owner)),
            "flush instance updates before drawing"
        );
        self.drawing_uniform();
        self.dynamic.validate_draw(pipelines);
    }
}
