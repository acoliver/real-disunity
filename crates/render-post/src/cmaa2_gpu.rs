//! Schedules the CMAA2 passes. The shader is `cmaa2.wgsl`.
//!
//! Frame order: clear the counters, detect edges (compute), write the candidate dispatch
//! arguments (one thread), process shape candidates (compute, indirect), then the fullscreen
//! pass that resolves the queued blends while it writes the output.
use worldrunner_render_gpu::configuration::AllocationSet;

use crate::{Cmaa2Settings, Operation, Stage, StageId, StageSet, StageTarget};

/// Stage names, in execution order. Identities are stable for timing.
pub(crate) const STAGES: [&str; 5] = [
    "cmaa2_clear",
    "cmaa2_edges",
    "cmaa2_candidate_args",
    "cmaa2_candidates",
    "cmaa2_present",
];

/// Pixels of one edge workgroup's output: 14 quads of 2x2 pixels in each direction.
const EDGE_GROUP_PIXELS: u32 = 28;
/// Counters: shape candidates and deferred blend items.
const CONTROL_BYTES: u64 = 8;
/// Indirect dispatch arguments: three `u32` workgroup counts for the candidate pass.
const ARGUMENT_BYTES: u64 = 12;

/// Element counts of the work lists. Like the reference they hold a fraction of the pixel count
/// and drop work items when a pathological image fills them.
struct Capacity {
    quads: u64,
    candidates: u64,
    items: u64,
}

impl Capacity {
    /// Capacities for `size`, none larger than one storage binding of `max_binding` bytes.
    fn new(max_binding: u64, [width, height]: [u32; 2]) -> Self {
        let pixels = u64::from(width) * u64::from(height);
        let fits = |count: u64, element: u64| count.min(max_binding / element);
        Self {
            quads: u64::from(width.div_ceil(2)) * u64::from(height.div_ceil(2)),
            candidates: fits(pixels.div_ceil(4).max(16), 4),
            items: fits(pixels.div_ceil(2).max(32), 8),
        }
    }
}

/// Declares the CMAA2 storage for `size`, in step with `allocate`, plus the display-referred
/// input image that the resolve reads.
pub(crate) fn allocations(set: &mut AllocationSet, size: [u32; 2]) {
    let capacity = Capacity::new(u64::MAX, size);
    let extent = [size[0], size[1], 1];
    let format = wgpu::TextureFormat::Rgba16Float;
    set.texture("display-linear post input", extent, format, [1, 1], 3);
    set.buffer("CMAA2 edges", capacity.quads * 4, 3);
    set.buffer("CMAA2 counters", CONTROL_BYTES, 3);
    set.buffer("CMAA2 dispatch arguments", ARGUMENT_BYTES, 3);
    set.buffer("CMAA2 shape candidates", capacity.candidates * 4, 3);
    set.buffer("CMAA2 blend list heads", capacity.quads * 4, 3);
    set.buffer("CMAA2 blend items", capacity.items * 8, 3);
}

/// Work lists owned by one schedule.
struct Inputs {
    edges: wgpu::Buffer,
    control: wgpu::Buffer,
    arguments: wgpu::Buffer,
    candidates: wgpu::Buffer,
    heads: wgpu::Buffer,
    items: wgpu::Buffer,
}

fn storage(
    device: &wgpu::Device,
    label: &str,
    size: u64,
    extra: wgpu::BufferUsages,
) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size,
        usage: wgpu::BufferUsages::STORAGE | extra,
        mapped_at_creation: false,
    })
}

fn allocate(device: &wgpu::Device, size: [u32; 2]) -> Inputs {
    let capacity = Capacity::new(device.limits().max_storage_buffer_binding_size, size);
    let none = wgpu::BufferUsages::empty();
    Inputs {
        edges: storage(device, "CMAA2 edges", capacity.quads * 4, none),
        control: storage(
            device,
            "CMAA2 counters",
            CONTROL_BYTES,
            wgpu::BufferUsages::COPY_DST,
        ),
        arguments: storage(
            device,
            "CMAA2 dispatch arguments",
            ARGUMENT_BYTES,
            wgpu::BufferUsages::INDIRECT,
        ),
        candidates: storage(
            device,
            "CMAA2 shape candidates",
            capacity.candidates * 4,
            none,
        ),
        heads: storage(device, "CMAA2 blend list heads", capacity.quads * 4, none),
        items: storage(device, "CMAA2 blend items", capacity.items * 8, none),
    }
}

fn view_entry(binding: u32, view: &wgpu::TextureView) -> wgpu::BindGroupEntry<'_> {
    wgpu::BindGroupEntry {
        binding,
        resource: wgpu::BindingResource::TextureView(view),
    }
}

fn buffer_entry(binding: u32, buffer: &wgpu::Buffer) -> wgpu::BindGroupEntry<'_> {
    wgpu::BindGroupEntry {
        binding,
        resource: buffer.as_entire_binding(),
    }
}

