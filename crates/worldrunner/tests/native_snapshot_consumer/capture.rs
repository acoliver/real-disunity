use super::{
    extraction::{Extraction, Stamp, hash},
    native::{Fixture, Session},
    view::Target,
};
use serde_json::Value;
use worldrunner::scene::{camera::ResolvedView, gpu::wgpu};

/// A completed offscreen capture bound to the native extraction and view it was rendered from.
pub struct Capture {
    pub stamp: Stamp,
    pub captured_snapshot: Value,
    pub resolved_view: ResolvedView,
    pub target: Target,
    pub captured_format: wgpu::TextureFormat,
    pub texture_revision: u64,
    pub gpu_completion_hash: String,
    pub pixels: Vec<u8>,
    pub retained_texture: wgpu::Texture,
}

impl Capture {
    /// Succeeds only while the capture still matches the current extraction, view and readback.
    pub fn bind_current(
        &self,
        fixture: &Fixture,
        session: &Session,
        source: &Extraction,
        target: &Target,
    ) -> Result<(), String> {
        source.validate(fixture, session, true)?;
        let fixture_hash =
            hash(&serde_json::to_vec(&self.captured_snapshot).map_err(|error| error.to_string())?);
        if self.stamp != source.stamp
            || self.texture_revision != source.stamp.content_revision
            || fixture_hash != self.stamp.snapshot_sha256
        {
            return Err("capture belongs to a different native extraction".into());
        }
        if self.target != *target
            || self.resolved_view.view_projection_matrix()
                != target.resolve().view_projection_matrix()
        {
            return Err("capture viewport/resize/view identity is stale".into());
        }
        if self.captured_format != super::stage::FORMAT
            || hash(&self.pixels) != self.gpu_completion_hash
        {
            return Err("capture format or completed readback hash differs".into());
        }
        Ok(())
    }

    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        let index = ((y * self.target.viewport[0] + x) * 4) as usize;
        self.pixels[index..index + 4].try_into().unwrap()
    }
}
