//! Attachment-only multisampling for externally owned resolve targets (for example surfaces).
use thiserror::Error;

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum AttachmentError {
    #[error("attachment extent must be nonzero and within device limits")]
    Extent,
    #[error("{0} is not a multisampled attachment count")]
    Samples(u32),
    #[error("{format:?} does not guarantee {samples}x color attachment resolve support")]
    Format {
        format: wgpu::TextureFormat,
        samples: u32,
    },
}

/// Owns exactly one multisampled color attachment; the caller supplies the resolve target.
/// It never allocates a single-sample image or enables sampling/copy usage.
#[derive(Debug)]
pub struct MsaaAttachment {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
}

impl MsaaAttachment {
    pub fn new(
        device: &wgpu::Device,
        label: &str,
        size: [u32; 2],
        format: wgpu::TextureFormat,
        samples: u32,
    ) -> Result<Self, AttachmentError> {
        if size.contains(&0)
            || size
                .iter()
                .any(|&side| side > device.limits().max_texture_dimension_2d)
        {
            return Err(AttachmentError::Extent);
        }
        if !matches!(samples, 2 | 4 | 8 | 16) {
            return Err(AttachmentError::Samples(samples));
        }
        let features = format.guaranteed_format_features(wgpu::Features::empty());
        if !features
            .allowed_usages
            .contains(wgpu::TextureUsages::RENDER_ATTACHMENT)
            || !features.flags.sample_count_supported(samples)
            || !features
                .flags
                .contains(wgpu::TextureFormatFeatureFlags::MULTISAMPLE_RESOLVE)
        {
            return Err(AttachmentError::Format { format, samples });
        }
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width: size[0],
                height: size[1],
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: samples,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        Ok(Self { texture, view })
    }

    pub const fn texture(&self) -> &wgpu::Texture {
        &self.texture
    }

    pub const fn view(&self) -> &wgpu::TextureView {
        &self.view
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attachment_rejects_invalid_contracts_before_gpu_allocation() {
        let instance = crate::device::headless_instance();
        let adapter =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
                .expect("attachment contract test requires a GPU adapter");
        let (device, _) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let format = wgpu::TextureFormat::Rgba8UnormSrgb;
        for size in [
            [0, 1],
            [1, 0],
            [device.limits().max_texture_dimension_2d + 1, 1],
        ] {
            assert_eq!(
                MsaaAttachment::new(&device, "invalid extent", size, format, 4).unwrap_err(),
                AttachmentError::Extent
            );
        }
        for samples in [0, 1, 3] {
            assert_eq!(
                MsaaAttachment::new(&device, "invalid samples", [1, 1], format, samples)
                    .unwrap_err(),
                AttachmentError::Samples(samples)
            );
        }
        let format = wgpu::TextureFormat::Rgba8Uint;
        assert_eq!(
            MsaaAttachment::new(&device, "invalid format", [1, 1], format, 4).unwrap_err(),
            AttachmentError::Format { format, samples: 4 }
        );
        let target = MsaaAttachment::new(
            &device,
            "valid",
            [7, 3],
            wgpu::TextureFormat::Rgba8UnormSrgb,
            4,
        )
        .unwrap();
        assert_eq!(target.texture(), target.view().texture());
        assert_eq!(target.texture().sample_count(), 4);
        assert_eq!(
            target.texture().usage(),
            wgpu::TextureUsages::RENDER_ATTACHMENT
        );
        assert!(pollster::block_on(scope.pop()).is_none());
    }
}
