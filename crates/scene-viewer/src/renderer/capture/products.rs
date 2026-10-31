use super::super::live::Capture;
use crate::{CaptureOutput, RetainedCaptureProducts, ViewerError};
use worldrunner_render_post::{FrameConfig, Readback, contracts::AsyncReadback};
use worldrunner_render_scene::{
    Viewport,
    compositor::{ConfiguredFrameRenderer, FeatureCapture, FeatureTarget},
    offscreen::{LinearRgbaImage, Rgba16FloatImage, ScalarImage},
};

pub(in crate::renderer) struct Products {
    pub display: Option<Capture>,
    native: Vec<Native>,
    terms: Vec<worldrunner_render_scene::compositor::terms::TermCapture>,
    completed: RetainedCaptureProducts,
}
struct Native {
    output: CaptureOutput,
    texture: wgpu::Texture,
    readback: Readback,
    size: [u32; 2],
    lanes: u32,
    columns: u32,
    stride: [u32; 2],
    mapping: Option<AsyncReadback>,
    done: bool,
}
impl Native {
    fn new(
        device: &wgpu::Device,
        target: FeatureCapture<'_>,
        output: CaptureOutput,
    ) -> Result<Self, ViewerError> {
        Ok(Self {
            output,
            texture: target.texture.clone(),
            size: target.size,
            lanes: target.lanes,
            columns: target.columns,
            stride: target.stride,
            readback: Readback::new(
                device,
                FrameConfig::new(target.size, target.texture.format(), 1),
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
            width: self.size[0],
            height: self.size[1],
        };
        let format = self.texture.format();
        match self.output {
            CaptureOutput::Specular => {
                products.specular = Some(Rgba16FloatImage { viewport, bytes });
            }
            CaptureOutput::LinearHdr
            | CaptureOutput::Normals
            | CaptureOutput::Albedo
            | CaptureOutput::SunVisibility
            | CaptureOutput::DirectDiffuse
            | CaptureOutput::AmbientDiffuse
            | CaptureOutput::EnvironmentSpecular
            | CaptureOutput::SunSpecular => {
                let image = LinearRgbaImage::from_native(viewport, format, &bytes)
                    .map_err(|e| ViewerError::Gpu(e.to_string()))?;
                match self.output {
                    CaptureOutput::LinearHdr => products.linear_hdr = Some(image),
                    CaptureOutput::Normals => products.normals = Some(image),
                    output if output.term().is_some() => {
                        products.store_term(self.output, image);
                    }
                    _ => unreachable!("colour output"),
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
            native.push(Native::new(
                device,
                FeatureCapture::whole(scene),
                CaptureOutput::LinearHdr,
            )?);
        }
        let terms = Self::term_products(device, scene, outputs, &mut native)?;
        Ok(Self {
            terms,
            display,
            native,
            completed: RetainedCaptureProducts::default(),
        })
    }
    fn term_products(
        device: &wgpu::Device,
        scene: &wgpu::Texture,
        outputs: &[CaptureOutput],
        native: &mut Vec<Native>,
    ) -> Result<Vec<worldrunner_render_scene::compositor::terms::TermCapture>, ViewerError> {
        let mut terms = Vec::new();
        for &output in outputs {
            if let Some(term) = output.term() {
                let capture = worldrunner_render_scene::compositor::terms::TermCapture::new(
                    device,
                    [scene.width(), scene.height()],
                    term,
                );
                native.push(Native::new(
                    device,
                    FeatureCapture::whole(&capture.texture),
                    output,
                )?);
                terms.push(capture);
            }
        }
        Ok(terms)
    }
    pub(in crate::renderer) fn features(
        &mut self,
        device: &wgpu::Device,
        scheduler: &ConfiguredFrameRenderer,
        outputs: &[CaptureOutput],
    ) -> Result<(), ViewerError> {
        for (output, target) in [
            (CaptureOutput::Specular, FeatureTarget::Specular),
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
                        .feature_capture(target)
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
            native.readback.encode_lanes(
                encoder,
                &native.texture,
                native.lanes,
                native.columns,
                native.stride,
            )?;
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

impl Products {
    pub(in crate::renderer) fn terms(
        &self,
    ) -> &[worldrunner_render_scene::compositor::terms::TermCapture] {
        &self.terms
    }

    pub(in crate::renderer) fn replay(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        resident: &super::super::Resident,
        scheduler: &ConfiguredFrameRenderer,
    ) {
        for term in &self.terms {
            match resident {
                super::super::Resident::Plain(plain) => {
                    scheduler.encode_term_capture(device, encoder, term, (&plain.gpu, None));
                }
                super::super::Resident::Shadowed(scene) => {
                    scene.encode_term_capture_with(device, encoder, term, scheduler);
                }
            }
        }
    }
}
