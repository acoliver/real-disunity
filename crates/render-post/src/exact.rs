use crate::{FrameConfig, PostError};

/// Opt-in display profile: linear f32 composition followed by nearest, ties-up sRGB8.
///
/// `Rgba32Float` preserves the f32 attachment value between draws. Blending and shading
/// still follow the GPU's f32 arithmetic contract; this profile does not promise real-
/// arithmetic composition. The final quantizer is exact for every finite f32 input.
/// RGB is clamped and sRGB encoded; alpha is clamped and rounded linearly. NaNs map to 0.
/// Existing HDR/effect chains and attachment-defined output are unchanged.
#[derive(Debug)]
pub struct ExactSrgb8 {
    texture: wgpu::Texture,
    write_view: wgpu::TextureView,
    sample_view: wgpu::TextureView,
}

impl ExactSrgb8 {
    pub const REQUIRED_FEATURES: wgpu::Features = wgpu::Features::FLOAT32_BLENDABLE;
    pub const WORKING_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba32Float;
    pub const OUTPUT_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
    pub const WRITE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

    /// Use this format for `ScenePipelines` and all stages preceding the output encoder.
    /// Request `REQUIRED_FEATURES` when acquiring the device; no precision fallback occurs.
    pub fn frame_config(device: &wgpu::Device, size: [u32; 2]) -> Result<FrameConfig, PostError> {
        if !device.features().contains(Self::REQUIRED_FEATURES) {
            return Err(PostError::ExactCompositionFeatures);
        }
        let config = FrameConfig::new(size, Self::WORKING_FORMAT, 1);
        config.validate(&device.limits())?;
        Ok(config)
    }

    /// Creates an sRGB sample/readback texture with an explicitly declared UNORM write view.
    /// Recreate this output alongside `PostChain::resize`. Caller still owns encode/submission.
    pub fn new(device: &wgpu::Device, size: [u32; 2]) -> Result<Self, PostError> {
        Self::frame_config(device, size)?;
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("exact sRGB8 output"),
            size: crate::targets::extent(size),
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: Self::OUTPUT_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[Self::WRITE_FORMAT],
        });
        let write_view = texture.create_view(&wgpu::TextureViewDescriptor {
            format: Some(Self::WRITE_FORMAT),
            ..Default::default()
        });
        let sample_view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        Ok(Self {
            texture,
            write_view,
            sample_view,
        })
    }

    /// Supply only this view to the exact `CopyOutput` stage. Avoids hardware sRGB conversion.
    pub const fn write_view(&self) -> &wgpu::TextureView {
        &self.write_view
    }

    /// Consumers sample through this sRGB view, which decodes the encoded texels normally.
    pub const fn sample_view(&self) -> &wgpu::TextureView {
        &self.sample_view
    }

    /// Raw RGBA8 sRGB bytes for the existing Readback service.
    pub const fn texture(&self) -> &wgpu::Texture {
        &self.texture
    }
}
