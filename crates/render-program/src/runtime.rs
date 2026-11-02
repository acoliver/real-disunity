//! The runtime: a device and queue with the resources, modules and pipelines built on them.

use std::borrow::Cow;
use std::collections::HashMap;
use std::future::Future;
use std::pin::pin;
use std::task::{Context, Poll, Waker};

use wgpu::util::DeviceExt;

use crate::error::ProgramError;
use crate::module::StageProgram;
use crate::resources::{
    BufferEntry, BufferHandle, Store, TargetEntry, TargetHandle, TextureEntry, TextureHandle,
};

pub use crate::dispatch::DispatchRequest;
pub use crate::draw::{DrawRequest, Geometry, VertexInput};
pub use crate::program::{ComputeProgram, GraphicsProgram};
pub use crate::readback::TargetImage;
pub use crate::resources::{BoundResource, Resource};

/// Owns a wgpu device and queue, the resources it creates by handle, and caches of shader
/// modules and pipelines keyed by their content.
///
/// Work is submitted to the queue directly; this slice does not go through the frame
/// submission authority of `worldrunner-render-gpu`.
pub struct ProgramRuntime {
    pub(crate) device: wgpu::Device,
    pub(crate) queue: wgpu::Queue,
    pub(crate) store: Store,
    /// The one sampler every sampler binding gets: linear filtering, clamped to the edge.
    pub(crate) sampler: wgpu::Sampler,
    modules: HashMap<blake3::Hash, wgpu::ShaderModule>,
    pub(crate) render_pipelines: HashMap<blake3::Hash, wgpu::RenderPipeline>,
    pub(crate) compute_pipelines: HashMap<blake3::Hash, wgpu::ComputePipeline>,
}

