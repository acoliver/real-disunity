//! Device-scoped retained uploads keyed by caller-owned content identity.
use std::collections::HashMap;

use crate::texture::{TextureUpload, UploadError};

/// One renderer/context owns this store. No registry or cross-device lookup is performed.
/// An ID identifies immutable content *and* its upload interpretation/usage contract.
/// Use a new ID when any of those change; private bytes and IDs remain caller-owned.
#[derive(Debug)]
pub struct CanonicalTextureStore {
    device: wgpu::Device,
    queue: wgpu::Queue,
    textures: HashMap<String, wgpu::Texture>,
}

impl CanonicalTextureStore {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        Self {
            device: device.clone(),
            queue: queue.clone(),
            textures: HashMap::new(),
        }
    }

    /// Calls the upload provider only on a miss, using this context's device and queue.
    /// Returned wgpu handles are cheap clones retaining the same GPU resource.
    pub fn get_or_upload<'a>(
        &mut self,
        content_id: &str,
        label: &str,
        usage: wgpu::TextureUsages,
        prepare: impl FnOnce() -> Result<TextureUpload<'a>, UploadError>,
    ) -> Result<wgpu::Texture, UploadError> {
        if let Some(texture) = self.textures.get(content_id) {
            return Ok(texture.clone());
        }
        let texture = prepare()?.upload(&self.device, &self.queue, label, usage)?;
        self.textures.insert(content_id.to_owned(), texture.clone());
        Ok(texture)
    }
}
