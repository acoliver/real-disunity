//! Bind group layouts from the resources stages declare and the modules' own resources.

use std::collections::BTreeMap;

use crate::binding::{BindingAddress, ResourceBinding, ResourceLayout};
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

/// A binding a stage declares for itself but whose entry point does not use. It gets no slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnusedDeclaration {
    pub stage: StageKind,
    pub set: u32,
    pub binding: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum LayoutError {
    /// A frontend could not turn its own declarations into a layout; the reason names why.
    #[error("{reason}")]
    Declaration { reason: String },
    #[error("set {set} binding {binding} is declared twice")]
    DuplicateDeclaration { set: u32, binding: u32 },
    #[error("the module uses {name} at set {set} binding {binding}, which nothing declares")]
    UndeclaredResource {
        set: u32,
        binding: u32,
        name: String,
    },
    #[error("set {set} binding {binding} is declared as {declared} but the module has {found}")]
    Mismatch {
        set: u32,
        binding: u32,
        declared: String,
        found: String,
    },
    #[error(
        "set {set} binding {binding} is visible to {declared:?} but a {needed:?} stage uses it"
    )]
    Visibility {
        set: u32,
        binding: u32,
        declared: wgpu::ShaderStages,
        needed: wgpu::ShaderStages,
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
    unused: Vec<UnusedDeclaration>,
}

impl ProgramLayout {
    /// Merges the stages' bindings. A binding two stages share keeps one slot visible to both.
    /// A slot's size is the larger of what the shader needs and what its declaration asks for.
    ///
    /// # Errors
    /// Fails on a binding declared twice, a resource the module uses that nothing declares or
    /// that is declared as another kind or not visible to its stage, and stages that declare
    /// one binding differently.
    pub fn build(stages: &[&StageProgram]) -> Result<Self, LayoutError> {
        let mut merged: BTreeMap<(u32, u32), Slot> = BTreeMap::new();
        let mut unused = Vec::new();
        for stage in stages {
            let declared = &stage.declarations().resources;
            check_unique(declared)?;
            for resource in stage.reflected() {
                let slot = stage_slot(stage.module().kind, declared, resource)?;
                merge(&mut merged, slot)?;
            }
            unused.extend(unused_declarations(stage));
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
    pub fn unused(&self) -> &[UnusedDeclaration] {
        &self.unused
    }
}

fn check_unique(declared: &ResourceLayout) -> Result<(), LayoutError> {
    for (index, binding) in declared.bindings.iter().enumerate() {
        if declared.bindings[..index]
            .iter()
            .any(|b| b.at == binding.at)
        {
            return Err(LayoutError::DuplicateDeclaration {
                set: binding.at.set,
                binding: binding.at.binding,
            });
        }
    }
    Ok(())
}

fn stage_slot(
    stage: StageKind,
    declared: &ResourceLayout,
    resource: &Reflected,
) -> Result<Slot, LayoutError> {
    let at = BindingAddress {
        set: resource.set,
        binding: resource.binding,
    };
    let found = declared
        .find(at)
        .ok_or_else(|| LayoutError::UndeclaredResource {
            set: at.set,
            binding: at.binding,
            name: resource.name.clone(),
        })?;
    if !found.kind.matches(&resource.ty) {
        return Err(LayoutError::Mismatch {
            set: at.set,
            binding: at.binding,
            declared: format!("{:?}", found.kind),
            found: format!("{:?}", resource.ty),
        });
    }
    if !found.visibility.contains(stage.visibility()) {
        return Err(LayoutError::Visibility {
            set: at.set,
            binding: at.binding,
            declared: found.visibility,
            needed: stage.visibility(),
        });
    }
    Ok(Slot {
        set: at.set,
        binding: at.binding,
        visibility: found.visibility,
        ty: resource.ty,
        min_size: resource.min_size.max(found.kind.min_size()),
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

fn unused_declarations(stage: &StageProgram) -> Vec<UnusedDeclaration> {
    let kind = stage.module().kind;
    stage
        .declarations()
        .resources
        .bindings
        .iter()
        .filter(|declared| declared.visibility.intersects(kind.visibility()))
        .filter(|declared| !used_by(stage, declared))
        .map(|declared| UnusedDeclaration {
            stage: kind,
            set: declared.at.set,
            binding: declared.at.binding,
        })
        .collect()
}

fn used_by(stage: &StageProgram, declared: &ResourceBinding) -> bool {
    stage
        .reflected()
        .iter()
        .any(|r| r.set == declared.at.set && r.binding == declared.at.binding)
}
