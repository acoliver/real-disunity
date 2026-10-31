//! Schedules the reconstruction of the temporal stage: "temporal depth clear", "temporal
//! reconstruct dilate" and "temporal depth clip", compute passes at the internal extent `R`. The
//! shader is `taa_reconstruct.wgsl`.
use crate::{Operation, Stage, StageId, StageSet};

/// Stage names, in execution order.
pub const STAGES: [&str; 3] = [
    "temporal depth clear",
    "temporal reconstruct dilate",
    "temporal depth clip",
];

/// Format of the dilated depth.
pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R32Float;
/// Format of the dilated total backward motion. Storage textures of two 16 bit float channels
/// are not available without an optional feature, so the motion is written at 32 bits.
pub const MOTION_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rg32Float;

/// Format of the history confidence, 1.0 or 0.0. A single 16 bit float channel is not a core
/// storage texture format, so the plane is written at 32 bits.
pub const CONFIDENCE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R32Float;

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
/// `confidence` is the weight of the history of each texel.
#[derive(Clone, Debug)]
pub struct Outputs {
    pub depth: wgpu::Texture,
    pub motion: wgpu::Texture,
    pub scatter: wgpu::Buffer,
    pub confidence: wgpu::Texture,
}

/// The shader's `dilate` and `clip` overrides: both 1 in production.
#[cfg(not(test))]
const fn overrides() -> [(&'static str, f64); 2] {
    [("dilate", 1.0), ("clip", 1.0)]
}

#[cfg(test)]
thread_local! {
    static DILATION: std::cell::Cell<bool> = const { std::cell::Cell::new(true) };
    static CLIPPING: std::cell::Cell<bool> = const { std::cell::Cell::new(true) };
}

#[cfg(test)]
fn overrides() -> [(&'static str, f64); 2] {
    let on = |switch: &'static std::thread::LocalKey<std::cell::Cell<bool>>| {
        f64::from(u8::from(switch.with(std::cell::Cell::get)))
    };
    [("dilate", on(&DILATION)), ("clip", on(&CLIPPING))]
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

/// While alive, stages built on this thread write confidence 1 everywhere: the path before the
/// depth clip existed.
#[cfg(test)]
pub(crate) struct WithoutClip;

#[cfg(test)]
impl WithoutClip {
    pub(crate) fn new() -> Self {
        CLIPPING.with(|d| d.set(false));
        Self
    }
}

#[cfg(test)]
impl Drop for WithoutClip {
    fn drop(&mut self) {
        CLIPPING.with(|d| d.set(true));
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
            constants: &overrides(),
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

fn outputs(device: &wgpu::Device, size: [u32; 2]) -> Outputs {
    Outputs {
        depth: target(device, "temporal dilated depth", size, DEPTH_FORMAT),
        motion: target(device, "temporal dilated motion", size, MOTION_FORMAT),
        scatter: device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("temporal reconstructed depth"),
            size: 4 * u64::from(size[0]) * u64::from(size[1]),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        }),
        confidence: target(device, "temporal depth confidence", size, CONFIDENCE_FORMAT),
    }
}

/// Appends the three stages and returns the resources they write; the caller decides which set
/// owns them.
pub fn append(device: &wgpu::Device, set: &mut StageSet, inputs: &Inputs<'_>) -> Outputs {
    let out = outputs(device, inputs.size);
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("temporal reconstruct"),
        source: wgpu::ShaderSource::Wgsl(include_str!("taa_reconstruct.wgsl").into()),
    });
    let view =
        |texture: &wgpu::Texture| texture.create_view(&wgpu::TextureViewDescriptor::default());
    let (depth, motion, confidence) = (view(&out.depth), view(&out.motion), view(&out.confidence));
    let entry = |binding, resource| wgpu::BindGroupEntry { binding, resource };
    let clear = [
        entry(0, wgpu::BindingResource::TextureView(inputs.depth)),
        entry(5, out.scatter.as_entire_binding()),
    ];
    let reconstruct = [
        entry(0, wgpu::BindingResource::TextureView(inputs.depth)),
        entry(1, wgpu::BindingResource::TextureView(inputs.motion)),
        entry(2, inputs.camera.as_entire_binding()),
        entry(3, wgpu::BindingResource::TextureView(&depth)),
        entry(4, wgpu::BindingResource::TextureView(&motion)),
        entry(5, out.scatter.as_entire_binding()),
    ];
    let clip = [
        entry(2, inputs.camera.as_entire_binding()),
        entry(5, out.scatter.as_entire_binding()),
        entry(6, wgpu::BindingResource::TextureView(&depth)),
        entry(7, wgpu::BindingResource::TextureView(&motion)),
        entry(8, wgpu::BindingResource::TextureView(&confidence)),
    ];
    for (entry_point, name, bindings) in [
        ("clear", STAGES[0], &clear[..]),
        ("reconstruct", STAGES[1], &reconstruct[..]),
        ("depth_clip", STAGES[2], &clip[..]),
    ] {
        let stage = pass(device, &module, entry_point, (name, bindings, inputs.size));
        set.stages.push(stage);
    }
    out
}
