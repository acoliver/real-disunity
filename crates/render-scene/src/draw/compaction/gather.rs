//! The compute dispatch that gathers a view's runs from the mirror into the view buffer.
use std::cell::{OnceCell, RefCell};

use wgpu::Buffer;
use worldrunner_render_gpu::census::{MemoryCensus, ResourceClass, Tracked};
use worldrunner_render_post::PrepareContext;

use super::mirror;
use super::plan::Run;

pub(super) const SOURCE: &str = include_str!("gather.wgsl");

/// Bytes of the run list header (the run count) and of one run.
const HEADER_BYTES: usize = 4;
const RUN_BYTES: usize = 12;

#[derive(Debug)]
struct Pipeline {
    layout: wgpu::BindGroupLayout,
    pipeline: wgpu::ComputePipeline,
}

/// The gather entry points, indexed by [`entry`]: records alone, or with previous transforms,
/// skin descriptors, or both.
#[derive(Debug)]
pub(super) struct Pipelines {
    entries: [Pipeline; 4],
}

/// The bindings each entry point reads (false) or writes (true): the run list, then a mirror
/// and a view buffer for each half it gathers.
const ENTRIES: [(&str, &[(u32, bool)]); 4] = [
    ("gather", &[(0, false), (1, false), (2, true)]),
    (
        "gather_with_previous",
        &[(0, false), (1, false), (2, true), (3, false), (4, true)],
    ),
    (
        "gather_with_skin",
        &[(0, false), (1, false), (2, true), (5, false), (6, true)],
    ),
    (
        "gather_with_previous_and_skin",
        &[
            (0, false),
            (1, false),
            (2, true),
            (3, false),
            (4, true),
            (5, false),
            (6, true),
        ],
    ),
];

/// The entry point that gathers the halves `bindings` names.
fn entry(bindings: &Bindings<'_>) -> usize {
    usize::from(bindings.previous.is_some()) + 2 * usize::from(bindings.skin.is_some())
}

impl Pipelines {
    fn new(device: &wgpu::Device) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("instance gather"),
            source: wgpu::ShaderSource::Wgsl(SOURCE.into()),
        });
        let pipeline = |(entry, bindings): (&str, &[(u32, bool)])| {
            let entries: Vec<_> = bindings
                .iter()
                .map(|&(binding, writable)| wgpu::BindGroupLayoutEntry {
                    binding,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage {
                            read_only: !writable,
                        },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                })
                .collect();
            let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some(entry),
                entries: &entries,
            });
            let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some(entry),
                bind_group_layouts: &[Some(&layout)],
                immediate_size: 0,
            });
            let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(entry),
                layout: Some(&pipeline_layout),
                module: &module,
                entry_point: Some(entry),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                cache: None,
            });
            Pipeline { layout, pipeline }
        };
        Self {
            entries: ENTRIES.map(pipeline),
        }
    }
}

/// The scene-wide half of compaction: the mirror every view gathers from and the pipelines.
#[derive(Debug, Default)]
pub(in crate::draw) struct Shared {
    pub(super) mirror: mirror::Mirror,
    pipelines: OnceCell<Pipelines>,
    /// How many batches `gather_sources` checked one by one.
    #[cfg(test)]
    pub(super) source_checks: std::cell::Cell<usize>,
}

/// The buffers one gather binds, in binding order.
pub(super) struct Bindings<'a> {
    pub mirror: &'a mirror::Buffers,
    pub current: &'a Buffer,
    pub previous: Option<&'a Buffer>,
    /// The view's skin descriptors, gathered from the mirror's skin half by the same runs.
    pub skin: Option<&'a Buffer>,
}

/// A view's run list buffer, reused across frames and grown when a list outgrows it, and the
/// bind group of its last gather. The list counts as instance bytes in the scene's census: its
/// size follows the view's selected ranges.
#[derive(Debug, Default)]
pub(super) struct Scratch {
    runs: Option<Tracked<Buffer>>,
    group: Option<(Vec<Buffer>, wgpu::BindGroup)>,
    /// The bytes of the run list, written again by each gather.
    bytes: Vec<u8>,
}

/// Replaces `bytes` with the run list as the shader reads it: the run count, then each run.
pub(super) fn write_run_bytes(runs: &[Run], bytes: &mut Vec<u8>) {
    let count = u32::try_from(runs.len()).expect("gather runs fit u32");
    bytes.clear();
    bytes.reserve(HEADER_BYTES + RUN_BYTES * runs.len());
    bytes.extend_from_slice(&count.to_le_bytes());
    for run in runs {
        for word in [run.source, run.destination, run.count] {
            bytes.extend_from_slice(&word.to_le_bytes());
        }
    }
}

/// The run list of [`write_run_bytes`] in a new buffer.
#[cfg(test)]
pub(super) fn run_bytes(runs: &[Run]) -> Vec<u8> {
    let mut bytes = Vec::new();
    write_run_bytes(runs, &mut bytes);
    bytes
}

