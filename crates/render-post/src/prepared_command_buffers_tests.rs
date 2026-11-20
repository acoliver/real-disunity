//! What a dispatch staged on the frame's preparation costs in command buffers. wgpu-core opens
//! two for each compute pass and Metal refuses the one after its 4096 not submitted, so the
//! largest number of dispatches one frame of a fresh device submits counts them
//! ([`largest_submitting_count`]). Other backends have no such cap and every count submits.

use std::sync::{Arc, Mutex};

use worldrunner_render_gpu::device::{DeviceRequirements, request_device};
use worldrunner_render_gpu::submission::SubmissionAuthority;
use worldrunner_render_gpu::test_support::{TestGpu, largest_submitting_count};

use crate::{PrepareContext, PreparedAccess, PreparedCommands, PreparedRefusal, PreparedWrite};

/// More dispatches than Metal's 4096 command buffers hold at one or more each.
const CAP: usize = 4097;

const COUNTER: &str = "
@group(0) @binding(0) var<storage, read_write> count: array<u32>;
@compute @workgroup_size(1) fn main() { count[0] = count[0] + 1u; }
";

/// A prepared dispatch as a program runtime's records it: one compute pass of its own, labelled.
struct PassDispatch {
    pipeline: wgpu::ComputePipeline,
    group: wgpu::BindGroup,
}

impl PreparedCommands for PassDispatch {
    fn writes(&self) -> Vec<PreparedWrite> {
        Vec::new()
    }

    fn retain(&self, _encoder: &wgpu::CommandEncoder) -> Result<(), PreparedRefusal> {
        Ok(())
    }

    fn record(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        label: &str,
    ) -> Result<(), PreparedRefusal> {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some(label),
            timestamp_writes: None,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.group, &[]);
        pass.dispatch_workgroups(1, 1, 1);
        Ok(())
    }
}

#[derive(Clone, Copy)]
enum Staging {
    /// [`PrepareContext::dispatch`].
    Context,
    /// [`PrepareContext::record_prepared`] of a [`PassDispatch`].
    Prepared,
}

/// Submits one frame of `units` dispatches staged as `staging` on a fresh device; the errors the
/// device reported when it refused the frame.
fn submit(staging: Staging, units: usize) -> Result<(), Vec<String>> {
    let adapter = TestGpu::shared_adapter().unwrap();
    let (device, queue, _) =
        pollster::block_on(request_device(&adapter, &DeviceRequirements::default())).unwrap();
    let errors = Arc::new(Mutex::new(Vec::new()));
    let sink = errors.clone();
    device.on_uncaptured_error(Arc::new(move |error: wgpu::Error| {
        sink.lock().unwrap().push(format!("uncaptured: {error}"));
    }));
    let sink = errors.clone();
    device.set_device_lost_callback(move |reason, message| {
        sink.lock()
            .unwrap()
            .push(format!("lost: {reason:?} {message}"));
    });
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
    let prepared = SubmissionAuthority::new(queue).prepare(&device, |encoder, _| {
        let mut context = PrepareContext::new(&device, encoder)?;
        context.open_preparation();
        for _ in 0..units {
            match staging {
                Staging::Context => context.dispatch("count", &pipeline, &group, [1, 1, 1]),
                Staging::Prepared => context.record_prepared(
                    "count",
                    PreparedAccess::default(),
                    Box::new(PassDispatch {
                        pipeline: pipeline.clone(),
                        group: group.clone(),
                    }),
                )?,
            }
        }
        context.finish_preparation()?;
        Ok::<_, crate::FrameError>(())
    });
    match prepared {
        Ok(frame) => {
            if let Err(error) = frame.submit() {
                errors.lock().unwrap().push(format!("submit: {error}"));
            }
        }
        Err(error) => errors.lock().unwrap().push(format!("prepare: {error:?}")),
    }
    let _ = device.poll(wgpu::PollType::wait_indefinitely());
    let errors = errors.lock().unwrap().clone();
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

#[test]
fn a_prepared_dispatch_costs_no_more_command_buffers_than_a_context_dispatch() {
    let (largest, refusal) = largest_submitting_count(CAP, |units| submit(Staging::Context, units));
    if let Some(refusal) = refusal {
        assert!(
            refusal.iter().any(|error| error.starts_with("lost:")),
            "the frame after {largest} dispatches was refused for another reason than the command \
             buffer cap: {refusal:?}"
        );
    }
    assert_eq!(
        submit(Staging::Prepared, largest),
        Ok(()),
        "one frame submits {largest} context dispatches but not as many prepared ones"
    );
}
