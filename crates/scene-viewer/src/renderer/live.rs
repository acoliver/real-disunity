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
        frames_in_flight: usize,
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
    buffer: wgpu::Buffer,
    mapping: Option<AsyncReadback>,
    viewport: Viewport,
    pitch: u32,
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
                frames_in_flight,
            } => Self::new(display, window, scene, shadows, profile, frames_in_flight).await,
            SceneSource::Live => {
                let scene = empty_scene();
                Self::new(
                    display,
                    window,
                    &scene,
                    None,
                    crate::FrameProfile::Legacy,
                    2,
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
            let Some(ticket) = self.render_admitted(view, &mut update, image.as_mut())? else {
                return Ok(None);
            };
            if let Some(image) = &mut image {
                image.mapping = Some(AsyncReadback::begin(&image.buffer)?);
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
    fn new(device: &wgpu::Device, viewport: Viewport) -> Result<Self, ViewerError> {
        let config = FrameConfig::new(
            [viewport.width, viewport.height],
            super::super::presentation::COLOR,
            1,
        );
        config.validate(&device.limits())?;
        let pitch = (viewport.width * 4).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
        let size = u64::from(pitch) * u64::from(viewport.height);
        if size > device.limits().max_buffer_size || size > 256 * 1024 * 1024 {
            return Err(worldrunner_render_post::ReadbackError::Limit.into());
        }
        Ok(Self {
            buffer: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("live submitted capture"),
                size,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            mapping: None,
            viewport,
            pitch,
        })
    }
    pub(super) fn encode(&self, encoder: &mut wgpu::CommandEncoder, source: &wgpu::Texture) {
        encoder.copy_texture_to_buffer(
            source.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &self.buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(self.pitch),
                    rows_per_image: Some(self.viewport.height),
                },
            },
            wgpu::Extent3d {
                width: self.viewport.width,
                height: self.viewport.height,
                depth_or_array_layers: 1,
            },
        );
    }
    fn try_read(&mut self) -> Result<Option<RgbaImage>, ViewerError> {
        let Some(bytes) = self
            .mapping
            .as_mut()
            .expect("submitted mapping")
            .try_read()?
        else {
            return Ok(None);
        };
        let pixels = worldrunner_render_post::unpad_rows(
            self.pitch,
            self.viewport.width as usize * 4,
            self.viewport.height,
            &bytes,
        )?;
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
