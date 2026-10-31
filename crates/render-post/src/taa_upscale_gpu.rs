//! Schedules the temporal reconstruction stage. The shader is `taa_upscale.wgsl` after the shared
//! `taa_common.wgsl` helpers, after the reconstruction stages of `taa_reconstruct_gpu`.
//!
//! It reads the jittered current color at the internal extent `R` and writes the output extent
//! `O` together with the thin-feature lock bank, then copies both into the shared histories. The uniform block is a [`crate::TemporalBlock`], the camera block, read by the reconstruction,
//! a [`crate::TemporalCamera`].
use crate::{Stage, StageId, StageSet, StageTarget, taa_reconstruct_gpu, taa_thin_gpu};

/// Stage names, in execution order. Identities are stable for timing. The candidate pass,
/// [`crate::taa_thin_gpu::STAGE`], runs before the first of them.
pub const STAGES: [&str; 3] = [
    "temporal upscale",
    "temporal upscale history",
    "temporal lock history",
];

/// Format of the output and of the history, and of the current color it reads.
pub const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

/// The output texture "temporal upscale output" at the output extent. Frame slots each own one.
pub fn output(device: &wgpu::Device, size: [u32; 2]) -> wgpu::Texture {
    texture(
        device,
        "temporal upscale output",
        size,
        wgpu::TextureUsages::empty(),
    )
}

/// The history texture "temporal upscale history" at the output extent, shared by every frame
/// slot like the native history.
pub fn history(device: &wgpu::Device, size: [u32; 2]) -> wgpu::Texture {
    texture(
        device,
        "temporal upscale history",
        size,
        wgpu::TextureUsages::COPY_DST,
    )
}

/// The lock bank "temporal lock bank" at `O`, written next to the output. Frame slots each own one.
/// Per texel: frames left, reference luminance, contrast, unused. Four 16 bit lanes, 8 bytes per
/// texel, because two 16 bit float channels are not a core storage format.
pub fn lock_bank(device: &wgpu::Device, size: [u32; 2]) -> wgpu::Texture {
    texture(
        device,
        "temporal lock bank",
        size,
        wgpu::TextureUsages::empty(),
    )
}

/// The lock history "temporal lock history" at `O`, shared by every frame slot like the colour
/// history.
pub fn lock_history(device: &wgpu::Device, size: [u32; 2]) -> wgpu::Texture {
    texture(
        device,
        "temporal lock history",
        size,
        wgpu::TextureUsages::COPY_DST,
    )
}

fn texture(
    device: &wgpu::Device,
    label: &str,
    size: [u32; 2],
    extra: wgpu::TextureUsages,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: crate::targets::extent(size),
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC
            | extra,
        view_formats: &[],
    })
}

/// Views and buffers one reconstruction reads and writes.
pub struct Inputs<'a> {
    /// Jittered scene color at `R`, [`FORMAT`].
    pub color: &'a wgpu::TextureView,
    /// Object residual of the backward motion in uv units at `R`, [`crate::VELOCITY_FORMAT`].
    pub motion: &'a wgpu::TextureView,
    /// Single-sample device depth at `R`, standard Z.
    pub depth: &'a wgpu::TextureView,
    /// The internal extent `R`.
    pub render: [u32; 2],
    pub uniform: &'a wgpu::Buffer,
    /// A [`crate::TemporalCamera`] block.
    pub camera: &'a wgpu::Buffer,
    /// [`history`] at `O`.
    pub history: &'a wgpu::Texture,
    /// [`output`] at `O`.
    pub output: &'a wgpu::Texture,
    /// [`lock_history`] at `O`.
    pub lock_history: &'a wgpu::Texture,
    /// [`lock_bank`] at `O`.
    pub lock_bank: &'a wgpu::Texture,
}

/// What the stages before "temporal upscale" write.
pub struct Outputs {
    pub reconstructed: taa_reconstruct_gpu::Outputs,
    /// The thin-feature candidate field at `R`.
    pub candidates: wgpu::Texture,
}

/// Appends the stages of [`taa_reconstruct_gpu`], "temporal create locks", which marks thin
/// feature candidates at `R`, "temporal upscale", which writes `inputs.output` and
/// `inputs.lock_bank` from the total motion, then "temporal upscale history" and "temporal lock
/// history", texture copies into `inputs.history` and `inputs.lock_history`. Returns the resources
/// of the stages before the reconstruction, which the caller keeps alive with the set.
pub fn append(device: &wgpu::Device, set: &mut StageSet, inputs: &Inputs<'_>) -> Outputs {
    let reconstructed = taa_reconstruct_gpu::append(
        device,
        set,
        &taa_reconstruct_gpu::Inputs {
            depth: inputs.depth,
            motion: inputs.motion,
            camera: inputs.camera,
            size: inputs.render,
        },
    );
    let (create_locks, candidates) = taa_thin_gpu::stage(device, inputs.color, inputs.render);
    set.stages.push(create_locks);
    let candidate_view = view(&candidates);
    set.stages.push(resolve_stage(
        device,
        inputs,
        &reconstructed,
        &candidate_view,
    ));
    set.stages.push(Stage::copy_texture(
        StageId::new(STAGES[1], 0),
        inputs.output,
        inputs.history,
    ));
    set.stages.push(Stage::copy_texture(
        StageId::new(STAGES[2], 0),
        inputs.lock_bank,
        inputs.lock_history,
    ));
    Outputs {
        reconstructed,
        candidates,
    }
}

