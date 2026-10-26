use super::{OffscreenError, OffscreenTarget};
use crate::{device::GpuContext, draw::SceneGpu};

impl OffscreenTarget {
    /// Prepare the capture camera's complete declared selection before copying pixels.
    /// Each bounded upload batch is submitted and retired before admitting the next.
    pub fn wait_for_terrain_pages(
        &self,
        context: &GpuContext,
        scene: &SceneGpu,
    ) -> Result<(), OffscreenError> {
        if !scene.has_terrain_pages() {
            return Ok(());
        }
        let viewport = self.viewport();
        let request = scene
            .terrain_residency_request(&scene.resident_frame(), [viewport.width, viewport.height])
            .expect("enabled terrain pages");
        while !scene.terrain_residency_ready(&request)? {
            let prepared = context
                .submission()
                .prepare(context.device(), |encoder, _| {
                    scene
                        .prepare_terrain_pages(
                            context.device(),
                            encoder,
                            &scene.resident_frame(),
                            [viewport.width, viewport.height],
                        )
                        .map_err(worldrunner_render_post::PostError::from)?;
                    Ok::<_, OffscreenError>(())
                })
                .map_err(worldrunner_render_gpu::submission::PrepareError::into_error)?;
            let submission = prepared.submit()?;
            context
                .device()
                .poll(wgpu::PollType::Wait {
                    submission_index: Some(submission),
                    timeout: None,
                })
                .map_err(OffscreenError::Poll)?;
        }
        Ok(())
    }
}
