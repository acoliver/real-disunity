use super::super::live::Capture;
use crate::{CaptureOutput, RetainedCaptureProducts, ViewerError};
use worldrunner_render_post::{FrameConfig, Readback, contracts::AsyncReadback};
use worldrunner_render_scene::{Viewport, offscreen::LinearRgbaImage};

pub(in crate::renderer) struct Products {
    pub display: Option<Capture>,
    linear: Option<Linear>,
    srgb8: Option<worldrunner_render_scene::offscreen::RgbaImage>,
    linear_hdr: Option<LinearRgbaImage>,
}
struct Linear {
    readback: Readback,
    mapping: Option<AsyncReadback>,
    viewport: Viewport,
    format: wgpu::TextureFormat,
}
impl Products {
    pub(in crate::renderer) fn new(
        device: &wgpu::Device,
        output: &wgpu::Texture,
        scene: &wgpu::Texture,
        outputs: &[CaptureOutput],
        spare: Option<Capture>,
    ) -> Result<Self, ViewerError> {
        let display = if outputs.contains(&CaptureOutput::Srgb8) {
            Some(match spare {
                Some(image) if image.matches(output) => image,
                _ => Capture::for_texture(device, output)?,
            })
        } else {
            None
        };
        let linear = if outputs.contains(&CaptureOutput::LinearHdr) {
            if scene.format() != wgpu::TextureFormat::Rgba16Float || scene.sample_count() != 1 {
                return Err(ViewerError::UnsupportedScene("linear HDR source"));
            }
            Some(Linear {
                readback: Readback::new(
                    device,
                    FrameConfig::new([scene.width(), scene.height()], scene.format(), 1),
                    256 * 1024 * 1024,
                )?,
                mapping: None,
                viewport: Viewport {
                    width: scene.width(),
                    height: scene.height(),
                },
                format: scene.format(),
            })
        } else {
            None
        };
        Ok(Self {
            display,
            linear,
            srgb8: None,
            linear_hdr: None,
        })
    }
    pub(in crate::renderer) fn encode(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        output: &wgpu::Texture,
        scene: &wgpu::Texture,
    ) -> Result<(), ViewerError> {
        if let Some(display) = &mut self.display {
            display.encode(encoder, output)?;
        }
        if let Some(linear) = &mut self.linear {
            linear.readback.encode(encoder, scene)?;
        }
        Ok(())
    }
    pub(in crate::renderer) fn begin(&mut self) -> Result<(), ViewerError> {
        if let Some(display) = &mut self.display {
            display.begin()?;
        }
        if let Some(linear) = &mut self.linear {
            linear.mapping = Some(linear.readback.begin_async()?);
        }
        Ok(())
    }
    pub(in crate::renderer) fn try_read(
        &mut self,
    ) -> Result<Option<RetainedCaptureProducts>, ViewerError> {
        if self.srgb8.is_none()
            && let Some(display) = &mut self.display
        {
            self.srgb8 = display.try_read()?;
            if self.srgb8.is_none() {
                return Ok(None);
            }
        }
        if self.linear_hdr.is_none()
            && let Some(linear) = &mut self.linear
        {
            let Some(bytes) = linear
                .readback
                .try_read_async(linear.mapping.as_mut().expect("submitted linear mapping"))?
            else {
                return Ok(None);
            };
            linear.mapping = None;
            self.linear_hdr = Some(
                LinearRgbaImage::from_native(linear.viewport, linear.format, &bytes)
                    .map_err(|error| ViewerError::Gpu(error.to_string()))?,
            );
        }
        Ok(Some(RetainedCaptureProducts {
            srgb8: self.srgb8.take(),
            linear_hdr: self.linear_hdr.take(),
        }))
    }
}
