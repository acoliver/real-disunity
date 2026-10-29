use super::{
    Arc, DirectionalShadowSettings, Extents, GpuContext, OwnedDisplayHandle, RenderScene, Renderer,
    Resident, SceneTexture, SceneView, ViewerError, Viewport, Window,
};
use worldrunner_render_gpu::flight::CompletionTicket;
use worldrunner_render_post::{FrameConfig, contracts::AsyncReadback};
use worldrunner_render_scene::{dynamic::DynamicQuad, offscreen::RgbaImage};

pub(crate) enum SceneSource<'a> {
    Static {
        scene: &'a RenderScene,
        shadows: Option<DirectionalShadowSettings>,
        profile: crate::FrameProfile,
        startup: super::Startup,
    },
    Live,
}

pub(super) struct Upload {
    quads: Vec<DynamicQuad>,
    textures: Option<Vec<SceneTexture>>,
    clear: wgpu::Color,
}

pub(super) struct Pending {
    ticket: CompletionTicket,
    capture: Option<Capture>,
}

pub(crate) struct Outcome {
    pub capture: Option<RgbaImage>,
}

pub(super) struct Capture {
    readback: worldrunner_render_post::Readback,
    mapping: Option<AsyncReadback>,
    viewport: Viewport,
    format: wgpu::TextureFormat,
}

impl Renderer {
    pub async fn from_source(
        display: OwnedDisplayHandle,
        window: Arc<Window>,
        source: SceneSource<'_>,
    ) -> Result<Self, ViewerError> {
        match source {
            SceneSource::Static {
                scene,
                shadows,
                profile,
                startup,
            } => Self::new(display, window, scene, shadows, profile, startup).await,
            SceneSource::Live => {
                let scene = empty_scene();
                Self::new(
                    display,
                    window,
                    &scene,
                    None,
                    crate::FrameProfile::Legacy,
                    2.into(),
                )
                .await
            }
        }
    }

    pub fn render_viewport(&self) -> Viewport {
        let size = self.viewport.unwrap_or(Viewport {
            width: 1,
            height: 1,
        });
        Extents::new(
            [size.width, size.height],
            self.context.device().limits().max_texture_dimension_2d,
        )
        .expect("validated surface extent")
        .expect("nonzero extent")
        .render
    }

    /// Retains CPU extraction until surface acquisition and admission succeed.
    pub fn prepare_live(&mut self, frame: &crate::LiveFrame) -> Result<(), ViewerError> {
        crate::live::startup::validate_frame(frame, self.render_viewport())?;
        let [r, g, b, a] = frame.clear;
        self.live_upload = Some(Upload {
            quads: frame.quads.clone(),
            textures: frame.textures.clone(),
            clear: wgpu::Color { r, g, b, a },
        });
        Ok(())
    }

    /// Polls the specific admitted submission and its asynchronous mapping.
    /// None includes both skipped acquisition and an unfinished submitted frame.
    pub fn render_capture(
        &mut self,
        view: &SceneView,
        capture: bool,
    ) -> Result<Option<Outcome>, ViewerError> {
        self.poll()?;
        if self.live_pending.is_none() {
            if self.viewport.is_none() {
                return Ok(None);
            }
            let mut image = capture
                .then(|| Capture::new(self.context.device(), view.viewport()))
                .transpose()?;
            let mut update: crate::SceneUpdate = Box::new(|_| Ok(()));
            let Some(ticket) = self.render_admitted(view, &mut update, image.as_mut(), None)?
            else {
                return Ok(None);
            };
            if let Some(image) = &mut image {
                image.begin()?;
            }
            self.live_pending = Some(Pending {
                ticket,
                capture: image,
            });
        }
        let pending = self.live_pending.as_mut().expect("submitted live frame");
        if !pending.ticket.is_complete() {
            return Ok(None);
        }
        let capture = if let Some(image) = &mut pending.capture {
            let Some(pixels) = image.try_read()? else {
                return Ok(None);
            };
            Some(pixels)
        } else {
            None
        };
        self.live_pending = None;
        Ok(Some(Outcome { capture }))
    }
}

impl Resident {
    pub(super) fn upload_live(
        &mut self,
        context: &GpuContext,
        upload: Upload,
    ) -> Result<(), ViewerError> {
        let Self::Plain(plain) = self else {
            return Err(ViewerError::UnsupportedScene("dynamic shadow host"));
        };
        if let Some(textures) = upload.textures {
            plain.gpu.set_dynamic_contribution(
                context.device(),
                context.submission(),
                &plain.pipelines,
                &textures,
                &upload.quads,
            )?;
        } else {
            plain.gpu.set_dynamic_quads(
                context.device(),
                context.submission(),
                &plain.pipelines,
                &upload.quads,
            )?;
        }
        plain.clear = Some(upload.clear);
        Ok(())
    }
    pub(super) fn clear_color(&self) -> Option<wgpu::Color> {
        match self {
            Self::Plain(plain) => plain.clear,
            Self::Shadowed(_) => None,
        }
    }
}

impl Capture {
    pub(super) fn new(device: &wgpu::Device, viewport: Viewport) -> Result<Self, ViewerError> {
        Self::with_format(device, viewport, super::super::presentation::COLOR)
    }
    pub(super) fn for_texture(
        device: &wgpu::Device,
        texture: &wgpu::Texture,
    ) -> Result<Self, ViewerError> {
        if !texture.usage().contains(wgpu::TextureUsages::COPY_SRC) {
            return Err(ViewerError::UnsupportedScene("surface capture copies"));
        }
        Self::with_format(
            device,
            Viewport {
                width: texture.width(),
                height: texture.height(),
            },
            texture.format(),
        )
    }
    fn with_format(
        device: &wgpu::Device,
        viewport: Viewport,
        format: wgpu::TextureFormat,
    ) -> Result<Self, ViewerError> {
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

fn empty_scene() -> RenderScene {
    use worldrunner_render_plan::{SceneGeometry, SceneParts, SceneResources};
    RenderScene::new(SceneParts {
        resources: SceneResources::new(vec![], vec![]).expect("empty resources"),
        geometry: SceneGeometry::new(vec![], vec![]).expect("empty geometry"),
        terrain: None,
        effects: None,
    })
    .expect("empty live scene")
}
