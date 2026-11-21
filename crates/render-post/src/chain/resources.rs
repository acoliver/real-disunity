//! The buffers and textures a post chain owns for its stages: fixed history and configuration
//! buffers made once, and the sized allocations of each build.
use wgpu::util::DeviceExt;

use crate::PostError;

/// Opaque caller-authored ABI. The chain allocates these once; resize never resets history.
#[derive(Clone, Debug)]
pub struct BufferSpec<'a> {
    pub label: &'a str,
    pub size: u64,
    pub usage: wgpu::BufferUsages,
    pub initial: &'a [u8],
}

/// Explicit owner for fixed history/config buffers and stage-local sized allocations.
#[derive(Default, Debug)]
pub struct Resources {
    pub buffers: Vec<wgpu::Buffer>,
    pub textures: Vec<wgpu::Texture>,
}

impl Resources {
    pub fn new(device: &wgpu::Device, specs: &[BufferSpec<'_>]) -> Result<Self, PostError> {
        let mut resources = Self::default();
        for spec in specs {
            if spec.size == 0
                || spec.size > device.limits().max_buffer_size
                || u64::try_from(spec.initial.len()).unwrap_or(u64::MAX) > spec.size
                || spec.size % wgpu::COPY_BUFFER_ALIGNMENT != 0
            {
                return Err(PostError::BufferSize(spec.size));
            }
            let buffer = if spec.initial.is_empty() {
                device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some(spec.label),
                    size: spec.size,
                    usage: spec.usage,
                    mapped_at_creation: false,
                })
            } else {
                let mut bytes = vec![
                    0;
                    usize::try_from(spec.size)
                        .map_err(|_| PostError::BufferSize(spec.size))?
                ];
                bytes[..spec.initial.len()].copy_from_slice(spec.initial);
                device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some(spec.label),
                    contents: &bytes,
                    usage: spec.usage,
                })
            };
            resources.buffers.push(buffer);
        }
        Ok(resources)
    }
}
