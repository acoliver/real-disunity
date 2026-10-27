//! Bounded offscreen RGBA capture, independent of files, image codecs or provenance.

#[cfg(test)]
use std::sync::mpsc;
#[cfg(test)]
use std::time::{Duration, Instant};

mod linear;
mod pages;
pub use linear::LinearRgbaImage;
use thiserror::Error;
use worldrunner_render_plan::RenderScene;
use worldrunner_render_post::{FrameConfig, Readback, ReadbackError, color_texture};

use crate::device::GpuContext;
use crate::draw::{DrawError, SceneGpu};
use crate::frame::{FrameError, FrameUniform};
use crate::renderer::{ScenePipelines, depth_texture};
use crate::view::{SceneView, Viewport};

/// Default ceiling for a padded staging buffer, separate from device limits.
pub const DEFAULT_MAX_READBACK_BYTES: u64 = 256 * 1024 * 1024;

/// A capture could not be allocated, rendered or read back.
#[derive(Debug, Error)]
pub enum OffscreenError {
    #[error(transparent)]
    TerrainResidency(#[from] crate::terrain::pages::scene::TerrainResidencyError),
    #[error(transparent)]
    Submission(#[from] worldrunner_render_gpu::submission::SubmitError),
    /// Empty images cannot be GPU attachments.
    #[error("offscreen viewport {0:?} has a zero dimension")]
    EmptyViewport(Viewport),
    /// The requested texture exceeds the device's two-dimensional limit.
    #[error("offscreen viewport {viewport:?} exceeds texture dimension limit {limit}")]
    TextureLimit {
        /// Requested extent.
        viewport: Viewport,
        /// Maximum width or height.
        limit: u32,
    },
    /// A row pitch or host allocation cannot be represented.
    #[error("offscreen readback layout overflows")]
    LayoutOverflow,
    /// Padded storage exceeds either the caller's cap or the device buffer limit.
    #[error("offscreen readback needs {required} bytes, exceeding limit {limit}")]
    ReadbackLimit {
        /// Padded allocation size.
        required: u64,
        /// Effective caller/device limit.
        limit: u64,
    },
    /// A copied buffer does not contain all the declared rows.
    #[error("offscreen readback is short: expected {expected} bytes, got {actual}")]
    ShortReadback {
        /// Expected padded length.
        expected: usize,
        /// Actual mapped length.
        actual: usize,
    },
    /// The host could not reserve tightly packed image memory.
    #[error("could not allocate offscreen image: {0}")]
    Allocation(#[source] std::collections::TryReserveError),
    /// Frame preparation failed.
    #[error("offscreen frame preparation failed: {0}")]
    Frame(#[from] FrameError),
    /// Scene preparation failed, retaining the original resource error.
    #[error("offscreen scene upload failed: {0}")]
    Draw(#[from] DrawError),
    /// Polling failed, or the copy/callback exhausted the shared readback deadline.
    #[error("offscreen GPU wait failed: {0}")]
    Poll(#[source] wgpu::PollError),
    /// Mapping the staging buffer failed.
    #[error("offscreen buffer mapping failed: {0}")]
    Map(#[source] wgpu::BufferAsyncError),
    /// The callback channel disconnected without delivering a map result.
    #[error("offscreen map completion was not delivered")]
    MapCallback,
    /// The mapped range was unavailable.
    #[error("offscreen mapped range is unavailable: {0}")]
    MappedRange(#[source] wgpu::MapRangeError),
    /// Shared target/readback validation retained at the canonical boundary.
    #[error(transparent)]
    Post(#[from] worldrunner_render_post::PostError),
    #[error("offscreen output service failed: {0}")]
    Output(#[source] ReadbackError),
}

/// A tightly packed, top-to-bottom RGBA8 image in sRGB colour space.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RgbaImage {
    /// Image dimensions.
    pub viewport: Viewport,
    /// Exactly `width * height * 4` bytes, with no row padding.
    pub pixels: Vec<u8>,
}

/// Checked 256-byte-aligned row pitch for an RGBA8 texture-to-buffer copy.
pub fn aligned_bytes_per_row(width: u32) -> Result<u32, OffscreenError> {
    let bytes =
        (u64::from(width) * 4).next_multiple_of(u64::from(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT));
    u32::try_from(bytes).map_err(|_| OffscreenError::LayoutOverflow)
}

#[derive(Debug)]
struct ReadbackLayout {
    viewport: Viewport,
    bytes_per_row: u32,
    padded_bytes: usize,
    #[cfg(test)]
    image_bytes: usize,
    #[cfg(test)]
    width_bytes: usize,
}

impl ReadbackLayout {
    fn new(
        viewport: Viewport,
        limits: &wgpu::Limits,
        max_bytes: u64,
    ) -> Result<Self, OffscreenError> {
        if viewport.width == 0 || viewport.height == 0 {
            return Err(OffscreenError::EmptyViewport(viewport));
        }
        if viewport.width.max(viewport.height) > limits.max_texture_dimension_2d {
            return Err(OffscreenError::TextureLimit {
                viewport,
                limit: limits.max_texture_dimension_2d,
            });
        }
        #[cfg(test)]
        let width_bytes = u64::from(viewport.width) * 4;
        let bytes_per_row = aligned_bytes_per_row(viewport.width)?;
        let required = u64::from(bytes_per_row) * u64::from(viewport.height);
        let limit = max_bytes.min(limits.max_buffer_size);
        if required > limit {
            return Err(OffscreenError::ReadbackLimit { required, limit });
        }
        let host_size = |size| usize::try_from(size).map_err(|_| OffscreenError::LayoutOverflow);
        Ok(Self {
            viewport,
            bytes_per_row,
            padded_bytes: host_size(required)?,
            #[cfg(test)]
            image_bytes: host_size(width_bytes * u64::from(viewport.height))?,
            #[cfg(test)]
            width_bytes: host_size(width_bytes)?,
        })
    }

    #[cfg(test)]
    fn unpad(&self, bytes: &[u8]) -> Result<Vec<u8>, OffscreenError> {
        worldrunner_render_post::unpad_rows(
            self.bytes_per_row,
            self.width_bytes,
            self.viewport.height,
            bytes,
        )
        .map_err(output_error)
    }
}

/// Retained colour, depth and three independent staging slots for repeated captures.
///
/// Submit custom rendering against the views, then call [`Self::read_rgba8`].
/// The convenience methods use slot zero. Explicit slots retain unread captures independently.
#[derive(Debug)]
pub struct OffscreenTarget {
    color: wgpu::Texture,
    color_view: wgpu::TextureView,
    depth_view: wgpu::TextureView,
    readback: [Readback; 3],
    layout: ReadbackLayout,
    scheduler: Option<crate::compositor::FrameRenderer>,
}

impl OffscreenTarget {
    /// The attachment format expected by pipelines rendering into this target.
    pub const COLOR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

    /// Creates a target after checking dimensions, device limits and the default byte cap.
    pub fn new(device: &wgpu::Device, viewport: Viewport) -> Result<Self, OffscreenError> {
        Self::with_max_readback_bytes(device, viewport, DEFAULT_MAX_READBACK_BYTES)
    }

    /// Creates a target with an explicit per-slot padded staging byte cap, still device-bounded.
    /// Three staging buffers together allocate three times the padded image size. Colour and
    /// depth each need about one image; each read also allocates a tightly packed CPU image.
    /// An encoder containing a capture must be submitted and read before reusing its slot;
    /// abandoning it does not cancel the unread capture.
    pub fn with_max_readback_bytes(
        device: &wgpu::Device,
        viewport: Viewport,
        max_bytes: u64,
    ) -> Result<Self, OffscreenError> {
        let layout = ReadbackLayout::new(viewport, &device.limits(), max_bytes)?;
        let config = FrameConfig::new([viewport.width, viewport.height], Self::COLOR_FORMAT, 1);
        let color = color_texture(device, "offscreen RGBA target", config.size, config.format);
        let color_view = color.create_view(&wgpu::TextureViewDescriptor::default());
        let depth_view =
            depth_texture(device, viewport).create_view(&wgpu::TextureViewDescriptor::default());
        debug_assert_eq!(
            u64::from(layout.bytes_per_row) * u64::from(viewport.height),
            layout.padded_bytes as u64
        );
        let readback = [
            Readback::new(device, config, max_bytes).map_err(OffscreenError::Output)?,
            Readback::new(device, config, max_bytes).map_err(OffscreenError::Output)?,
            Readback::new(device, config, max_bytes).map_err(OffscreenError::Output)?,
        ];
        Ok(Self {
            color,
            color_view,
            depth_view,
            readback,
            layout,
            scheduler: None,
        })
    }

    /// Checked mip-zero, layer-zero color attachment for stock composition.
    pub fn color_attachment(&self) -> worldrunner_render_post::AttachmentView {
        worldrunner_render_post::AttachmentView::new(
            worldrunner_render_post::ResourceRef::new("scene color"),
            self.color_view().texture(),
        )
        .expect("offscreen color subresource")
    }
    /// Checked mip-zero, layer-zero depth attachment for stock composition.
    pub fn depth_attachment(&self) -> worldrunner_render_post::AttachmentView {
        worldrunner_render_post::AttachmentView::new(
            worldrunner_render_post::ResourceRef::new("scene depth"),
            self.depth_view().texture(),
        )
        .expect("offscreen depth subresource")
    }

    /// Returns the retained colour attachment for custom rendering.
    pub const fn color_view(&self) -> &wgpu::TextureView {
        &self.color_view
    }

    /// Returns the retained `Depth32Float` attachment (standard depth, clear to 1.0).
    pub const fn depth_view(&self) -> &wgpu::TextureView {
        &self.depth_view
    }

    /// Returns the validated extent of all attachments.
    pub const fn viewport(&self) -> Viewport {
        self.layout.viewport
    }

    /// Clears to opaque black, draws a resident scene, and reads its pixels.
    /// The caller must set the scene's frame and use pipelines with [`Self::COLOR_FORMAT`].
    pub fn render(
        &mut self,
        context: &GpuContext,
        scene: &SceneGpu,
        pipelines: &ScenePipelines,
    ) -> Result<RgbaImage, OffscreenError> {
        self.wait_for_terrain_pages(context, scene)?;
        let prepared = context
            .submission()
            .prepare(context.device(), |encoder, _| {
                let color = self.color_attachment();
                let depth = self.depth_attachment();
                let renderer = self
                    .scheduler
                    .get_or_insert_with(|| crate::compositor::FrameRenderer::new(context.device()));
                crate::compositor::encode_resident(
                    renderer,
                    context.device(),
                    encoder,
                    (scene, pipelines),
                    (&color, &depth),
                )
                .map_err(worldrunner_render_post::PostError::from)?;
                self.encode_readback(encoder)?;
                Ok::<_, OffscreenError>(())
            })
            .map_err(worldrunner_render_gpu::submission::PrepareError::into_error)?;
        let submission = prepared.submit()?;
        self.read_encoded(context, submission)
    }

    /// Selects a validated linear clear through the compositor, draws and captures.
    pub fn render_with_clear_color(
        &mut self,
        context: &GpuContext,
        scene: &SceneGpu,
        pipelines: &ScenePipelines,
        color: wgpu::Color,
    ) -> Result<RgbaImage, OffscreenError> {
        self.scheduler
            .get_or_insert_with(|| crate::compositor::FrameRenderer::new(context.device()))
            .set_clear_color(color)?;
        self.render(context, scene, pipelines)
    }

    /// Appends capture to the frame encoder, avoiding a second submission.
    /// Uses slot zero and rejects overwriting an unread capture.
    /// Use explicit slots when more than one capture is pending.
    pub fn encode_readback(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
    ) -> Result<(), OffscreenError> {
        self.encode_readback_slot(0, encoder)
    }

    /// Encode into an admitted slot. Unread captures cannot be overwritten.
    pub fn encode_readback_slot(
        &mut self,
        slot: usize,
        encoder: &mut wgpu::CommandEncoder,
    ) -> Result<(), OffscreenError> {
        self.readback[slot]
            .encode(encoder, &self.color)
            .map_err(output_error)
    }

    /// Waits only for a submitted frame that already contains the capture copy.
    pub fn read_encoded(
        &mut self,
        context: &GpuContext,
        submission: wgpu::SubmissionIndex,
    ) -> Result<RgbaImage, OffscreenError> {
        self.read_encoded_slot(context, 0, submission)
    }

    /// Read only the named capture's submission, leaving other slots untouched.
    pub fn read_encoded_slot(
        &mut self,
        context: &GpuContext,
        slot: usize,
        submission: wgpu::SubmissionIndex,
    ) -> Result<RgbaImage, OffscreenError> {
        let pixels = self.readback[slot]
            .read_encoded(context.device(), Some(submission))
            .map_err(output_error)?;
        Ok(RgbaImage {
            viewport: self.viewport(),
            pixels,
        })
    }

    /// Copies previously submitted rendering and strips its padded rows.
    /// Polling and callback delivery share one 30-second deadline; concurrent
    /// device pollers are supported. Always unmaps on success or returned error.
    /// Timeouts return [`OffscreenError::Poll`] with [`wgpu::PollError::Timeout`].
    /// All resources and the supplied context must belong to the same device.
    pub fn read_rgba8(&mut self, context: &GpuContext) -> Result<RgbaImage, OffscreenError> {
        let pixels = self.readback[0]
            .read(context.device(), context.submission(), &self.color)
            .map_err(output_error)?;
        Ok(RgbaImage {
            viewport: self.viewport(),
            pixels,
        })
    }
}

#[cfg(test)]
fn wait_for_map(
    deadline: Instant,
    poll: impl FnOnce(Duration) -> Result<wgpu::PollStatus, wgpu::PollError>,
    receive: impl FnOnce(Duration) -> Result<Result<(), wgpu::BufferAsyncError>, mpsc::RecvTimeoutError>,
) -> Result<(), OffscreenError> {
    worldrunner_render_post::wait_for_map(deadline, poll, receive).map_err(output_error)
}

fn output_error(error: ReadbackError) -> OffscreenError {
    match error {
        ReadbackError::Poll(error) => OffscreenError::Poll(error),
        ReadbackError::Map(error) => OffscreenError::Map(error),
        ReadbackError::Callback => OffscreenError::MapCallback,
        ReadbackError::Range(error) => OffscreenError::MappedRange(error),
        ReadbackError::Short { expected, actual } => {
            OffscreenError::ShortReadback { expected, actual }
        }
        ReadbackError::Allocation(error) => OffscreenError::Allocation(error),
        error => OffscreenError::Output(error),
    }
}

/// Uploads and captures a scene using standard pipelines, without encoding or I/O.
/// For repeated captures retain [`SceneGpu`], [`ScenePipelines`] and [`OffscreenTarget`] instead.
pub fn capture_scene(
    context: &GpuContext,
    scene: &RenderScene,
    view: &SceneView,
) -> Result<RgbaImage, OffscreenError> {
    // Reject oversized targets before uploading any scene resources.
    let mut target = OffscreenTarget::new(context.device(), view.viewport())?;
    let frame = FrameUniform::new(view, scene.effects())?;
    let pipelines = ScenePipelines::new(context.device(), OffscreenTarget::COLOR_FORMAT);
    let gpu = SceneGpu::upload(context.device(), context.submission(), scene, &pipelines)?;
    gpu.set_frame(context.queue(), frame);
    target.render(context, &gpu, &pipelines)
}

#[cfg(test)]
mod tests;