/// One workgroup per run, folded into rows of at most `limit` workgroups.
pub(super) fn workgroups(runs: u32, limit: u32) -> [u32; 3] {
    assert!(runs > 0, "a gather has at least one run");
    let columns = runs.min(limit);
    let rows = runs.div_ceil(columns);
    assert!(
        rows <= limit,
        "{runs} gather runs exceed the device's dispatch limit"
    );
    [columns, rows, 1]
}

impl Shared {
    /// The capture write key of the mirror's published state.
    pub(in crate::draw) fn write_key(
        &self,
    ) -> worldrunner_render_gpu::submission::capture::WriteKey {
        crate::compositor::publication::key(&self.mirror.committed)
    }

    /// Stages one dispatch that writes every run into the view buffers. A grown run list is
    /// created through `census`.
    pub(super) fn dispatch(
        &self,
        context: &mut PrepareContext<'_>,
        (scratch, census): (&RefCell<Scratch>, &MemoryCensus),
        runs: &[Run],
        bindings: &Bindings<'_>,
    ) {
        let device = context.device().clone();
        let pipelines = self.pipelines.get_or_init(|| Pipelines::new(&device));
        let pipeline = &pipelines.entries[entry(bindings)];
        let mut scratch = scratch.borrow_mut();
        let mut bytes = std::mem::take(&mut scratch.bytes);
        write_run_bytes(runs, &mut bytes);
        let list = scratch.runs_holding(&device, census, bytes.len() as u64);
        // The upload copies the bytes, so the buffer holds the next list once this returns.
        context.upload_buffer(&list, 0, &bytes);
        scratch.bytes = bytes;
        let group = scratch.group(&device, &pipeline.layout, &list, bindings);
        let limit = device.limits().max_compute_workgroups_per_dimension;
        let count = u32::try_from(runs.len()).expect("gather runs fit u32");
        context.dispatch(
            "instance gather",
            &pipeline.pipeline,
            &group,
            workgroups(count, limit),
        );
    }
}

impl Scratch {
    /// Bytes of the run list buffer, zero before the first gather.
    #[cfg(test)]
    pub fn bytes(&self) -> u64 {
        self.runs.as_deref().map_or(0, Buffer::size)
    }

    /// Where the run list bytes live and how much they can hold.
    #[cfg(test)]
    pub fn bytes_buffer(&self) -> (usize, usize) {
        (self.bytes.as_ptr() as usize, self.bytes.capacity())
    }

    fn runs_holding(&mut self, device: &wgpu::Device, census: &MemoryCensus, bytes: u64) -> Buffer {
        if let Some(buffer) = self.runs.as_deref().filter(|buffer| buffer.size() >= bytes) {
            return buffer.clone();
        }
        let size = bytes
            .checked_next_power_of_two()
            .expect("gather run list size fits u64");
        let limits = device.limits();
        assert!(
            size <= limits.max_buffer_size && size <= limits.max_storage_buffer_binding_size,
            "gather run list of {size} bytes exceeds the device's storage buffer limits"
        );
        let buffer = census.create_buffer(
            device,
            ResourceClass::Instance,
            &wgpu::BufferDescriptor {
                label: Some("gather runs"),
                size,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            },
        );
        let list = Buffer::clone(&buffer);
        self.runs = Some(buffer);
        list
    }

    /// The bind group of the last gather when it bound the same buffers, otherwise a new one.
    fn group(
        &mut self,
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        list: &Buffer,
        bindings: &Bindings<'_>,
    ) -> wgpu::BindGroup {
        let mut buffers = vec![
            list.clone(),
            Buffer::clone(&bindings.mirror.current),
            bindings.current.clone(),
        ];
        if let Some(previous) = bindings.previous {
            let mirror = bindings
                .mirror
                .previous
                .as_ref()
                .expect("previous mirror for a previous gather");
            buffers.extend([Buffer::clone(mirror), previous.clone()]);
        }
        if let Some(skin) = bindings.skin {
            let mirror = bindings
                .mirror
                .skin
                .as_ref()
                .expect("skin mirror for a skin gather");
            buffers.extend([Buffer::clone(mirror), skin.clone()]);
        }
        if let Some((_, group)) = self.group.as_ref().filter(|(bound, _)| *bound == buffers) {
            return group.clone();
        }
        let (_, slots) = ENTRIES[entry(bindings)];
        let entries: Vec<_> = buffers
            .iter()
            .zip(slots)
            .map(|(buffer, &(binding, _))| wgpu::BindGroupEntry {
                binding,
                resource: buffer.as_entire_binding(),
            })
            .collect();
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("instance gather"),
            layout,
            entries: &entries,
        });
        self.group = Some((buffers, group.clone()));
        group
    }
}
