//! Compute dispatches.

use crate::error::ProgramError;
use crate::pipeline_key::Key;
use crate::program::ComputeProgram;
use crate::resources::BoundResource;
use crate::runtime::ProgramRuntime;

/// One dispatch of a compute kernel.
#[derive(Clone, Copy)]
pub struct DispatchRequest<'a> {
    pub program: &'a ComputeProgram,
    pub bindings: &'a [BoundResource],
    /// Workgroup counts in x, y and z.
    pub groups: [u32; 3],
}

impl ProgramRuntime {
    /// Records the kernel's dispatch into `encoder`; the caller submits the frame and reads
    /// results back with `read_buffer`. The frame retains the reservations of the buffers and
    /// textures the kernel binds, so their bytes stay counted until the frame is cancelled,
    /// dropped or completed on the GPU, even when the runtime is dropped first.
    ///
    /// # Errors
    /// Fails on a binding that does not fit the program, an encoder that is not the
    /// authority's, a lost device or a GPU validation error.
    pub fn dispatch(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        request: &DispatchRequest<'_>,
    ) -> Result<(), ProgramError> {
        self.recording(encoder)?;
        let pipeline = self.compute_pipeline(request.program)?;
        let groups = self.bind_groups(&request.program.gpu, request.bindings)?;
        self.hold_for_frame(encoder, self.binding_shares(request.bindings)?)?;
        self.guarded(|| {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor::default());
            pass.set_pipeline(&pipeline);
            for (set, group) in &groups {
                pass.set_bind_group(*set, group, &[]);
            }
            pass.dispatch_workgroups(request.groups[0], request.groups[1], request.groups[2]);
        })
    }

    fn compute_pipeline(
        &mut self,
        program: &ComputeProgram,
    ) -> Result<wgpu::ComputePipeline, ProgramError> {
        let mut key = Key::new("compute");
        key.hash(program.stage.content_key());
        key.debug(&program.gpu.layout);
        let key = key.finish();
        if let Some(pipeline) = self.compute_pipelines.get(&key) {
            return Ok(pipeline.clone());
        }
        let module = self.shader_module(&program.stage)?;
        let constants = program.stage.constants();
        let pipeline = self.guarded(|| {
            self.device
                .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some(&program.stage.module().entry_point),
                    layout: Some(&program.gpu.pipeline),
                    module: &module,
                    entry_point: Some(&program.stage.module().entry_point),
                    compilation_options: wgpu::PipelineCompilationOptions {
                        constants: &constants,
                        ..wgpu::PipelineCompilationOptions::default()
                    },
                    cache: None,
                })
        })?;
        self.counters.pipeline_built();
        self.compute_pipelines.insert(key, pipeline.clone());
        Ok(pipeline)
    }
}
