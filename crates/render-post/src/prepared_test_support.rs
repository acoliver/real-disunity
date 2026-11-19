//! Prepared commands that record what a program runtime's do, for frame tests that cannot depend
//! on the runtime: a dispatch with a compute form, as `PreparedDispatch`, and copies inside an
//! encoder debug group, as `PreparedBlockTransfer`.

use std::sync::{Arc, Mutex};

use worldrunner_render_gpu::device::{DeviceRequirements, request_device};
use worldrunner_render_gpu::test_support::TestGpu;

use crate::{PreparedCommands, PreparedCompute, PreparedRefusal, PreparedWrite};

/// The refusal of a [`PassDispatch`] made to refuse.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
#[error("the dispatch refused to record")]
pub(crate) struct Refused;

/// One dispatch of `pipeline` over `group` at set 0. Recorded on its own, it opens a compute pass
/// labelled with its label; recorded in a shared pass, it names itself with a debug group.
pub(crate) struct PassDispatch {
    pub(crate) pipeline: wgpu::ComputePipeline,
    pub(crate) group: wgpu::BindGroup,
    pub(crate) refuse: bool,
}

impl PassDispatch {
    pub(crate) fn new(pipeline: &wgpu::ComputePipeline, group: &wgpu::BindGroup) -> Self {
        Self {
            pipeline: pipeline.clone(),
            group: group.clone(),
            refuse: false,
        }
    }

    fn encode(&self, pass: &mut wgpu::ComputePass<'_>) {
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.group, &[]);
        pass.dispatch_workgroups(1, 1, 1);
    }
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
        if self.refuse {
            return Err(PreparedRefusal::new(Refused));
        }
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some(label),
            timestamp_writes: None,
        });
        self.encode(&mut pass);
        Ok(())
    }

    fn compute(&self) -> Option<&dyn PreparedCompute> {
        Some(self)
    }
}

impl PreparedCompute for PassDispatch {
    fn record_in_pass(
        &self,
        pass: &mut wgpu::ComputePass<'_>,
        label: &str,
    ) -> Result<(), PreparedRefusal> {
        if self.refuse {
            return Err(PreparedRefusal::new(Refused));
        }
        pass.push_debug_group(label);
        self.encode(pass);
        pass.pop_debug_group();
        Ok(())
    }
}

/// A copy of the first `bytes` bytes of `source` into `destination` inside an encoder debug group
/// of its label, as a block transfer records its copies. It has no compute form.
pub(crate) struct BufferCopy {
    pub(crate) source: wgpu::Buffer,
    pub(crate) destination: wgpu::Buffer,
    pub(crate) bytes: u64,
}

impl PreparedCommands for BufferCopy {
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
        encoder.push_debug_group(label);
        encoder.copy_buffer_to_buffer(&self.source, 0, &self.destination, 0, self.bytes);
        encoder.pop_debug_group();
        Ok(())
    }
}

/// A device of its own, with the errors it reports, uncaptured or as its loss, collected.
pub(crate) struct Recorded {
    pub(crate) device: wgpu::Device,
    pub(crate) queue: wgpu::Queue,
    errors: Arc<Mutex<Vec<String>>>,
}

impl Recorded {
    pub(crate) fn new() -> Self {
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
        Self {
            device,
            queue,
            errors,
        }
    }

    pub(crate) fn report(&self, error: String) {
        self.errors.lock().unwrap().push(error);
    }

    /// Waits for the device, then the errors it reported, if any.
    pub(crate) fn outcome(&self) -> Result<(), Vec<String>> {
        let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
        let errors = self.errors.lock().unwrap().clone();
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

/// A compute pipeline of `source`'s `main` with its own layout.
pub(crate) fn pipeline(device: &wgpu::Device, label: &str, source: &str) -> wgpu::ComputePipeline {
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(label),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some(label),
        layout: None,
        module: &module,
        entry_point: Some("main"),
        compilation_options: wgpu::PipelineCompilationOptions::default(),
        cache: None,
    })
}