fn view(texture: &wgpu::Texture) -> wgpu::TextureView {
    texture.create_view(&wgpu::TextureViewDescriptor::default())
}

/// The "temporal upscale" stage: reads the colour, the colour and lock histories, the dilated
/// motion, the confidence and the candidates, and writes the output and the lock bank.
fn resolve_stage(
    device: &wgpu::Device,
    inputs: &Inputs<'_>,
    reconstructed: &taa_reconstruct_gpu::Outputs,
    candidates: &wgpu::TextureView,
) -> Stage {
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("temporal upscale"),
        source: wgpu::ShaderSource::Wgsl(
            concat!(
                include_str!("taa_common.wgsl"),
                include_str!("taa_upscale.wgsl")
            )
            .into(),
        ),
    });
    let pipeline = pipeline(device, &module);
    let (history, motion) = (view(inputs.history), view(&reconstructed.motion));
    let (confidence, lock_history) = (view(&reconstructed.confidence), view(inputs.lock_history));
    let entry = |binding, resource| wgpu::BindGroupEntry { binding, resource };
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some(STAGES[0]),
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[
            entry(0, wgpu::BindingResource::TextureView(inputs.color)),
            entry(1, wgpu::BindingResource::TextureView(&history)),
            entry(2, wgpu::BindingResource::TextureView(&motion)),
            entry(3, inputs.uniform.as_entire_binding()),
            entry(4, wgpu::BindingResource::TextureView(&confidence)),
            entry(5, wgpu::BindingResource::TextureView(candidates)),
            entry(6, wgpu::BindingResource::TextureView(&lock_history)),
        ],
    });
    Stage::fullscreen(
        StageId::new(STAGES[0], 0),
        &pipeline,
        &[&group],
        StageTarget::Pair(view(inputs.output), view(inputs.lock_bank)),
    )
}

/// Whether the thin-feature locks run by default. They are off: the lock rules of the
/// coordinator (at least 10% lower ROI error than the path without locks on the designated thin
/// features) were not met in the first measurement, so the plumbing ships and the feature stays
/// off until it is re-measured. See the temporal section of `docs/rendering.md`.
const LOCKS_DEFAULT: bool = false;

#[cfg(test)]
thread_local! {
    static LOCKING: std::cell::Cell<bool> = const { std::cell::Cell::new(LOCKS_DEFAULT) };
}

/// The fragment shader's `locks` override: [`LOCKS_DEFAULT`] in production.
fn overrides() -> [(&'static str, f64); 1] {
    #[cfg(test)]
    let on = LOCKING.with(std::cell::Cell::get);
    #[cfg(not(test))]
    let on = LOCKS_DEFAULT;
    [("locks", f64::from(u8::from(on)))]
}

/// While alive, stages built on this thread run with the thin-feature locks off: the path of PR 4.
/// A test measures it next to the lock path on the same device.
#[cfg(test)]
pub(crate) struct WithoutLocks;

#[cfg(test)]
impl WithoutLocks {
    pub(crate) fn new() -> Self {
        LOCKING.with(|d| d.set(false));
        Self
    }
}

#[cfg(test)]
impl Drop for WithoutLocks {
    fn drop(&mut self) {
        LOCKING.with(|d| d.set(LOCKS_DEFAULT));
    }
}

/// While alive, stages built on this thread run with the thin-feature locks on.
#[cfg(test)]
pub(crate) struct WithLocks;

#[cfg(test)]
impl WithLocks {
    pub(crate) fn new() -> Self {
        LOCKING.with(|d| d.set(true));
        Self
    }
}

#[cfg(test)]
impl Drop for WithLocks {
    fn drop(&mut self) {
        LOCKING.with(|d| d.set(LOCKS_DEFAULT));
    }
}

fn pipeline(device: &wgpu::Device, module: &wgpu::ShaderModule) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("taa_upscale"),
        layout: None,
        vertex: wgpu::VertexState {
            module,
            entry_point: Some("vertex"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module,
            entry_point: Some("taa_upscale"),
            compilation_options: wgpu::PipelineCompilationOptions {
                constants: &overrides(),
                ..Default::default()
            },
            targets: &[Some(FORMAT.into()), Some(FORMAT.into())],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}
