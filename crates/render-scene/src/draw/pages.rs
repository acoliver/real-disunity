use super::SceneGpu;
use crate::terrain::pages::scene::{
    ScenePages, TerrainPageSource, TerrainResidencyError, TerrainResidencyRequest,
};
use worldrunner_render_gpu::{
    configuration::{ConfigurationError, DriverAllocationError},
    submission::SubmissionAuthority,
};
use worldrunner_render_post::FrameError;

impl SceneGpu {
    /// Enables layered material pages on this scene's existing terrain geometry.
    /// Sources return linear decoded layers in normalized terrain coordinates.
    pub fn enable_terrain_pages(
        &mut self,
        device: &wgpu::Device,
        authority: &SubmissionAuthority,
        pipelines: &crate::renderer::ScenePipelines,
        source: TerrainPageSource,
    ) -> Result<(), ConfigurationError<DriverAllocationError>> {
        let terrain = self.terrain.as_mut().ok_or(ConfigurationError::Invalid(
            "material pages require terrain",
        ))?;
        if terrain.pages.is_some() {
            return Err(ConfigurationError::Invalid(
                "material pages already enabled",
            ));
        }
        let mut pages = ScenePages::new(device, authority, source, terrain.page_bounds)?;
        pages.elevation = terrain.page_elevation;
        let view = pages.cache.texture_view();
        for table in &pages.tables {
            pages.groups.push(super::resources::terrain_bind_group(
                device,
                &pipelines.cached_terrain_layout,
                &terrain.sampler,
                (
                    &terrain.uniform,
                    &terrain.retained[3],
                    &terrain.retained[..3],
                ),
                Some((&view, table)),
            ));
        }
        terrain.pages = Some(std::cell::RefCell::new(pages));
        Ok(())
    }

    /// Exact selected-page readiness for the currently selected scene material generation.
    pub fn terrain_pages_ready(&self) -> bool {
        self.terrain
            .as_ref()
            .and_then(|terrain| terrain.pages.as_ref())
            .is_none_or(|pages| pages.borrow().ready())
    }

    /// Invalidate an edited normalized source footprint before preparing another frame.
    pub fn invalidate_terrain_pages(&self, minimum: [f64; 2], maximum: [f64; 2]) {
        let pages = self
            .terrain
            .as_ref()
            .and_then(|terrain| terrain.pages.as_ref())
            .expect("material pages enabled");
        pages.borrow_mut().invalidate(minimum, maximum);
    }

    /// Declares the exact camera selection and current source generation for capture.
    pub fn terrain_residency_request(
        &self,
        frame: crate::frame::FrameUniform,
        viewport: [u32; 2],
    ) -> Option<TerrainResidencyRequest> {
        self.select_terrain_pages(frame, viewport);
        self.terrain
            .as_ref()
            .and_then(|terrain| terrain.pages.as_ref())
            .map(|pages| pages.borrow().request())
    }

    pub fn terrain_residency_ready(
        &self,
        request: &TerrainResidencyRequest,
    ) -> Result<bool, TerrainResidencyError> {
        self.terrain
            .as_ref()
            .and_then(|terrain| terrain.pages.as_ref())
            .ok_or(TerrainResidencyError::Stale)?
            .borrow()
            .ready_for(request)
    }

    pub(crate) fn prepare_terrain_pages(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        frame: crate::frame::FrameUniform,
        viewport: [u32; 2],
    ) -> Result<(), FrameError> {
        if let Some(pages) = self
            .terrain
            .as_ref()
            .and_then(|terrain| terrain.pages.as_ref())
        {
            pages
                .borrow_mut()
                .prepare(device, encoder, frame.view_projection, viewport)?;
        }
        Ok(())
    }

    pub(crate) fn select_terrain_pages(
        &self,
        frame: crate::frame::FrameUniform,
        viewport: [u32; 2],
    ) {
        if let Some(pages) = self
            .terrain
            .as_ref()
            .and_then(|terrain| terrain.pages.as_ref())
        {
            let mut pages = pages.borrow_mut();
            pages.select(frame.view_projection, viewport);
        }
    }

    pub(crate) fn has_terrain_pages(&self) -> bool {
        self.terrain
            .as_ref()
            .is_some_and(|terrain| terrain.pages.is_some())
    }

    pub(crate) fn resident_frame(&self) -> crate::frame::FrameUniform {
        self.frame.get().expect("set a frame before drawing")
    }
}
