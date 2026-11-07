//! Programs: stages with the wgpu layouts built from their merged bindings.

use std::sync::Arc;

use crate::binding::ResourceLayout;
use crate::error::ProgramError;
use crate::layout::ProgramLayout;
use crate::module::{StageKind, StageProgram};
use crate::runtime::ProgramRuntime;

/// A program's bind group layouts and pipeline layout.
pub(crate) struct GpuLayout {
    pub(crate) layout: ProgramLayout,
    /// One per descriptor set up to the highest used; `None` for a set with no bindings. A lent
    /// set holds the layout it was lent as.
    pub(crate) groups: Vec<Option<wgpu::BindGroupLayout>>,
    pub(crate) pipeline: wgpu::PipelineLayout,
    /// The lent sets, ordered by set.
    pub(crate) external: Vec<ExternalSet>,
    /// The numbers of the lent sets, ascending.
    pub(crate) external_sets: Vec<u32>,
}

impl GpuLayout {
    pub(crate) fn is_external(&self, set: u32) -> bool {
        self.external_sets.contains(&set)
    }
}

/// A compute stage with its layouts.
#[derive(Clone)]
pub struct ComputeProgram {
    pub(crate) stage: StageProgram,
    pub(crate) gpu: Arc<GpuLayout>,
}

/// A bind group set that the owner of a pass binds once and every draw of the pass shares.
///
/// The program uses `layout` as it is in its pipeline layout, so a group the owner builds
/// against that layout fits every pipeline of a program that lends it. The runtime builds no
/// group for the set and a recorded draw does not bind it. `bindings` is what the shader reads
/// from the set, for checking against the shader: every resource a stage uses at `set` must be
/// listed at its address as the same class (see
/// [`ResourceKind::same_class`](crate::binding::ResourceKind::same_class)), and a listed binding
/// the shader does not use is allowed. A binding's `source` names the engine semantic the owner
/// fills it from, so a draw that imports that semantic is refused as
/// [`ExternalSetConflict`](crate::error::ProgramError::ExternalSetConflict).
#[derive(Clone, Debug)]
pub struct ExternalSet {
    pub set: u32,
    pub layout: wgpu::BindGroupLayout,
    pub bindings: ResourceLayout,
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

    /// The sets the pass owner binds, ascending. A draw of this program leaves them alone.
    #[must_use]
    pub fn external_sets(&self) -> &[u32] {
        &self.gpu.external_sets
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
    /// Fails when the stage is not a compute stage, its declarations do not match the module,
    /// or the device cannot hold the layout.
    pub fn compute_program(&self, stage: &StageProgram) -> Result<ComputeProgram, ProgramError> {
        expect(stage, StageKind::Compute)?;
        let gpu = self.gpu_layout(ProgramLayout::build(&[stage])?, &[])?;
        Ok(ComputeProgram {
            stage: stage.clone(),
            gpu: Arc::new(gpu),
        })
    }

    /// Builds the merged layouts of a vertex and a fragment stage.
    ///
    /// # Errors
    /// Fails when a stage is of the wrong kind, the stages' declarations do not match their
    /// modules or each other, or the device cannot hold the layout.
    pub fn graphics_program(
        &self,
        vertex: &StageProgram,
        fragment: &StageProgram,
    ) -> Result<GraphicsProgram, ProgramError> {
        self.graphics_program_in(vertex, fragment, &[])
    }

    /// Like [`graphics_program`](Self::graphics_program), with the sets of `external` lent: the
    /// owner of the pass binds them, and each layout is used as given in the pipeline layout.
    ///
    /// The stages need not declare what they use at a lent set, because the lent declaration
    /// does that. Declarations a stage makes at a lent set are ignored. Programs that lend one
    /// layout share the group the owner binds across their pipelines, and programs that lend
    /// different layouts never share a pipeline.
    ///
    /// # Errors
    /// Also fails with `ExternalSetConflict` when a set is lent twice, with a layout error that
    /// names a resource a lent declaration does not list or lists as another class, and with
    /// `TooManyGroups` when a lent set is above the device's limit. A lent layout that the
    /// shader does not fit in ways this check does not see, such as visibility, is the GPU's
    /// to refuse when a pipeline is created.
    pub fn graphics_program_in(
        &self,
        vertex: &StageProgram,
        fragment: &StageProgram,
        external: &[ExternalSet],
    ) -> Result<GraphicsProgram, ProgramError> {
        expect(vertex, StageKind::Vertex)?;
        expect(fragment, StageKind::Fragment)?;
        if let Some(repeated) = external
            .iter()
            .enumerate()
            .find(|(index, lent)| external[..*index].iter().any(|other| other.set == lent.set))
        {
            return Err(ProgramError::ExternalSetConflict {
                set: repeated.1.set,
            });
        }
        let layout = ProgramLayout::build_in(&[vertex, fragment], external)?;
        let gpu = self.gpu_layout(layout, external)?;
        Ok(GraphicsProgram {
            vertex: vertex.clone(),
            fragment: fragment.clone(),
            gpu: Arc::new(gpu),
        })
    }

    fn gpu_layout(
        &self,
        layout: ProgramLayout,
        external: &[ExternalSet],
    ) -> Result<GpuLayout, ProgramError> {
        let limit = self.device.limits().max_bind_groups;
        let sets = layout
            .slots()
            .iter()
            .map(|slot| slot.set)
            .chain(external.iter().map(|lent| lent.set))
            .map(|set| set + 1)
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
            groups.push(match external.iter().find(|lent| lent.set == set) {
                Some(lent) => Some(lent.layout.clone()),
                None => self.group_layout(&layout, set)?,
            });
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
        let mut external = external.to_vec();
        external.sort_by_key(|lent| lent.set);
        Ok(GpuLayout {
            layout,
            groups,
            pipeline,
            external_sets: external.iter().map(|lent| lent.set).collect(),
            external,
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