struct Builder<'a> {
    device: &'a wgpu::Device,
    module: wgpu::ShaderModule,
    constants: [(&'static str, f64); 2],
}

impl Builder<'_> {
    fn pipeline_and_group(
        &self,
        name: &'static str,
        entries: &[wgpu::BindGroupEntry<'_>],
    ) -> (wgpu::ComputePipeline, wgpu::BindGroup) {
        let pipeline = self
            .device
            .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(name),
                layout: None,
                module: &self.module,
                entry_point: Some(name),
                compilation_options: wgpu::PipelineCompilationOptions {
                    constants: &self.constants,
                    ..Default::default()
                },
                cache: None,
            });
        let group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(name),
            layout: &pipeline.get_bind_group_layout(0),
            entries,
        });
        (pipeline, group)
    }

    fn compute(
        &self,
        name: &'static str,
        entries: &[wgpu::BindGroupEntry<'_>],
        dispatch: [u32; 3],
    ) -> Stage {
        let (pipeline, group) = self.pipeline_and_group(name, entries);
        Stage {
            id: StageId::new(name, 0),
            label: name,
            enabled: true,
            operation: Operation::Compute {
                pipeline,
                groups: vec![group],
                dispatch,
            },
        }
    }

    /// A compute stage whose workgroup counts the argument stage wrote at `offset`.
    fn indirect(
        &self,
        name: &'static str,
        entries: &[wgpu::BindGroupEntry<'_>],
        (arguments, offset): (&wgpu::Buffer, u64),
    ) -> Stage {
        let (pipeline, group) = self.pipeline_and_group(name, entries);
        Stage {
            id: StageId::new(name, 0),
            label: name,
            enabled: true,
            operation: Operation::ComputeIndirect {
                pipeline,
                groups: vec![group],
                arguments: arguments.clone(),
                offset,
            },
        }
    }

    fn present(
        &self,
        entries: &[wgpu::BindGroupEntry<'_>],
        output: (wgpu::TextureFormat, StageTarget),
    ) -> Stage {
        let name = STAGES[4];
        let pipeline = self
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(name),
                layout: None,
                vertex: wgpu::VertexState {
                    module: &self.module,
                    entry_point: Some("vertex"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    buffers: &[],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &self.module,
                    entry_point: Some(name),
                    compilation_options: wgpu::PipelineCompilationOptions {
                        constants: &self.constants,
                        ..Default::default()
                    },
                    targets: &[Some(output.0.into())],
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            });
        let group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(name),
            layout: &pipeline.get_bind_group_layout(0),
            entries,
        });
        Stage::fullscreen(StageId::new(name, 0), &pipeline, &[&group], output.1)
    }
}

/// Everything the passes bind, so each pass is built by one short method.
struct Schedule<'a> {
    builder: Builder<'a>,
    source: &'a wgpu::TextureView,
    inputs: Inputs,
    size: [u32; 2],
}

impl Schedule<'_> {
    fn clear(&self) -> Stage {
        let name = STAGES[0];
        Stage {
            id: StageId::new(name, 0),
            label: name,
            enabled: true,
            operation: Operation::Clear(self.inputs.control.clone()),
        }
    }

    fn edges(&self) -> Stage {
        let inputs = &self.inputs;
        self.builder.compute(
            STAGES[1],
            &[
                view_entry(0, self.source),
                buffer_entry(3, &inputs.edges),
                buffer_entry(4, &inputs.control),
                buffer_entry(5, &inputs.candidates),
                buffer_entry(6, &inputs.heads),
            ],
            [
                self.size[0].div_ceil(EDGE_GROUP_PIXELS),
                self.size[1].div_ceil(EDGE_GROUP_PIXELS),
                1,
            ],
        )
    }

    /// One thread turns the candidate counter into the dispatch arguments of the candidate pass.
    fn arguments(&self) -> Stage {
        let inputs = &self.inputs;
        self.builder.compute(
            STAGES[2],
            &[
                buffer_entry(4, &inputs.control),
                buffer_entry(5, &inputs.candidates),
                buffer_entry(9, &inputs.arguments),
            ],
            [1, 1, 1],
        )
    }

    /// The fullscreen pass reads the heads and items through read-only bindings.
    fn present(&self, output: (wgpu::TextureFormat, StageTarget)) -> Stage {
        let inputs = &self.inputs;
        self.builder.present(
            &[
                view_entry(0, self.source),
                buffer_entry(10, &inputs.heads),
                buffer_entry(11, &inputs.items),
            ],
            output,
        )
    }

    fn candidates(&self) -> Stage {
        let inputs = &self.inputs;
        self.builder.indirect(
            STAGES[3],
            &[
                view_entry(0, self.source),
                buffer_entry(3, &inputs.edges),
                buffer_entry(4, &inputs.control),
                buffer_entry(5, &inputs.candidates),
                buffer_entry(6, &inputs.heads),
                buffer_entry(7, &inputs.items),
            ],
            (&inputs.arguments, 0),
        )
    }
}

/// Appends the CMAA2 stages.
///
/// `source` is display-referred color (`Rgba16Float`) at `size`. The work
/// lists and their counters belong to `set`, so a resize or reconfiguration rebuilds them with
/// the rest of the schedule.
pub(crate) fn append(
    device: &wgpu::Device,
    set: &mut StageSet,
    source: &wgpu::TextureView,
    size: [u32; 2],
    cmaa2: Cmaa2Settings,
    output: (wgpu::TextureFormat, StageTarget),
) {
    let schedule = Schedule {
        builder: Builder {
            device,
            module: device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("CMAA2"),
                source: wgpu::ShaderSource::Wgsl(include_str!("cmaa2.wgsl").into()),
            }),
            constants: cmaa2.constants(),
        },
        source,
        inputs: allocate(device, size),
        size,
    };
    set.stages.extend([
        schedule.clear(),
        schedule.edges(),
        schedule.arguments(),
        schedule.candidates(),
        schedule.present(output),
    ]);
    let inputs = schedule.inputs;
    set.resources.buffers.extend([
        inputs.edges,
        inputs.control,
        inputs.arguments,
        inputs.candidates,
        inputs.heads,
        inputs.items,
    ]);
}
