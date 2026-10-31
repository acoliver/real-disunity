//! Schedules the reconstruction of the temporal stage: "temporal depth clear" and "temporal
//! reconstruct dilate", both compute passes at the internal extent `R`. The shader is
//! `taa_reconstruct.wgsl`.
use crate::{Operation, Stage, StageId, StageSet};

/// Stage names, in execution order.
pub const STAGES: [&str; 2] = ["temporal depth clear", "temporal reconstruct dilate"];

/// Format of the dilated depth.
pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R32Float;
/// Format of the dilated total backward motion. Storage textures of two 16 bit float channels
/// are not available without an optional feature, so the motion is written at 32 bits.
pub const MOTION_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rg32Float;

/// Single-sample depth, residual motion and camera block at `R`.
pub struct Inputs<'a> {
    pub depth: &'a wgpu::TextureView,
    pub motion: &'a wgpu::TextureView,
    pub camera: &'a wgpu::Buffer,
    pub size: [u32; 2],
}

/// What the stages write. The textures are `TEXTURE_BINDING | COPY_SRC` for the resolve and for
/// diagnostic readback; the scatter buffer holds one `u32` per texel, row major, the f32 bits of
/// the nearest depth that reaches the texel from the current frame, or the bits of +infinity.
#[derive(Clone, Debug)]
pub struct Outputs {
    pub depth: wgpu::Texture,
    pub motion: wgpu::Texture,
    pub scatter: wgpu::Buffer,
}

/// The shader's `dilate` override: 1 in production.
#[cfg(not(test))]
const fn dilation() -> f64 {
    1.0
}

#[cfg(test)]
thread_local! {
    static DILATION: std::cell::Cell<bool> = const { std::cell::Cell::new(true) };
}

#[cfg(test)]
fn dilation() -> f64 {
    f64::from(u8::from(DILATION.with(std::cell::Cell::get)))
}

/// While alive, stages built on this thread keep each texel's own depth and residual instead of
/// the nearest of the 3x3 neighbourhood. A test measures the path before the dilation existed
/// next to the real one on the same device.
#[cfg(test)]
pub(crate) struct WithoutDilation;

#[cfg(test)]
impl WithoutDilation {
    pub(crate) fn new() -> Self {
        DILATION.with(|d| d.set(false));
        Self
    }
}

#[cfg(test)]
impl Drop for WithoutDilation {
    fn drop(&mut self) {
        DILATION.with(|d| d.set(true));
    }
}

fn target(
    device: &wgpu::Device,
    label: &str,
    size: [u32; 2],
    format: wgpu::TextureFormat,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: crate::targets::extent(size),
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::STORAGE_BINDING
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}

fn pass(
    device: &wgpu::Device,
    module: &wgpu::ShaderModule,
    entry: &'static str,
    (name, entries, size): (&'static str, &[wgpu::BindGroupEntry<'_>], [u32; 2]),
) -> Stage {
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some(name),
        layout: None,
        module,
        entry_point: Some(entry),
        compilation_options: wgpu::PipelineCompilationOptions {
            constants: &[("dilate", dilation())],
            ..Default::default()
        },
        cache: None,
    });
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some(name),
        layout: &pipeline.get_bind_group_layout(0),
        entries,
    });
    Stage {
        id: StageId::new(name, 0),
        label: name,
        enabled: true,
        operation: Operation::Compute {
            pipeline,
            groups: vec![group],
            dispatch: [size[0].div_ceil(8), size[1].div_ceil(8), 1],
        },
    }
}

/// Appends the two stages and returns the resources they write; the caller decides which set
/// owns them.
pub fn append(device: &wgpu::Device, set: &mut StageSet, inputs: &Inputs<'_>) -> Outputs {
    let depth = target(device, "temporal dilated depth", inputs.size, DEPTH_FORMAT);
    let motion = target(
        device,
        "temporal dilated motion",
        inputs.size,
        MOTION_FORMAT,
    );
    let scatter = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("temporal reconstructed depth"),
        size: 4 * u64::from(inputs.size[0]) * u64::from(inputs.size[1]),
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("temporal reconstruct"),
        source: wgpu::ShaderSource::Wgsl(include_str!("taa_reconstruct.wgsl").into()),
    });
    let depth_view = depth.create_view(&wgpu::TextureViewDescriptor::default());
    let motion_view = motion.create_view(&wgpu::TextureViewDescriptor::default());
    let entry = |binding, resource| wgpu::BindGroupEntry { binding, resource };
    let clear = [
        entry(0, wgpu::BindingResource::TextureView(inputs.depth)),
        entry(5, scatter.as_entire_binding()),
    ];
    let reconstruct = [
        entry(0, wgpu::BindingResource::TextureView(inputs.depth)),
        entry(1, wgpu::BindingResource::TextureView(inputs.motion)),
        entry(2, inputs.camera.as_entire_binding()),
        entry(3, wgpu::BindingResource::TextureView(&depth_view)),
        entry(4, wgpu::BindingResource::TextureView(&motion_view)),
        entry(5, scatter.as_entire_binding()),
    ];
    set.stages.push(pass(
        device,
        &module,
        "clear",
        (STAGES[0], &clear, inputs.size),
    ));
    set.stages.push(pass(
        device,
        &module,
        "reconstruct",
        (STAGES[1], &reconstruct, inputs.size),
    ));
    Outputs {
        depth,
        motion,
        scatter,
    }
}
