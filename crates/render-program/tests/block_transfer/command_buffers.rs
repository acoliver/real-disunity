//! What a prepared dispatch costs in command buffers when a frame records it as prepared
//! commands. wgpu-core opens two for each compute pass and Metal refuses the one after its 4096 not
//! submitted, so the largest number of dispatches one frame of a fresh device submits counts them
//! (`largest_submitting_count`). Other backends have no such cap and every count submits.

use std::sync::{Arc, Mutex};

use worldrunner_render_gpu::test_support::largest_submitting_count;
use worldrunner_render_post::{PrepareContext, PreparedCommands};
use worldrunner_render_program::runtime::BoundResource;

use crate::common::{Gpu, texture, writer_dispatch, writer_program};
use crate::encoded::bc4_blocks;

/// More dispatches than Metal's 4096 command buffers hold at one or more each.
const CAP: usize = 4097;

const GRID: [u32; 2] = [2, 2];

const COUNTER: &str = "
@group(0) @binding(0) var<storage, read_write> count: array<u32>;
@compute @workgroup_size(1) fn main() { count[0] = count[0] + 1u; }
";

#[derive(Clone, Copy)]
enum Unit {
    /// [`PrepareContext::dispatch`] of a counting kernel.
    Context,
    /// A runtime's prepared dispatch, recorded as its [`PreparedCommands`].
    Prepared,
}

/// Submits one frame of `units` dispatches recorded as `unit` on a fresh device; `None` when the
/// adapter has no block compression for the runtime's kernel, otherwise the errors the device
/// reported when it refused the frame.
fn submit(unit: Unit, units: usize) -> Option<Result<(), Vec<String>>> {
    let gpu = Gpu::new()?;
    let errors = Arc::new(Mutex::new(Vec::new()));
    let sink = errors.clone();
    gpu.device
        .on_uncaptured_error(Arc::new(move |error: wgpu::Error| {
            sink.lock().unwrap().push(format!("uncaptured: {error}"));
        }));
    let sink = errors.clone();
    gpu.device.set_device_lost_callback(move |reason, message| {
        sink.lock()
            .unwrap()
            .push(format!("lost: {reason:?} {message}"));
    });
    let mut frame = gpu.authority.begin(&gpu.device).unwrap();
    match unit {
        Unit::Context => {
            let (pipeline, group) = counter(&gpu.device);
            let mut context = PrepareContext::new(&gpu.device, &mut frame).unwrap();
            for _ in 0..units {
                context.dispatch("count", &pipeline, &group, [1, 1, 1]);
            }
            context.finish();
        }
        Unit::Prepared => {
            let blocks: Vec<Vec<u8>> = bc4_blocks().iter().map(|block| block.bytes()).collect();
            let mut runtime = gpu.runtime();
            let image = texture(&mut runtime, wgpu::TextureFormat::Rg32Uint, GRID, 1, 1);
            let program = writer_program(&runtime, GRID, &blocks).unwrap();
            let bindings = [BoundResource::texture(0, 0, image)];
            let dispatch = runtime
                .prepare_dispatch(&writer_dispatch(&program, &bindings, GRID, &blocks))
                .unwrap();
            PreparedCommands::retain(&dispatch, &frame).unwrap();
            for _ in 0..units {
                PreparedCommands::record(&dispatch, &mut frame, "dispatch").unwrap();
            }
        }
    }
    if let Err(error) = frame.finish().submit() {
        errors.lock().unwrap().push(format!("submit: {error}"));
    }
    let _ = gpu.device.poll(wgpu::PollType::wait_indefinitely());
    let errors = errors.lock().unwrap().clone();
    Some(if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    })
}

/// A kernel that counts its invocations into a buffer, and its bind group.
fn counter(device: &wgpu::Device) -> (wgpu::ComputePipeline, wgpu::BindGroup) {
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("count"),
        source: wgpu::ShaderSource::Wgsl(COUNTER.into()),
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("count"),
        layout: None,
        module: &module,
        entry_point: Some("main"),
        compilation_options: wgpu::PipelineCompilationOptions::default(),
        cache: None,
    });
    let count = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("count"),
        size: 16,
        usage: wgpu::BufferUsages::STORAGE,
        mapped_at_creation: false,
    });
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("count"),
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: count.as_entire_binding(),
        }],
    });
    (pipeline, group)
}

#[test]
fn a_prepared_dispatch_recorded_as_prepared_commands_costs_no_more_than_a_context_dispatch() {
    if Gpu::new().is_none() {
        return;
    }
    let (largest, refusal) =
        largest_submitting_count(CAP, |units| submit(Unit::Context, units).unwrap());
    if let Some(refusal) = refusal {
        assert!(
            refusal.iter().any(|error| error.starts_with("lost:")),
            "the frame after {largest} dispatches was refused for another reason than the command \
             buffer cap: {refusal:?}"
        );
    }
    assert_eq!(
        submit(Unit::Prepared, largest).unwrap(),
        Ok(()),
        "one frame submits {largest} context dispatches but not as many prepared ones"
    );
}
