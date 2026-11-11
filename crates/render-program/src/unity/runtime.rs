//! The runtime as a Unity caller uses it: programs built from [`StageProgram`]s and draws
//! described with Unity's [`RenderState`] and a front-face winding.
//!
//! [`ProgramRuntime`] wraps the neutral runtime and dereferences to it, so everything that
//! takes no Unity record (resources, frames, readbacks, arenas) is the neutral runtime's own.
//! Only the methods that take a stage, a program or a state are redefined here, converting to
//! the neutral values and calling the neutral method.

use std::ops::{Deref, DerefMut};

use worldrunner_render_gpu::configuration::{AllocationBudget, AllocationSet};
use worldrunner_render_gpu::submission::SubmissionAuthority;

use super::layout::LayoutError;
use super::module::StageProgram;
use super::state::RenderState;
use crate::error::ProgramError;
pub use crate::runtime::{
    AdmittedPrograms, BoundResource, BufferHandle, Geometry, PendingBufferRead, PendingTargetRead,
    PreparedProgramDraw, ProgramArena, Resource, TargetHandle, TargetImage, TargetSignature,
    TextureHandle, VertexInput,
};

/// A Unity layout error is a declaration the neutral runtime could not take.
impl From<LayoutError> for ProgramError {
    fn from(error: LayoutError) -> Self {
        Self::Layout(crate::layout::LayoutError::Declaration {
            reason: error.to_string(),
        })
    }
}

/// A compute stage with its layouts.
#[derive(Clone)]
pub struct ComputeProgram(crate::program::ComputeProgram);

/// A vertex and a fragment stage with their merged layouts.
#[derive(Clone)]
pub struct GraphicsProgram(crate::program::GraphicsProgram);

impl ComputeProgram {
    #[must_use]
    pub fn layout(&self) -> &crate::layout::ProgramLayout {
        self.0.layout()
    }
}

impl GraphicsProgram {
    #[must_use]
    pub fn layout(&self) -> &crate::layout::ProgramLayout {
        self.0.layout()
    }
}

/// One dispatch of a compute kernel.
#[derive(Clone, Copy)]
pub struct DispatchRequest<'a> {
    pub program: &'a ComputeProgram,
    pub bindings: &'a [BoundResource],
    /// Workgroup counts in x, y and z.
    pub groups: [u32; 3],
}

/// One draw of triangles into a target.
#[derive(Clone, Copy)]
pub struct DrawRequest<'a> {
    pub program: &'a GraphicsProgram,
    pub state: &'a RenderState,
    /// The winding that counts as a front face, in framebuffer space.
    pub front_face: wgpu::FrontFace,
    pub target: TargetHandle,
    pub vertex_inputs: &'a [VertexInput],
    pub bindings: &'a [BoundResource],
    pub geometry: Geometry,
}

/// One draw of triangles, without the target it draws into.
#[derive(Clone, Copy)]
pub struct DrawPacket<'a> {
    pub program: &'a GraphicsProgram,
    pub state: &'a RenderState,
    /// The winding that counts as a front face, in framebuffer space.
    pub front_face: wgpu::FrontFace,
    pub vertex_inputs: &'a [VertexInput],
    pub bindings: &'a [BoundResource],
    pub geometry: Geometry,
}

impl DrawPacket<'_> {
    /// The neutral packet over `state`, which must be this packet's state converted.
    fn neutral<'a>(
        &'a self,
        state: &'a crate::state::RenderState,
    ) -> crate::runtime::DrawPacket<'a> {
        crate::runtime::DrawPacket {
            program: &self.program.0,
            state,
            vertex_inputs: self.vertex_inputs,
            bindings: self.bindings,
            geometry: self.geometry,
        }
    }
}

/// The neutral runtime with Unity-typed program, draw and dispatch entry points.
pub struct ProgramRuntime(crate::runtime::ProgramRuntime);

