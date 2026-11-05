//! Programs: stages with the wgpu layouts built from their merged bindings.

use std::sync::Arc;

use crate::error::ProgramError;
use crate::layout::ProgramLayout;
use crate::module::{StageKind, StageProgram};
use crate::runtime::ProgramRuntime;

/// A program's bind group layouts and pipeline layout.
pub(crate) struct GpuLayout {
    pub(crate) layout: ProgramLayout,
    /// One per descriptor set up to the highest used; `None` for a set with no bindings.
    pub(crate) groups: Vec<Option<wgpu::BindGroupLayout>>,
    pub(crate) pipeline: wgpu::PipelineLayout,
}

/// A compute stage with its layouts.
#[derive(Clone)]
pub struct ComputeProgram {
    pub(crate) stage: StageProgram,
    pub(crate) gpu: Arc<GpuLayout>,
}

/// A vertex and a fragment stage with their merged layouts.
#[derive(Clone)]
pub struct GraphicsProgram {
    pub(crate) vertex: StageProgram,
    pub(crate) fragment: StageProgram,
    pub(crate) gpu: Arc<GpuLayout>,
}

impl ComputeProgram {
    #[must_use]
    pub fn layout(&self) -> &ProgramLayout {
        &self.gpu.layout
    }
}

impl GraphicsProgram {
    #[must_use]
    pub fn layout(&self) -> &ProgramLayout {
        &self.gpu.layout
    }
}

fn expect(stage: &StageProgram, expected: StageKind) -> Result<(), ProgramError> {
    let found = stage.module().kind;
    if found == expected {
        return Ok(());
    }
    Err(ProgramError::WrongStage { expected, found })
}

impl ProgramRuntime {
    /// Builds the layouts of a compute stage.
    ///
    /// # Errors
    /// Fails when the stage is not a compute stage, its parameters do not match the module,
    /// or the device cannot hold the layout.
    pub fn compute_program(&self, stage: &StageProgram) -> Result<ComputeProgram, ProgramError> {
        expect(stage, StageKind::Compute)?;
        let gpu = self.gpu_layout(ProgramLayout::build(&[stage])?)?;
        Ok(ComputeProgram {
            stage: stage.clone(),
            gpu: Arc::new(gpu),
        })
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
        expect(vertex, StageKind::Vertex)?;
        expect(fragment, StageKind::Fragment)?;
        let gpu = self.gpu_layout(ProgramLayout::build(&[vertex, fragment])?)?;
        Ok(GraphicsProgram {
            vertex: vertex.clone(),
            fragment: fragment.clone(),
            gpu: Arc::new(gpu),
        })
    }

    fn gpu_layout(&self, layout: ProgramLayout) -> Result<GpuLayout, ProgramError> {
        let limit = self.device.limits().max_bind_groups;
        let sets = layout
            .slots()
            .iter()
            .map(|slot| slot.set + 1)
            .max()
            .unwrap_or(0);
        if sets > limit {
            return Err(ProgramError::TooManyGroups {
                set: sets - 1,
                limit,
            });
        }
        let mut groups = Vec::new();
        for set in 0..sets {
            groups.push(self.group_layout(&layout, set)?);
        }
        let borrowed: Vec<Option<&wgpu::BindGroupLayout>> =
            groups.iter().map(Option::as_ref).collect();
        let pipeline = self.guarded(|| {
            self.device
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("render-program pipeline layout"),
                    bind_group_layouts: &borrowed,
                    immediate_size: 0,
                })
        })?;
        Ok(GpuLayout {
            layout,
            groups,
            pipeline,
        })
    }

    fn group_layout(
        &self,
        layout: &ProgramLayout,
        set: u32,
    ) -> Result<Option<wgpu::BindGroupLayout>, ProgramError> {
        let slots: Vec<_> = layout.slots().iter().filter(|s| s.set == set).collect();
        if slots.is_empty() {
            return Ok(None);
        }
        let mut entries = Vec::new();
        for slot in slots {
            if matches!(
                slot.ty,
                wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison)
            ) {
                return Err(ProgramError::UnsupportedSampler {
                    set,
                    binding: slot.binding,
                });
            }
            entries.push(wgpu::BindGroupLayoutEntry {
                binding: slot.binding,
                visibility: slot.visibility,
                ty: slot.ty,
                count: None,
            });
        }
        let group = self.guarded(|| {
            self.device
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("render-program bind group layout"),
                    entries: &entries,
                })
        })?;
        Ok(Some(group))
    }
}
