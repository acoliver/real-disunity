//! Programs: stages with the wgpu layouts built from their merged bindings.

use std::sync::Arc;

use crate::binding::{BindingSource, ResourceLayout};
use crate::error::{ProgramError, StorageImageError};
use crate::layout::ProgramLayout;
use crate::module::{StageKind, StageProgram};
use crate::pipeline_key::Key;
use crate::runtime::ProgramRuntime;

/// A program's bind group layouts and pipeline layout.
pub(crate) struct GpuLayout {
    pub(crate) layout: ProgramLayout,
    /// One per descriptor set up to the highest used; `None` for a set with no bindings. A lent
    /// set holds the layout it was lent as.
    pub(crate) groups: Vec<Option<wgpu::BindGroupLayout>>,
    /// The identity of each runtime-built set's layout, parallel to `groups`: `None` for a set
    /// with no bindings and for a lent set. Equal keys are one shared layout object.
    pub(crate) set_keys: Vec<Option<blake3::Hash>>,
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

/// A vertex stage and, usually, a fragment stage, with their merged layouts. A program with no
/// fragment stage (see [`ProgramRuntime::vertex_only_program`]) draws into passes that have no
/// colour attachment, such as the depth passes of shadow casters.
#[derive(Clone)]
pub struct GraphicsProgram {
    pub(crate) vertex: StageProgram,
    pub(crate) fragment: Option<StageProgram>,
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
    pub fn vertex(&self) -> &StageProgram {
        &self.vertex
    }

    /// The fragment stage, or `None` for a vertex-only program.
    #[must_use]
    pub fn fragment(&self) -> Option<&StageProgram> {
        self.fragment.as_ref()
    }

    #[must_use]
    pub fn layout(&self) -> &ProgramLayout {
        &self.gpu.layout
    }

    /// The sets the pass owner binds, ascending. A draw of this program leaves them alone.
    #[must_use]
    pub fn external_sets(&self) -> &[u32] {
        &self.gpu.external_sets
    }

    /// The layout of `set`: the lent layout for a lent set, and for a set the runtime builds the
    /// one layout every program of the runtime with equal slots at that set shares, so a group
    /// built against it fits all of them. `None` for a set with no bindings.
    #[must_use]
    pub fn set_layout(&self, set: u32) -> Option<&wgpu::BindGroupLayout> {
        self.gpu.groups.get(usize::try_from(set).ok()?)?.as_ref()
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

    /// Builds the layouts of a vertex stage alone, for passes with a depth attachment and no
    /// colour attachment: it can be drawn only into a signature made by
    /// [`TargetSignature::depth_only`](crate::packet::TargetSignature::depth_only), and a
    /// pipeline for any other is refused as `NoFragmentForColors`. The stage's declarations and
    /// overrides are used as they are.
    ///
    /// # Errors
    /// Fails when the stage is not a vertex stage, its declarations do not match its module or
    /// the device cannot hold the layout.
    pub fn vertex_only_program(
        &self,
        vertex: &StageProgram,
    ) -> Result<GraphicsProgram, ProgramError> {
        expect(vertex, StageKind::Vertex)?;
        let layout = ProgramLayout::build_in(&[vertex], &[])?;
        let gpu = self.gpu_layout(layout, &[])?;
        Ok(GraphicsProgram {
            vertex: vertex.clone(),
            fragment: None,
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
            fragment: Some(fragment.clone()),
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
        self.check_storage_image_count(&layout)?;
        let mut groups = Vec::new();
        let mut set_keys = Vec::new();
        for set in 0..sets {
            let (group, key) = match external.iter().find(|lent| lent.set == set) {
                Some(lent) => (Some(lent.layout.clone()), None),
                None => self.group_layout(&layout, set)?.unzip(),
            };
            groups.push(group);
            set_keys.push(key);
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
            set_keys,
            pipeline,
            external_sets: external.iter().map(|lent| lent.set).collect(),
            external,
        })
    }

    /// The device allows only so many storage images in a stage; a program that binds more is
    /// refused here by name, where wgpu would reject its layout with a message.
    fn check_storage_image_count(&self, layout: &ProgramLayout) -> Result<(), ProgramError> {
        let limit = self.device.limits().max_storage_textures_per_shader_stage;
        for (stage, visibility) in [
            (StageKind::Vertex, wgpu::ShaderStages::VERTEX),
            (StageKind::Fragment, wgpu::ShaderStages::FRAGMENT),
            (StageKind::Compute, wgpu::ShaderStages::COMPUTE),
        ] {
            let count = layout
                .slots()
                .iter()
                .filter(|slot| {
                    matches!(slot.ty, wgpu::BindingType::StorageTexture { .. })
                        && slot.visibility.contains(visibility)
                })
                .count();
            let count = u32::try_from(count).unwrap_or(u32::MAX);
            if count > limit {
                return Err(ProgramError::StorageImageLimit {
                    stage,
                    count,
                    limit,
                });
            }
        }
        Ok(())
    }

    /// The layout of `set`, and the key that names it. Programs whose slots at a set are equal
    /// get one layout object, so a group built for one fits the others.
    fn group_layout(
        &self,
        layout: &ProgramLayout,
        set: u32,
    ) -> Result<Option<(wgpu::BindGroupLayout, blake3::Hash)>, ProgramError> {
        let slots: Vec<_> = layout.slots().iter().filter(|s| s.set == set).collect();
        if slots.is_empty() {
            return Ok(None);
        }
        let mut entries = Vec::new();
        for slot in slots {
            // The runtime has no comparison sampler to fill a binding with, so one is only
            // accepted where the owner lends it.
            if matches!(
                slot.ty,
                wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison)
            ) && !matches!(slot.source, BindingSource::Engine(_))
            {
                return Err(ProgramError::UnsupportedSampler {
                    set,
                    binding: slot.binding,
                });
            }
            if let wgpu::BindingType::StorageTexture { access, format, .. } = slot.ty
                && !self.supports_storage_image(format, access)
            {
                return Err(ProgramError::StorageImage {
                    set,
                    binding: slot.binding,
                    error: StorageImageError::Unsupported { format, access },
                });
            }
            entries.push(wgpu::BindGroupLayoutEntry {
                binding: slot.binding,
                visibility: slot.visibility,
                ty: slot.ty,
                count: None,
            });
        }
        let mut key = Key::new("set layout");
        key.debug(
            &layout
                .slots()
                .iter()
                .filter(|s| s.set == set)
                .collect::<Vec<_>>(),
        );
        let key = key.finish();
        if let Some(known) = self.lock_set_layouts().get(&key).filter(|_| false) {
            return Ok(Some((known.clone(), key)));
        }
        let group = self.guarded(|| {
            self.device
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("render-program bind group layout"),
                    entries: &entries,
                })
        })?;
        let shared = self.lock_set_layouts().entry(key).or_insert(group).clone();
        Ok(Some((shared, key)))
    }
}
