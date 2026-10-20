//! Retained binding resources with caller-owned shader contracts.
use wgpu::util::DeviceExt;

/// Uploads encoded bytes without an unsafe host-structure cast. Empty buffers remain valid.
pub fn upload_buffer(
    device: &wgpu::Device,
    _queue: &wgpu::Queue,
    label: &str,
    usage: wgpu::BufferUsages,
    bytes: &[u8],
) -> wgpu::Buffer {
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(label),
        contents: bytes,
        usage: usage | wgpu::BufferUsages::COPY_DST,
    })
}

/// A prepared uniform, texture bindings and sampler.
/// The supplied layout remains authoritative for visibility, dimensions and sample types.
#[derive(Debug)]
pub struct PreparedMaterial {
    pub uniform: wgpu::Buffer,
    pub bind_group: wgpu::BindGroup,
}

#[derive(Clone, Copy, Debug)]
pub struct MaterialDescriptor<'a> {
    pub label: &'a str,
    pub layout: &'a wgpu::BindGroupLayout,
    pub uniform_bytes: &'a [u8],
    pub textures: &'a [&'a wgpu::TextureView],
    pub sampler: &'a wgpu::Sampler,
    /// `None` retains the SDK uniform/textures/sampler consecutive convention.
    pub bindings: Option<MaterialBindings<'a>>,
}

/// Caller-owned shader binding numbers; order matches `MaterialDescriptor::textures`.
#[derive(Clone, Copy, Debug)]
pub struct MaterialBindings<'a> {
    pub uniform: u32,
    pub textures: &'a [u32],
    pub sampler: u32,
}

impl PreparedMaterial {
    /// # Panics
    /// An explicit binding map must supply one number per texture. Layout compatibility and
    /// duplicate binding numbers remain wgpu validation errors under the caller's error policy.
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        descriptor: MaterialDescriptor<'_>,
    ) -> Self {
        let MaterialDescriptor {
            label,
            layout,
            uniform_bytes,
            textures,
            sampler,
            bindings,
        } = descriptor;
        if let Some(bindings) = bindings {
            assert_eq!(
                bindings.textures.len(),
                textures.len(),
                "one binding number per texture is required"
            );
        }
        let uniform = upload_buffer(
            device,
            queue,
            label,
            wgpu::BufferUsages::UNIFORM,
            uniform_bytes,
        );
        let mut entries = Vec::with_capacity(textures.len() + 2);
        entries.push(wgpu::BindGroupEntry {
            binding: bindings.map_or(0, |bindings| bindings.uniform),
            resource: uniform.as_entire_binding(),
        });
        for (index, view) in textures.iter().enumerate() {
            entries.push(wgpu::BindGroupEntry {
                binding: bindings.map_or_else(
                    || u32::try_from(index + 1).expect("binding count fits u32"),
                    |bindings| bindings.textures[index],
                ),
                resource: wgpu::BindingResource::TextureView(view),
            });
        }
        entries.push(wgpu::BindGroupEntry {
            binding: bindings.map_or_else(
                || u32::try_from(textures.len() + 1).expect("binding count fits u32"),
                |bindings| bindings.sampler,
            ),
            resource: wgpu::BindingResource::Sampler(sampler),
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(label),
            layout,
            entries: &entries,
        });
        Self {
            uniform,
            bind_group,
        }
    }
}
