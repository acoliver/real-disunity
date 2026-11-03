//! Bind group layouts from the parameters Unity declares and the resources a module uses.

use std::collections::BTreeMap;

use crate::declare::{Class, Declared, declarations};
use crate::module::{StageKind, StageProgram};
use crate::reflect::Reflected;

/// One binding of a bind group.
#[derive(Clone, Debug, PartialEq)]
pub struct Slot {
    /// The descriptor set, which is the bind group index.
    pub set: u32,
    pub binding: u32,
    pub visibility: wgpu::ShaderStages,
    pub ty: wgpu::BindingType,
    /// Bytes a buffer binding must hold; zero for textures and samplers.
    pub min_size: u64,
}

/// A parameter a stage declares but whose entry point does not use. It gets no slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnusedParameter {
    pub stage: StageKind,
    pub set: u32,
    pub binding: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum LayoutError {
    #[error("{kind} parameters are not bound by this renderer")]
    UnsupportedParameter { kind: String },
    #[error("parameter binding {unity_binding} is not in the module's binding table")]
    UnboundResource { unity_binding: i64 },
    #[error("binding {unity_binding} is in several descriptor sets of the binding table")]
    AmbiguousBinding { unity_binding: u32 },
    #[error("parameter binding {binding} in set {set} is not in the module's binding table")]
    UnboundSlot { set: u32, binding: u32 },
    #[error("constant buffer name {name_index} has a binding but no buffer record")]
    UndescribedConstantBuffer { name_index: i32 },
    #[error("constant buffer {name:?} has a binding but no layout of that name")]
    UndescribedBuffer { name: String },
    #[error("texture dimension code {code} has not been checked against a module")]
    UnmeasuredTextureDimension { code: i32 },
    #[error(
        "the {stage:?} module uses set {set} binding {binding}, which the parameters bind for \
         stages {stages:#b} only"
    )]
    StageMismatch {
        set: u32,
        binding: u32,
        stage: StageKind,
        stages: u8,
    },
    #[error("the module uses {name} at set {set} binding {binding}, which no parameter declares")]
    UndeclaredResource {
        set: u32,
        binding: u32,
        name: String,
    },
    #[error("set {set} binding {binding} is declared as {declared} but the module has {found}")]
    Mismatch {
        set: u32,
        binding: u32,
        declared: &'static str,
        found: String,
    },
    #[error("set {set} binding {binding} is {first} in one stage and {second} in another")]
    Conflict {
        set: u32,
        binding: u32,
        first: String,
        second: String,
    },
}

/// Every binding of a program's stages, merged.
#[derive(Clone, Debug, PartialEq)]
pub struct ProgramLayout {
    slots: Vec<Slot>,
    unused: Vec<UnusedParameter>,
}

impl ProgramLayout {
    /// Merges the stages' bindings. A binding two stages share keeps one slot visible to both.
    ///
    /// # Errors
    /// Fails on a parameter kind that is not bound, a parameter outside the binding table, a
    /// resource without a parameter or of another kind than its parameter, or stages that
    /// declare one binding differently.
    pub fn build(stages: &[&StageProgram]) -> Result<Self, LayoutError> {
        let mut merged: BTreeMap<(u32, u32), Slot> = BTreeMap::new();
        let mut unused = Vec::new();
        for stage in stages {
            let declared = declarations(
                stage.parameters(),
                &stage.module().bindings,
                stage.module().kind,
            )?;
            for resource in stage.resources() {
                let slot = stage_slot(stage.module().kind, &declared, resource)?;
                merge(&mut merged, slot)?;
            }
            unused.extend(unused_parameters(stage, &declared));
        }
        Ok(Self {
            slots: merged.into_values().collect(),
            unused,
        })
    }

    /// The slots ordered by set and binding.
    #[must_use]
    pub fn slots(&self) -> &[Slot] {
        &self.slots
    }

