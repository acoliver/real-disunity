//! Reads one presented frame back from the GPU, tightly packed and top row first.
use super::{ViewerError, Viewport};
use worldrunner_render_post::{FrameConfig, contracts::AsyncReadback};
use worldrunner_render_scene::offscreen::RgbaImage;

pub(super) struct Capture {
    readback: worldrunner_render_post::Readback,
    mapping: Option<AsyncReadback>,
    viewport: Viewport,
    format: wgpu::TextureFormat,
}

impl Capture {
    /// A readback of `texture`, which must allow copies out of it.
    pub(super) fn for_texture(
        device: &wgpu::Device,
        texture: &wgpu::Texture,
    ) -> Result<Self, ViewerError> {
        if !texture.usage().contains(wgpu::TextureUsages::COPY_SRC) {
            return Err(ViewerError::UnsupportedScene("surface capture copies"));
        }
        let viewport = Viewport {
            width: texture.width(),
            height: texture.height(),
        };
        let format = texture.format();
        let config = FrameConfig::new([viewport.width, viewport.height], format, 1);
        config.validate(&device.limits())?;
        Ok(Self {
            readback: worldrunner_render_post::Readback::new(device, config, 256 * 1024 * 1024)?,
            mapping: None,
            viewport,
            format,
        })
    }
    pub(super) fn matches(&self, texture: &wgpu::Texture) -> bool {
        self.viewport.width == texture.width()
            && self.viewport.height == texture.height()
            && self.format == texture.format()
    }
    pub(super) fn begin(&mut self) -> Result<(), ViewerError> {
        self.mapping = Some(self.readback.begin_async()?);
        Ok(())
    }
    pub(super) fn encode(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        source: &wgpu::Texture,
    ) -> Result<(), ViewerError> {
        self.readback.encode(encoder, source)?;
        Ok(())
    }
    pub(super) fn try_read(&mut self) -> Result<Option<RgbaImage>, ViewerError> {
        let Some(mut pixels) = self
            .readback
            .try_read_async(self.mapping.as_mut().expect("submitted mapping"))?
        else {
            return Ok(None);
        };
        self.mapping = None;
        if matches!(
            self.format,
            wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb
        ) {
            for pixel in pixels.chunks_exact_mut(4) {
                pixel.swap(0, 2);
            }
        }
        Ok(Some(RgbaImage {
            viewport: self.viewport,
            pixels,
        }))
    }
}