impl ProgramRuntime {
    #[must_use]
    pub fn new(device: wgpu::Device, queue: wgpu::Queue) -> Self {
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("render-program fixed sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..wgpu::SamplerDescriptor::default()
        });
        Self {
            device,
            queue,
            store: Store::default(),
            sampler,
            modules: HashMap::new(),
            render_pipelines: HashMap::new(),
            compute_pipelines: HashMap::new(),
        }
    }

    #[must_use]
    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    #[must_use]
    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    /// Shader modules compiled so far; stages with the same content share one.
    #[must_use]
    pub fn cached_modules(&self) -> usize {
        self.modules.len()
    }

    /// Render and compute pipelines built so far; equal programs and state share one.
    #[must_use]
    pub fn cached_pipelines(&self) -> usize {
        self.render_pipelines.len() + self.compute_pipelines.len()
    }

    /// Uploads `bytes` to a new buffer usable as uniform, storage, vertex and index data.
    pub fn create_buffer(&mut self, label: &str, bytes: &[u8]) -> BufferHandle {
        let buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(label),
                contents: &padded(bytes),
                usage: wgpu::BufferUsages::UNIFORM
                    | wgpu::BufferUsages::STORAGE
                    | wgpu::BufferUsages::VERTEX
                    | wgpu::BufferUsages::INDEX
                    | wgpu::BufferUsages::COPY_SRC
                    | wgpu::BufferUsages::COPY_DST,
            });
        self.store.add_buffer(BufferEntry {
            buffer,
            len: bytes.len() as u64,
        })
    }

    /// Overwrites part of a buffer; the offset and length must be multiples of four.
    ///
    /// # Errors
    /// Fails on an unknown handle or a range outside the buffer.
    pub fn write_buffer(
        &self,
        handle: BufferHandle,
        offset: u64,
        bytes: &[u8],
    ) -> Result<(), ProgramError> {
        let entry = self.store.buffer(handle)?;
        let end = offset.checked_add(bytes.len() as u64);
        if end.is_none_or(|end| end > entry.len) {
            return Err(ProgramError::WriteRange {
                offset,
                len: bytes.len() as u64,
                size: entry.len,
            });
        }
        self.guarded(|| self.queue.write_buffer(&entry.buffer, offset, bytes))
    }

    /// Uploads tightly packed rows of an uncompressed 2D texture.
    ///
    /// # Errors
    /// Fails when `bytes` is not exactly `width * height` texels of `format`.
    pub fn create_texture_2d(
        &mut self,
        label: &str,
        size: [u32; 2],
        format: wgpu::TextureFormat,
        bytes: &[u8],
    ) -> Result<TextureHandle, ProgramError> {
        let texel = format
            .block_copy_size(None)
            .filter(|_| format.block_dimensions() == (1, 1))
            .ok_or_else(|| {
                ProgramError::Gpu(format!("{format:?} is not an uncompressed format"))
            })?;
        let needed = size[0] as usize * size[1] as usize * texel as usize;
        if bytes.len() != needed {
            return Err(ProgramError::TextureData {
                size,
                format,
                needed,
                given: bytes.len(),
            });
        }
        let texture = self.new_texture(
            label,
            size,
            format,
            wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::COPY_SRC,
        );
        self.queue.write_texture(
            texture.as_image_copy(),
            bytes,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(size[0] * texel),
                rows_per_image: Some(size[1]),
            },
            texture.size(),
        );
        Ok(self.store.add_texture(entry(texture)))
    }

    /// Creates a render target with a color texture and, when `depth` is a depth format, a
    /// depth texture.
    ///
    /// # Errors
    /// Fails when `depth` is not a depth format.
    pub fn create_render_target(
        &mut self,
        label: &str,
        size: [u32; 2],
        color: wgpu::TextureFormat,
        depth: Option<wgpu::TextureFormat>,
    ) -> Result<TargetHandle, ProgramError> {
        if let Some(format) = depth.filter(|format| !format.has_depth_aspect()) {
            return Err(ProgramError::Gpu(format!(
                "{format:?} is not a depth format"
            )));
        }
        let attachment = wgpu::TextureUsages::RENDER_ATTACHMENT;
        let color = self.new_texture(
            label,
            size,
            color,
            attachment | wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::TEXTURE_BINDING,
        );
        let depth = depth.map(|format| entry(self.new_texture(label, size, format, attachment)));
        Ok(self.store.add_target(TargetEntry {
            color: entry(color),
            depth,
        }))
    }

    pub fn release_buffer(&mut self, handle: BufferHandle) -> bool {
        self.store.remove_buffer(handle)
    }

    pub fn release_texture(&mut self, handle: TextureHandle) -> bool {
        self.store.remove_texture(handle)
    }

    pub fn release_target(&mut self, handle: TargetHandle) -> bool {
        self.store.remove_target(handle)
    }

    fn new_texture(
        &self,
        label: &str,
        size: [u32; 2],
        format: wgpu::TextureFormat,
        usage: wgpu::TextureUsages,
    ) -> wgpu::Texture {
        self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width: size[0],
                height: size[1],
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage,
            view_formats: &[],
        })
    }

    /// The compiled module for a stage, built from the parsed naga module on first use.
    ///
    /// # Errors
    /// Fails when the device rejects the module.
    pub(crate) fn shader_module(
        &mut self,
        stage: &StageProgram,
    ) -> Result<wgpu::ShaderModule, ProgramError> {
        if let Some(module) = self.modules.get(&stage.content_key()) {
            return Ok(module.clone());
        }
        let module = self.guarded(|| {
            self.device
                .create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: Some(&stage.module().entry_point),
                    source: wgpu::ShaderSource::Naga(Cow::Owned(stage.naga_module().clone())),
                })
        })?;
        self.modules.insert(stage.content_key(), module.clone());
        Ok(module)
    }

    /// Runs `work` and reports a GPU validation error it caused as a named error instead of
    /// letting wgpu's default handler panic.
    pub(crate) fn guarded<T>(&self, work: impl FnOnce() -> T) -> Result<T, ProgramError> {
        let scope = self.device.push_error_scope(wgpu::ErrorFilter::Validation);
        let value = work();
        let mut pending = pin!(scope.pop());
        let mut context = Context::from_waker(Waker::noop());
        // Native backends resolve the scope when it is popped; the poll covers a backend that
        // needs the device to make progress first.
        for _ in 0..PENDING_POLLS {
            if let Poll::Ready(error) = pending.as_mut().poll(&mut context) {
                return error.map_or(Ok(value), |error| Err(ProgramError::Gpu(error.to_string())));
            }
            self.poll_device()?;
        }
        Err(ProgramError::Gpu(
            "the error scope did not resolve".to_owned(),
        ))
    }

    pub(crate) fn poll_device(&self) -> Result<(), ProgramError> {
        self.device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: None,
            })
            .map(|_| ())
            .map_err(|error| ProgramError::Gpu(error.to_string()))
    }
}

/// Times a pending error scope is polled, with the device polled between.
const PENDING_POLLS: usize = 4;

fn entry(texture: wgpu::Texture) -> TextureEntry {
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    TextureEntry { texture, view }
}

/// The bytes, extended with zeros to a multiple of four as buffer copies require.
fn padded(bytes: &[u8]) -> Cow<'_, [u8]> {
    if bytes.len().is_multiple_of(4) {
        return Cow::Borrowed(bytes);
    }
    let mut owned = bytes.to_vec();
    owned.resize(bytes.len().next_multiple_of(4), 0);
    Cow::Owned(owned)
}
