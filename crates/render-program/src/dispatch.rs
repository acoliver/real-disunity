//! Compute dispatches.

use worldrunner_render_post::PreparedWrite;

use crate::error::ProgramError;
use crate::import::Import;
use crate::program::ComputeProgram;
use crate::resources::{BoundResource, Keepalive, Pins};
use crate::runtime::{ProgramRuntime, guarded};

/// One dispatch of a compute kernel.
#[derive(Clone, Copy)]
pub struct DispatchRequest<'a> {
    pub program: &'a ComputeProgram,
    pub bindings: &'a [BoundResource],
    /// Workgroup counts in x, y and z.
    pub groups: [u32; 3],
}

/// One dispatch of a compute kernel over bindings the runtime holds and resources another owner
/// lends to it.
///
/// `imports` fill the bindings the program sources from an engine semantic, `bindings` fill the
/// ones that name runtime handles, and `owners` are held by the dispatch and by every frame that
/// retains it. Nothing lent is counted by the runtime.
#[derive(Clone, Copy)]
pub struct ImportedDispatch<'a> {
    pub program: &'a ComputeProgram,
    pub bindings: &'a [BoundResource],
    pub imports: &'a [Import],
    /// Workgroup counts in x, y and z.
    pub groups: [u32; 3],
    pub owners: &'a [Keepalive],
}

/// A dispatch whose pipeline and bind groups are built and whose bindings and workgroup counts
/// were checked. Recording it touches no runtime state, and is guarded like
/// [`ProgramRuntime::dispatch`]'s: a GPU validation error it causes is `ProgramError::Gpu`.
///
/// It holds shares of the runtime's reservations of the resources it binds, and the owners of the
/// resources it imports, for as long as it is held and then for every frame that
/// [`retain`](Self::retain)ed it, until that frame is cancelled, dropped or completed on the GPU.
pub struct PreparedDispatch {
    pins: Pins,
    device: wgpu::Device,
    pipeline: wgpu::ComputePipeline,
    groups: Vec<(u32, wgpu::BindGroup)>,
    workgroups: [u32; 3],
    writes: Vec<PreparedWrite>,
}

impl std::fmt::Debug for PreparedDispatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreparedDispatch")
            .field("workgroups", &self.workgroups)
            .finish_non_exhaustive()
    }
}

impl PreparedDispatch {
    /// Makes the frame `encoder` records hold the dispatch's reservations and owners until the
    /// frame is cancelled, dropped or completed on the GPU. Call it for every frame the dispatch
    /// is recorded into.
    ///
    /// # Errors
    /// Fails with `ForeignAuthority` when the frame belongs to another submission authority than
    /// the runtime's and when `encoder` is not a frame being recorded.
    pub fn retain(&self, encoder: &wgpu::CommandEncoder) -> Result<(), ProgramError> {
        self.pins.retain(encoder)
    }

    /// Records the dispatch in a compute pass of its own.
    ///
    /// # Errors
    /// Fails with `Gpu` on a GPU validation error recording it causes, such as an encoder another
    /// pass holds.
    pub fn record(&self, encoder: &mut wgpu::CommandEncoder) -> Result<(), ProgramError> {
        self.record_labelled(encoder, None)
    }

    /// Records the dispatch in a compute pass of its own labelled `label`.
    pub(crate) fn record_labelled(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        label: Option<&str>,
    ) -> Result<(), ProgramError> {
        guarded(&self.device, || {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label,
                timestamp_writes: None,
            });
            self.encode(&mut pass);
        })
    }

    /// Records the dispatch into `pass`, which other work shares, inside a debug group of `label`.
    /// Every bind group of the program is set, so none an earlier dispatch of the pass left bound
    /// is used.
    pub(crate) fn record_in_pass(
        &self,
        pass: &mut wgpu::ComputePass<'_>,
        label: &str,
    ) -> Result<(), ProgramError> {
        guarded(&self.device, || {
            pass.push_debug_group(label);
            self.encode(pass);
            pass.pop_debug_group();
        })
    }

    fn encode(&self, pass: &mut wgpu::ComputePass<'_>) {
        pass.set_pipeline(&self.pipeline);
        for (set, group) in &self.groups {
            pass.set_bind_group(*set, group, &[]);
        }
        let [x, y, z] = self.workgroups;
        pass.dispatch_workgroups(x, y, z);
    }

    /// The textures the dispatch writes through its storage images: the base mip of every layer
    /// of a runtime texture, and the whole of a lent one.
    pub fn writes(&self) -> &[PreparedWrite] {
        &self.writes
    }
}

impl ProgramRuntime {
    /// Records the kernel's dispatch into `encoder`; the caller submits the frame and reads
    /// results back with `read_buffer`. The frame retains the reservations of the buffers and
    /// textures the kernel binds, so their bytes stay counted until the frame is cancelled,
    /// dropped or completed on the GPU, even when the runtime is dropped first.
    ///
    /// # Errors
    /// Fails on a binding that does not fit the program, a workgroup count above the device's
    /// limit, an encoder that is not the authority's, a lost device or a GPU validation error.
    pub fn dispatch(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        request: &DispatchRequest<'_>,
    ) -> Result<(), ProgramError> {
        self.recording(encoder)?;
        let prepared = self.prepare_dispatch(request)?;
        prepared.retain(encoder)?;
        prepared.record(encoder)
    }

    /// Prepares the kernel's dispatch for a frame that records it later, as
    /// [`dispatch`](Self::dispatch) would record it now. Everything that can fail is checked here.
    ///
    /// # Errors
    /// As [`dispatch`](Self::dispatch), less the encoder's.
    pub fn prepare_dispatch(
        &mut self,
        request: &DispatchRequest<'_>,
    ) -> Result<PreparedDispatch, ProgramError> {
        self.prepare_imported_dispatch(&ImportedDispatch {
            program: request.program,
            bindings: request.bindings,
            imports: &[],
            groups: request.groups,
            owners: &[],
        })
    }

    /// Prepares a dispatch over runtime bindings and lent imports.
    ///
    /// # Errors
    /// As [`prepare_dispatch`](Self::prepare_dispatch), and on a binding with no import, an
    /// import no binding takes or one given twice, and an import that is not the kind of resource
    /// its binding is.
    pub fn prepare_imported_dispatch(
        &mut self,
        request: &ImportedDispatch<'_>,
    ) -> Result<PreparedDispatch, ProgramError> {
        self.authority.check_device()?;
        let limit = self.device.limits().max_compute_workgroups_per_dimension;
        if request.groups.iter().any(|count| *count > limit) {
            return Err(ProgramError::Workgroups {
                groups: request.groups,
                limit,
            });
        }
        let pipeline = self.compute_pipeline(request.program)?;
        let groups = self.bind_groups_sharing(
            &request.program.gpu,
            (request.bindings, &[], request.imports),
            &[],
        )?;
        let shares = self.binding_shares(request.bindings)?;
        let writes =
            self.storage_writes(&request.program.gpu, request.bindings, request.imports)?;
        Ok(PreparedDispatch {
            pins: Pins::borrowing(&self.authority, shares, request.owners.to_vec()),
            device: self.device.clone(),
            pipeline,
            groups,
            workgroups: request.groups,
            writes,
        })
    }

    fn compute_pipeline(
        &mut self,
        program: &ComputeProgram,
    ) -> Result<wgpu::ComputePipeline, ProgramError> {
        let key = program.identity;
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
