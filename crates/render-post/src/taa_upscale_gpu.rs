//! Schedules the temporal reconstruction stage. The shader is `taa_upscale.wgsl` after the shared
//! `taa_common.wgsl` helpers, after the reconstruction stages of `taa_reconstruct_gpu`.
//!
//! It reads the jittered current color at the internal extent `R` and writes the output extent
//! `O`. With the thin-feature locks on it also writes the lock bank and copies both into the shared
//! histories; with them off (the default) there are no lock passes and no lock resources. The uniform block is a [`crate::TemporalBlock`], the camera block, read by the reconstruction,
//! a [`crate::TemporalCamera`].
use crate::{
    Stage, StageId, StageSet, StageTarget, taa_mask_gpu, taa_reconstruct_gpu, taa_thin_gpu,
};

/// Stage names, in execution order. Identities are stable for timing. With locks on, the candidate
/// pass, [`crate::taa_thin_gpu::STAGE`], runs before the first of them and "temporal lock history"
/// is the last. Without locks only the first two exist.
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
    /// The lock resources, present only when [`locks_enabled`].
    pub locks: Option<LockInputs<'a>>,
    /// The producer mask at `R`, present when the frame targets carry one.
    pub mask: Option<&'a wgpu::TextureView>,
}

/// The textures of the thin-feature lock path.
pub struct LockInputs<'a> {
    /// [`lock_history`] at `O`.
    pub history: &'a wgpu::Texture,
    /// [`lock_bank`] at `O`.
    pub bank: &'a wgpu::Texture,
}

/// What the stages before "temporal upscale" write.
pub struct Outputs {
    pub reconstructed: taa_reconstruct_gpu::Outputs,
    /// The thin-feature candidate field at `R`, present with locks.
    pub candidates: Option<wgpu::Texture>,
    /// The prepared (dilated) mask at `R`, present with a producer mask.
    pub prepared_mask: Option<wgpu::Texture>,
}

/// Appends the stages of [`taa_reconstruct_gpu`], "temporal upscale", which writes `inputs.output`
/// from the total motion, and "temporal upscale history", a texture copy into `inputs.history`.
/// With `inputs.locks`, "temporal create locks" marks thin feature candidates at `R` before the
/// upscale, which then also writes the lock bank, and "temporal lock history" copies the bank into
/// the lock history. With `inputs.mask`, "temporal mask dilate" prepares the mask after the
/// reconstruction. Returns the resources of the stages before the reconstruction, which the
/// caller keeps alive with the set.
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
    let prepared_mask = inputs.mask.map(|mask| {
        let (dilate, prepared) = taa_mask_gpu::stage(device, mask, inputs.render);
        set.stages.push(dilate);
        prepared
    });
    let candidates = inputs.locks.as_ref().map(|_| {
        let (create_locks, candidates) = taa_thin_gpu::stage(device, inputs.color, inputs.render);
        set.stages.push(create_locks);
        candidates
    });
    let candidate_view = candidates.as_ref().map(view);
    set.stages.push(resolve_stage(
        device,
        inputs,
        &reconstructed,
        candidate_view.as_ref(),
    ));
    set.stages.push(Stage::copy_texture(
        StageId::new(STAGES[1], 0),
        inputs.output,
        inputs.history,
    ));
    if let Some(locks) = &inputs.locks {
        set.stages.push(Stage::copy_texture(
            StageId::new(STAGES[2], 0),
            locks.bank,
            locks.history,
        ));
    }
    Outputs {
        reconstructed,
        candidates,
        prepared_mask,
    }
}

fn view(texture: &wgpu::Texture) -> wgpu::TextureView {
    texture.create_view(&wgpu::TextureViewDescriptor::default())
}

/// The "temporal upscale" stage: reads the colour, the colour history, the dilated motion and the
/// confidence and writes the output. With locks it also reads the candidates and the lock history
/// and writes the lock bank.
fn resolve_stage(
    device: &wgpu::Device,
    inputs: &Inputs<'_>,
    reconstructed: &taa_reconstruct_gpu::Outputs,
    candidates: Option<&wgpu::TextureView>,
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
    let pipeline = pipeline(device, &module, inputs.locks.is_some());
    let (history, motion) = (view(inputs.history), view(&reconstructed.motion));
    let confidence = view(&reconstructed.confidence);
    let entry = |binding, resource| wgpu::BindGroupEntry { binding, resource };
    let mut entries = vec![
        entry(0, wgpu::BindingResource::TextureView(inputs.color)),
        entry(1, wgpu::BindingResource::TextureView(&history)),
        entry(2, wgpu::BindingResource::TextureView(&motion)),
        entry(3, inputs.uniform.as_entire_binding()),
        entry(4, wgpu::BindingResource::TextureView(&confidence)),
    ];
    let lock_history = inputs.locks.as_ref().map(|l| view(l.history));
    if let (Some(candidates), Some(lock_history)) = (candidates, &lock_history) {
        entries.push(entry(5, wgpu::BindingResource::TextureView(candidates)));
        entries.push(entry(6, wgpu::BindingResource::TextureView(lock_history)));
    }
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some(STAGES[0]),
        layout: &pipeline.get_bind_group_layout(0),
        entries: &entries,
    });
    let target = match &inputs.locks {
        Some(locks) => StageTarget::Pair(view(inputs.output), view(locks.bank)),
        None => StageTarget::Texture(view(inputs.output)),
    };
    Stage::fullscreen(StageId::new(STAGES[0], 0), &pipeline, &[&group], target)
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

/// Whether the lock passes and resources exist: [`LOCKS_DEFAULT`] in production. Off means no
/// "temporal create locks", no "temporal lock history", no lock bank and no lock memory.
pub(crate) fn locks_enabled() -> bool {
    #[cfg(test)]
    let on = LOCKING.with(std::cell::Cell::get);
    #[cfg(not(test))]
    let on = LOCKS_DEFAULT;
    on
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

/// The unlocked pipeline has one target and binds no lock texture. The locked one sets the shader's
/// `locks` override, which gates its lock branches.
fn pipeline(
    device: &wgpu::Device,
    module: &wgpu::ShaderModule,
    locks: bool,
) -> wgpu::RenderPipeline {
    let (entry_point, overrides, targets) = if locks {
        (
            "taa_upscale",
            [("locks", 1.0)],
            vec![Some(FORMAT.into()), Some(FORMAT.into())],
        )
    } else {
        (
            "taa_upscale_unlocked",
            [("locks", 0.0)],
            vec![Some(FORMAT.into())],
        )
    };
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
            entry_point: Some(entry_point),
            compilation_options: wgpu::PipelineCompilationOptions {
                constants: &overrides,
                ..Default::default()
            },
            targets: &targets,
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}