impl Deref for ProgramRuntime {
    type Target = crate::runtime::ProgramRuntime;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for ProgramRuntime {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl ProgramRuntime {
    /// A runtime over the caller's device and submission authority; see
    /// [`crate::runtime::ProgramRuntime::new`].
    #[must_use]
    pub fn new(
        device: wgpu::Device,
        authority: SubmissionAuthority,
        budget: AllocationBudget,
    ) -> Self {
        Self(crate::runtime::ProgramRuntime::new(
            device, authority, budget,
        ))
    }

    /// The storage the renderer reserves for an arena of `bytes`.
    #[must_use]
    pub fn arena_allocations(bytes: u64) -> AllocationSet {
        crate::runtime::ProgramRuntime::arena_allocations(bytes)
    }

    /// The storage the renderer reserves for an arena of `bytes` on a device with `limits`, in
    /// buffers the device can hold.
    #[must_use]
    pub fn arena_allocations_within(bytes: u64, limits: &wgpu::Limits) -> AllocationSet {
        crate::runtime::ProgramRuntime::arena_allocations_within(bytes, limits)
    }

    /// Builds the layouts of a compute stage.
    ///
    /// # Errors
    /// Fails when the stage is not a compute stage, its parameters do not match the module,
    /// or the device cannot hold the layout.
    pub fn compute_program(&self, stage: &StageProgram) -> Result<ComputeProgram, ProgramError> {
        Ok(ComputeProgram(self.0.compute_program(&stage.neutral()?)?))
    }

    /// Builds the merged layouts of a vertex and a fragment stage.
    ///
    /// # Errors
    /// Fails when a stage is of the wrong kind, the stages' parameters do not match their
    /// modules or each other, or the device cannot hold the layout.
    pub fn graphics_program(
        &self,
        vertex: &StageProgram,
        fragment: &StageProgram,
    ) -> Result<GraphicsProgram, ProgramError> {
        Ok(GraphicsProgram(self.0.graphics_program(
            &vertex.neutral()?,
            &fragment.neutral()?,
        )?))
    }

    /// Records the kernel's dispatch into `encoder`; see
    /// [`crate::runtime::ProgramRuntime::dispatch`].
    ///
    /// # Errors
    /// Fails like the neutral dispatch.
    pub fn dispatch(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        request: &DispatchRequest<'_>,
    ) -> Result<(), ProgramError> {
        self.0.dispatch(
            encoder,
            &crate::runtime::DispatchRequest {
                program: &request.program.0,
                bindings: request.bindings,
                groups: request.groups,
            },
        )
    }

    /// Checks the storage images of a dispatch without recording it; see
    /// [`crate::runtime::ProgramRuntime::check_storage_images`].
    ///
    /// # Errors
    /// Fails like the neutral check.
    pub fn check_storage_images(
        &self,
        program: &ComputeProgram,
        bound: &[BoundResource],
    ) -> Result<(), ProgramError> {
        self.0.check_storage_images(&program.0, bound)
    }

    /// Records the geometry's draw into `encoder`; see [`crate::runtime::ProgramRuntime::draw`].
    ///
    /// # Errors
    /// Fails like the neutral draw.
    pub fn draw(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        request: &DrawRequest<'_>,
    ) -> Result<(), ProgramError> {
        let state = request.state.neutral(request.front_face);
        self.0.draw(
            encoder,
            &crate::runtime::DrawRequest {
                program: &request.program.0,
                state: &state,
                target: request.target,
                vertex_inputs: request.vertex_inputs,
                bindings: request.bindings,
                geometry: request.geometry,
            },
        )
    }

    /// Prepares `packet` for passes with the attachments of `signature`; see
    /// [`crate::runtime::ProgramRuntime::prepare_draw`].
    ///
    /// # Errors
    /// Fails like the neutral preparation.
    pub fn prepare_draw(
        &mut self,
        packet: &DrawPacket<'_>,
        signature: TargetSignature,
    ) -> Result<PreparedProgramDraw, ProgramError> {
        let state = packet.state.neutral(packet.front_face);
        self.0.prepare_draw(&packet.neutral(&state), signature)
    }

    /// Prepares `packets` and admits them into the frame `encoder` records; see
    /// [`crate::runtime::ProgramRuntime::admit_draws`].
    ///
    /// # Errors
    /// Fails like the neutral admission, and nothing is admitted.
    pub fn admit_draws(
        &mut self,
        encoder: &wgpu::CommandEncoder,
        arena: &ProgramArena,
        signature: TargetSignature,
        packets: &[DrawPacket<'_>],
    ) -> Result<AdmittedPrograms, ProgramError> {
        let states: Vec<_> = packets
            .iter()
            .map(|packet| packet.state.neutral(packet.front_face))
            .collect();
        let neutral: Vec<_> = packets
            .iter()
            .zip(&states)
            .map(|(packet, state)| packet.neutral(state))
            .collect();
        self.0.admit_draws(encoder, arena, signature, &neutral)
    }
}
