//! Schedules the CMAA2 passes. The shader is `cmaa2.wgsl`.
//!
//! Frame order: clear the counters, detect edges (compute), process shape candidates (compute),
//! apply the queued blends (compute), present the working image (fullscreen).
use crate::{Cmaa2Settings, Operation, Stage, StageId, StageSet, StageTarget};

/// Stage names, in execution order. Identities are stable for timing.
pub(crate) const STAGES: [&str; 5] = [
    "cmaa2_clear",
    "cmaa2_edges",
    "cmaa2_candidates",
    "cmaa2_apply",
    "cmaa2_present",
];

/// Pixels of one edge workgroup's output: 14 quads of 2x2 pixels in each direction.
const EDGE_GROUP_PIXELS: u32 = 28;
const CANDIDATES_PER_GROUP: u64 = 128;
const LOCATIONS_PER_GROUP: u64 = 32;
/// Workgroups per row of a list dispatch. Longer lists wrap to further rows, which keeps
/// every dimension within `max_compute_workgroups_per_dimension` at any extent.
const DISPATCH_ROW: u64 = 4096;
/// Counters: shape candidates, deferred blend locations, deferred blend items, padding.
const CONTROL_BYTES: u64 = 16;

/// Element counts of the work lists. Like the reference they hold a fraction of the pixel count
/// and drop work items when a pathological image fills them.
struct Capacity {
    quads: u64,
    candidates: u64,
    locations: u64,
    items: u64,
}

impl Capacity {
    fn new(device: &wgpu::Device, [width, height]: [u32; 2]) -> Self {
        let pixels = u64::from(width) * u64::from(height);
        let limits = device.limits();
        let fits = |count: u64, element: u64| {
            count.min(u64::from(limits.max_storage_buffer_binding_size) / element)
        };
        Self {
            quads: u64::from(width.div_ceil(2)) * u64::from(height.div_ceil(2)),
            candidates: fits(pixels.div_ceil(4).max(16), 4),
            locations: fits(pixels.div_ceil(6).max(16), 4),
            items: fits(pixels.div_ceil(2).max(32), 8),
        }
    }
}

/// `[x, y, 1]` workgroup counts covering `groups` groups in rows of `DISPATCH_ROW`.
fn list_dispatch(groups: u64) -> [u32; 3] {
    let x = groups.clamp(1, DISPATCH_ROW);
    let y = groups.div_ceil(x).max(1);
    [
        u32::try_from(x).expect("row width fits u32"),
        u32::try_from(y).unwrap_or(u32::MAX),
        1,
    ]
}

/// Working image and lists owned by one schedule.
struct Inputs {
    work: wgpu::TextureView,
    edges: wgpu::Buffer,
    control: wgpu::Buffer,
    candidates: wgpu::Buffer,
    heads: wgpu::Buffer,
    items: wgpu::Buffer,
    locations: wgpu::Buffer,
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

fn allocate(device: &wgpu::Device, set: &mut StageSet, size: [u32; 2]) -> Inputs {
    let capacity = Capacity::new(device, size);
    let none = wgpu::BufferUsages::empty();
    let work = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("CMAA2 working image"),
        size: crate::targets::extent(size),
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba16Float,
        usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let view = work.create_view(&wgpu::TextureViewDescriptor::default());
    set.resources.textures.push(work);
    Inputs {
        work: view,
        edges: storage(device, "CMAA2 edges", capacity.quads * 4, none),
        control: storage(
            device,
            "CMAA2 counters",
            CONTROL_BYTES,
            wgpu::BufferUsages::COPY_DST,
        ),
        candidates: storage(
            device,
            "CMAA2 shape candidates",
            capacity.candidates * 4,
            none,
        ),
        heads: storage(device, "CMAA2 blend list heads", capacity.quads * 4, none),
        items: storage(device, "CMAA2 blend items", capacity.items * 8, none),
        locations: storage(
            device,
            "CMAA2 blend locations",
            capacity.locations * 4,
            none,
        ),
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
    fn compute(
        &self,
        name: &'static str,
        entries: &[wgpu::BindGroupEntry<'_>],
        dispatch: [u32; 3],
    ) -> Stage {
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

/// Appends the CMAA2 stages.
///
/// `source` is display-referred color (`Rgba16Float`) at `size`. The working image, the work
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
    let builder = Builder {
        device,
        module: device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("CMAA2"),
            source: wgpu::ShaderSource::Wgsl(include_str!("cmaa2.wgsl").into()),
        }),
        constants: cmaa2.constants(),
    };
    let inputs = allocate(device, set, size);
    let capacity = Capacity::new(device, size);
    let [clear, edges, candidates, apply, _] = STAGES;
    set.stages.push(Stage {
        id: StageId::new(clear, 0),
        label: clear,
        enabled: true,
        operation: Operation::Clear(inputs.control.clone()),
    });
    set.stages.push(builder.compute(
        edges,
        &[
            view_entry(0, source),
            view_entry(1, &inputs.work),
            buffer_entry(3, &inputs.edges),
            buffer_entry(4, &inputs.control),
            buffer_entry(5, &inputs.candidates),
            buffer_entry(6, &inputs.heads),
        ],
        [
            size[0].div_ceil(EDGE_GROUP_PIXELS),
            size[1].div_ceil(EDGE_GROUP_PIXELS),
            1,
        ],
    ));
    set.stages.push(builder.compute(
        candidates,
        &[
            view_entry(0, source),
            buffer_entry(3, &inputs.edges),
            buffer_entry(4, &inputs.control),
            buffer_entry(5, &inputs.candidates),
            buffer_entry(6, &inputs.heads),
            buffer_entry(7, &inputs.items),
            buffer_entry(8, &inputs.locations),
        ],
        list_dispatch(capacity.candidates.div_ceil(CANDIDATES_PER_GROUP)),
    ));
    set.stages.push(builder.compute(
        apply,
        &[
            view_entry(0, source),
            view_entry(1, &inputs.work),
            buffer_entry(4, &inputs.control),
            buffer_entry(6, &inputs.heads),
            buffer_entry(7, &inputs.items),
            buffer_entry(8, &inputs.locations),
        ],
        list_dispatch(capacity.locations.div_ceil(LOCATIONS_PER_GROUP)),
    ));
    set.stages
        .push(builder.present(&[view_entry(2, &inputs.work)], output));
    set.resources.buffers.extend([
        inputs.edges,
        inputs.control,
        inputs.candidates,
        inputs.heads,
        inputs.items,
        inputs.locations,
    ]);
}
