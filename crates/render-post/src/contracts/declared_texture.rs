use super::{AllocationKey, ContractError, ResourceLifetime, ResourceRegistry, Semantic};

/// A texture and the view formats established by its creation descriptor.
#[derive(Clone, Debug)]
pub struct DeclaredTexture {
    texture: wgpu::Texture,
    view_formats: Vec<wgpu::TextureFormat>,
}

impl DeclaredTexture {
    pub fn new(device: &wgpu::Device, descriptor: &wgpu::TextureDescriptor<'_>) -> Self {
        Self {
            texture: device.create_texture(descriptor),
            view_formats: descriptor.view_formats.to_vec(),
        }
    }

    /// Validates a foreign texture against the descriptor used to create it.
    /// Pass the texture's creating device so validation scopes capture its errors.
    /// Alternate declarations are also checked by wgpu before being retained.
    pub async fn from_texture(
        device: &wgpu::Device,
        texture: &wgpu::Texture,
        descriptor: &wgpu::TextureDescriptor<'_>,
    ) -> Result<Self, ContractError> {
        if texture.format() != descriptor.format
            || texture.size() != descriptor.size
            || texture.mip_level_count() != descriptor.mip_level_count
            || texture.sample_count() != descriptor.sample_count
            || texture.dimension() != descriptor.dimension
            || texture.usage() != descriptor.usage
        {
            return Err(ContractError::TextureDescriptor);
        }
        for &format in descriptor.view_formats {
            let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
            let view = texture.create_view(&wgpu::TextureViewDescriptor {
                format: Some(format),
                usage: Some(view_usage(texture, format)),
                ..Default::default()
            });
            let error = scope.pop().await;
            drop(view);
            if error.is_some() {
                return Err(ContractError::TextureDeclaration(format));
            }
        }
        Ok(Self {
            texture: texture.clone(),
            view_formats: descriptor.view_formats.to_vec(),
        })
    }

    pub fn texture(&self) -> &wgpu::Texture {
        &self.texture
    }
}

impl ResourceRegistry {
    /// Allocates storage and captures the same descriptor's view declarations.
    pub fn create(
        &mut self,
        device: &wgpu::Device,
        descriptor: &wgpu::TextureDescriptor<'_>,
        lifetime: ResourceLifetime,
        semantic: Semantic,
        initial: Option<u32>,
    ) -> Result<AllocationKey, ContractError> {
        self.import_declared(
            &DeclaredTexture::new(device, descriptor),
            lifetime,
            semantic,
            initial,
        )
    }

    pub fn import_declared(
        &mut self,
        texture: &DeclaredTexture,
        lifetime: ResourceLifetime,
        semantic: Semantic,
        initial: Option<u32>,
    ) -> Result<AllocationKey, ContractError> {
        self.import_with_view_formats(
            &texture.texture,
            lifetime,
            semantic,
            initial,
            &texture.view_formats,
        )
    }
}

pub(super) fn view_usage(
    texture: &wgpu::Texture,
    format: wgpu::TextureFormat,
) -> wgpu::TextureUsages {
    let mut usage = texture.usage();
    if format.is_srgb() {
        usage.remove(wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::STORAGE_ATOMIC);
    }
    usage
}
