use super::super::live::Capture;
use crate::{CaptureOutput, RetainedCaptureProducts, ViewerError};
use worldrunner_render_post::{FrameConfig, Readback, contracts::AsyncReadback};
use worldrunner_render_scene::{
    Viewport,
    compositor::{ConfiguredFrameRenderer, FeatureTarget},
    offscreen::{LinearRgbaImage, ScalarImage},
};

pub(in crate::renderer) struct Products {
    pub display: Option<Capture>,
    native: Vec<Native>,
    completed: RetainedCaptureProducts,
}
struct Native {
    output: CaptureOutput,
    texture: wgpu::Texture,
    readback: Readback,
    mapping: Option<AsyncReadback>,
    done: bool,
}
impl Native {
    fn new(
        device: &wgpu::Device,
        texture: &wgpu::Texture,
        output: CaptureOutput,
    ) -> Result<Self, ViewerError> {
        Ok(Self {
            output,
            texture: texture.clone(),
            readback: Readback::new(
                device,
                FrameConfig::new([texture.width(), texture.height()], texture.format(), 1),
                256 * 1024 * 1024,
            )?,
            mapping: None,
            done: false,
        })
    }
    fn collect(&mut self, products: &mut RetainedCaptureProducts) -> Result<bool, ViewerError> {
        if self.done {
            return Ok(true);
        }
        let Some(bytes) = self
            .readback
            .try_read_async(self.mapping.as_mut().expect("submitted mapping"))?
        else {
            return Ok(false);
        };
        self.mapping = None;
        let viewport = Viewport {
            width: self.texture.width(),
            height: self.texture.height(),
        };
        let format = self.texture.format();
        match self.output {
            CaptureOutput::LinearHdr | CaptureOutput::Normals => {
                let image = LinearRgbaImage::from_native(viewport, format, &bytes)
                    .map_err(|e| ViewerError::Gpu(e.to_string()))?;
                if self.output == CaptureOutput::LinearHdr {
                    products.linear_hdr = Some(image);
                } else {
                    products.normals = Some(image);
                }
            }
            output => {
                let image = ScalarImage::from_native(viewport, format, &bytes)
                    .map_err(|e| ViewerError::Gpu(e.to_string()))?;
                match output {
                    CaptureOutput::Depth => products.depth = Some(image),
                    CaptureOutput::AmbientOcclusion => products.ambient_occlusion = Some(image),
                    CaptureOutput::SsrConfidence => products.ssr_confidence = Some(image),
                    _ => unreachable!("native feature"),
                }
            }
        }
        self.done = true;
        Ok(true)
    }
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
        let mut native = Vec::new();
        if outputs.contains(&CaptureOutput::LinearHdr) {
            if scene.format() != wgpu::TextureFormat::Rgba16Float || scene.sample_count() != 1 {
                return Err(ViewerError::UnsupportedScene("linear HDR source"));
            }
            native.push(Native::new(device, scene, CaptureOutput::LinearHdr)?);
        }
        Ok(Self {
            display,
            native,
            completed: RetainedCaptureProducts::default(),
        })
    }
    pub(in crate::renderer) fn features(
        &mut self,
        device: &wgpu::Device,
        scheduler: &ConfiguredFrameRenderer,
        outputs: &[CaptureOutput],
    ) -> Result<(), ViewerError> {
        for (output, target) in [
            (CaptureOutput::Depth, FeatureTarget::Depth),
            (CaptureOutput::Normals, FeatureTarget::Normals),
            (
                CaptureOutput::AmbientOcclusion,
                FeatureTarget::AmbientOcclusion,
            ),
            (CaptureOutput::SsrConfidence, FeatureTarget::SsrConfidence),
        ] {
            if outputs.contains(&output) {
                let texture =
                    scheduler
                        .feature_target(target)
                        .ok_or(ViewerError::UnsupportedScene(
                            "admitted capture feature missing",
                        ))?;
                self.native.push(Native::new(device, texture, output)?);
            }
        }
        Ok(())
    }
    pub(in crate::renderer) fn encode(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        output: &wgpu::Texture,
        _scene: &wgpu::Texture,
    ) -> Result<(), ViewerError> {
        if let Some(display) = &mut self.display {
            display.encode(encoder, output)?;
        }
        for native in &mut self.native {
            native.readback.encode(encoder, &native.texture)?;
        }
        Ok(())
    }
    pub(in crate::renderer) fn begin(&mut self) -> Result<(), ViewerError> {
        if let Some(display) = &mut self.display {
            display.begin()?;
        }
        for native in &mut self.native {
            native.mapping = Some(native.readback.begin_async()?);
        }
        Ok(())
    }
    pub(in crate::renderer) fn try_read(
        &mut self,
    ) -> Result<Option<RetainedCaptureProducts>, ViewerError> {
        if self.completed.srgb8.is_none()
            && let Some(display) = &mut self.display
        {
            self.completed.srgb8 = display.try_read()?;
            if self.completed.srgb8.is_none() {
                return Ok(None);
            }
        }
        for native in &mut self.native {
            if !native.collect(&mut self.completed)? {
                return Ok(None);
            }
        }
        Ok(Some(std::mem::take(&mut self.completed)))
    }
}