    #[must_use]
    pub fn slot(&self, set: u32, binding: u32) -> Option<&Slot> {
        self.slots
            .iter()
            .find(|slot| slot.set == set && slot.binding == binding)
    }

    #[must_use]
    pub fn unused(&self) -> &[UnusedParameter] {
        &self.unused
    }
}

fn stage_slot(
    kind: StageKind,
    declared: &[Declared],
    resource: &Reflected,
) -> Result<Slot, LayoutError> {
    let at_slot: Vec<&Declared> = declared
        .iter()
        .filter(|d| d.set == resource.set && d.binding == resource.binding)
        .collect();
    let found = at_slot
        .iter()
        .find(|d| d.applies(kind))
        .or(at_slot.first())
        .copied()
        .ok_or_else(|| LayoutError::UndeclaredResource {
            set: resource.set,
            binding: resource.binding,
            name: resource.name.clone(),
        })?;
    if !found.applies(kind) {
        return Err(LayoutError::StageMismatch {
            set: resource.set,
            binding: resource.binding,
            stage: kind,
            stages: found.stages.unwrap_or_default(),
        });
    }
    if !found.class.matches(&resource.ty) {
        return Err(LayoutError::Mismatch {
            set: resource.set,
            binding: resource.binding,
            declared: found.class.name(),
            found: format!("{:?}", resource.ty),
        });
    }
    Ok(Slot {
        set: resource.set,
        binding: resource.binding,
        visibility: kind.visibility() | found.visibility(),
        ty: resource.ty,
        min_size: resource.min_size,
    })
}

fn merge(merged: &mut BTreeMap<(u32, u32), Slot>, slot: Slot) -> Result<(), LayoutError> {
    let Some(existing) = merged.get_mut(&(slot.set, slot.binding)) else {
        merged.insert((slot.set, slot.binding), slot);
        return Ok(());
    };
    if existing.ty != slot.ty {
        return Err(LayoutError::Conflict {
            set: slot.set,
            binding: slot.binding,
            first: format!("{:?}", existing.ty),
            second: format!("{:?}", slot.ty),
        });
    }
    existing.visibility |= slot.visibility;
    existing.min_size = existing.min_size.max(slot.min_size);
    Ok(())
}

fn unused_parameters(stage: &StageProgram, declared: &[Declared]) -> Vec<UnusedParameter> {
    declared
        .iter()
        .filter(|d| {
            d.applies(stage.module().kind)
                && !stage
                    .resources()
                    .iter()
                    .any(|r| r.set == d.set && r.binding == d.binding)
        })
        .map(|d| UnusedParameter {
            stage: stage.module().kind,
            set: d.set,
            binding: d.binding,
        })
        .collect()
}

impl Class {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::ConstantBuffer => "a constant buffer",
            Self::Texture { .. } => "a texture",
            Self::Sampler => "a sampler",
            Self::StructuredBuffer => "a structured buffer",
            Self::UnorderedAccess => "a UAV",
            Self::StorageBuffer => "a buffer block",
        }
    }

    fn matches(self, ty: &wgpu::BindingType) -> bool {
        use wgpu::{BindingType as B, BufferBindingType as Buf};
        match (self, ty) {
            (
                Self::ConstantBuffer,
                B::Buffer {
                    ty: Buf::Uniform, ..
                },
            )
            | (Self::Sampler, B::Sampler(_))
            | (
                Self::UnorderedAccess | Self::StorageBuffer,
                B::Buffer {
                    ty: Buf::Storage { .. },
                    ..
                },
            )
            | (
                Self::StructuredBuffer,
                B::Buffer {
                    ty: Buf::Storage { read_only: true },
                    ..
                },
            ) => true,
            (
                Self::Texture {
                    dimension,
                    multisampled,
                },
                B::Texture {
                    view_dimension,
                    multisampled: found_multisampled,
                    ..
                },
            ) => dimension == *view_dimension && multisampled == *found_multisampled,
            _ => false,
        }
    }
}
