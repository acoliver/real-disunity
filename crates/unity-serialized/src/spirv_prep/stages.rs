//! One binding numbering for the stage modules of a program variant.
//!
//! Unity gives every resource of a variant one descriptor set and binding, whichever stage
//! declares it. The sampler split moves the bindings after each sampler it adds, so stages
//! that split different samplers would number the same resource differently. The plan here is
//! made from every stage's resources before any module is rewritten: a Unity binding that
//! some stage splits gets a sampler slot after its image in all of them, and each later
//! binding of the set moves up by the samplers below it, which is the numbering the split
//! gives a single module. A stage that does not declare a binding still leaves its slot free.

use std::collections::{BTreeMap, BTreeSet};

use super::{
    DECORATION_BINDING, HEADER_WORDS, OP_DECORATE, OP_VARIABLE, PrepareError, ResourceBinding,
    Split, decorated_bindings, instructions, opcode,
};

const OP_TYPE_IMAGE: u32 = 25;
const OP_TYPE_SAMPLER: u32 = 26;
const OP_TYPE_SAMPLED_IMAGE: u32 = 27;
const OP_TYPE_ARRAY: u32 = 28;
const OP_TYPE_RUNTIME_ARRAY: u32 = 29;
const OP_TYPE_STRUCT: u32 = 30;
const OP_TYPE_POINTER: u32 = 32;
const DECORATION_BUFFER_BLOCK: u32 = 3;
const STORAGE_CLASS_UNIFORM: u32 = 2;
const STORAGE_CLASS_STORAGE_BUFFER: u32 = 12;

/// What a resource is, as far as sharing a Unity binding between stages is concerned. A
/// combined image sampler and a separate image are both textures: the sampler slot a split
/// adds is not a stage's own resource until a stage declares it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Class {
    Texture,
    Sampler,
    UniformBuffer,
    StorageBuffer,
    Other,
}

impl Class {
    fn name(self) -> &'static str {
        match self {
            Self::Texture => "a texture",
            Self::Sampler => "a sampler",
            Self::UniformBuffer => "a uniform buffer",
            Self::StorageBuffer => "a storage buffer",
            Self::Other => "another resource",
        }
    }
}

/// A resource of one stage in Unity's module.
pub(super) struct Resource {
    id: u32,
    set: u32,
    unity_binding: u32,
    class: Class,
    /// For a split combined sampler, the ID of the sampler variable the split created.
    sampler: Option<u32>,
}

/// The type declarations the class of a variable depends on.
struct Types {
    /// Each pointer type's storage class and pointee.
    pointers: BTreeMap<u32, (u32, u32)>,
    /// Each variable's pointer type.
    variables: BTreeMap<u32, u32>,
    /// Array types' elements.
    elements: BTreeMap<u32, u32>,
    /// Image types and sampled image types, which a combined image sampler is.
    images: BTreeSet<u32>,
    samplers: BTreeSet<u32>,
    structs: BTreeSet<u32>,
    buffer_blocks: BTreeSet<u32>,
}

impl Types {
    fn new(words: &[u32]) -> Self {
        let mut types = Self {
            pointers: BTreeMap::new(),
            variables: BTreeMap::new(),
            elements: BTreeMap::new(),
            images: BTreeSet::new(),
            samplers: BTreeSet::new(),
            structs: BTreeSet::new(),
            buffer_blocks: BTreeSet::new(),
        };
        for instruction in instructions(words) {
            match (opcode(instruction), instruction) {
                (OP_TYPE_IMAGE | OP_TYPE_SAMPLED_IMAGE, &[_, id, ..]) => {
                    types.images.insert(id);
                }
                (OP_TYPE_SAMPLER, &[_, id]) => {
                    types.samplers.insert(id);
                }
                (OP_TYPE_ARRAY, &[_, id, element, _])
                | (OP_TYPE_RUNTIME_ARRAY, &[_, id, element]) => {
                    types.elements.insert(id, element);
                }
                (OP_TYPE_STRUCT, &[_, id, ..]) => {
                    types.structs.insert(id);
                }
                (OP_TYPE_POINTER, &[_, id, storage, pointee]) => {
                    types.pointers.insert(id, (storage, pointee));
                }
                (OP_VARIABLE, &[_, pointer, id, ..]) => {
                    types.variables.insert(id, pointer);
                }
                (OP_DECORATE, &[_, id, DECORATION_BUFFER_BLOCK]) => {
                    types.buffer_blocks.insert(id);
                }
                _ => {}
            }
        }
        types
    }

    /// The class of a variable. An array of resources is classed by its element.
    fn class(&self, variable: u32) -> Class {
        let Some(&(storage, mut pointee)) = self
            .variables
            .get(&variable)
            .and_then(|pointer| self.pointers.get(pointer))
        else {
            return Class::Other;
        };
        while let Some(&element) = self.elements.get(&pointee) {
            pointee = element;
        }
        if self.images.contains(&pointee) {
            Class::Texture
        } else if self.samplers.contains(&pointee) {
            Class::Sampler
        } else if !self.structs.contains(&pointee) {
            Class::Other
        } else if storage == STORAGE_CLASS_STORAGE_BUFFER
            || (storage == STORAGE_CLASS_UNIFORM && self.buffer_blocks.contains(&pointee))
        {
            Class::StorageBuffer
        } else if storage == STORAGE_CLASS_UNIFORM {
            Class::UniformBuffer
        } else {
            Class::Other
        }
    }
}

/// The resources of a stage `words`, read from the decorations of the module before and after
/// the sampler split. A split sampler is the new variable in its set at the binding after its
/// image's.
pub(super) fn stage_resources(words: &[u32], split: &Split) -> Result<Vec<Resource>, PrepareError> {
    let before = decorated_bindings(words);
    let after = decorated_bindings(&split.words);
    let added: BTreeMap<(u32, u32), u32> = after
        .iter()
        .filter(|(id, _)| !before.contains_key(id))
        .map(|(&id, &place)| (place, id))
        .collect();
    let types = Types::new(words);
    let mut resources = Vec::with_capacity(before.len());
    for (&id, &(set, unity_binding)) in &before {
        let &(after_set, binding) = after
            .get(&id)
            .filter(|(after_set, _)| *after_set == set)
            .ok_or(PrepareError::Bindings("a resource lost its binding"))?;
        let sampler = if split.combined.contains(&(set, unity_binding)) {
            let sampler = binding
                .checked_add(1)
                .and_then(|b| added.get(&(after_set, b)));
            Some(*sampler.ok_or(PrepareError::Bindings("a split sampler has no binding"))?)
        } else {
            None
        };
        resources.push(Resource {
            id,
            set,
            unity_binding,
            class: types.class(id),
            sampler,
        });
    }
    if added.len() != split.combined.len() {
        return Err(PrepareError::Bindings(
            "new bindings do not match the split samplers",
        ));
    }
    Ok(resources)
}

/// The shared table for stages whose resources are `stages`: every Unity binding any of them
/// declares, with the prepared binding it has in all of them.
///
/// # Errors
/// Fails when stages declare one Unity binding as different classes of resource, or a
/// prepared binding does not fit in 32 bits.
pub(super) fn plan(stages: &[Vec<Resource>]) -> Result<Vec<ResourceBinding>, PrepareError> {
    // The class of each Unity binding and whether any stage splits a sampler off it.
    let mut declared: BTreeMap<(u32, u32), (Class, bool)> = BTreeMap::new();
    for resource in stages.iter().flatten() {
        let (class, split) = declared
            .entry((resource.set, resource.unity_binding))
            .or_insert((resource.class, false));
        if *class != resource.class {
            return Err(PrepareError::BindingClass {
                set: resource.set,
                binding: resource.unity_binding,
                first: class.name(),
                second: resource.class.name(),
            });
        }
        *split |= resource.sampler.is_some();
    }
    let no_room = || PrepareError::Bindings("a prepared binding does not fit in 32 bits");
    let mut table = Vec::with_capacity(declared.len());
    let (mut current_set, mut moved) = (None, 0u32);
    for ((set, unity_binding), (_, split)) in declared {
        if current_set != Some(set) {
            (current_set, moved) = (Some(set), 0);
        }
        let binding = unity_binding.checked_add(moved).ok_or_else(no_room)?;
        let sampler_binding = if split {
            moved = moved.checked_add(1).ok_or_else(no_room)?;
            Some(binding.checked_add(1).ok_or_else(no_room)?)
        } else {
            None
        };
        table.push(ResourceBinding {
            set,
            unity_binding,
            binding,
            sampler_binding,
        });
    }
    Ok(table)
}

/// Rewrites the `Binding` decorations of a stage's resources, and of the samplers split from
/// them, to the shared `table`. Nothing else in the module changes.
pub(super) fn renumber(
    mut words: Vec<u32>,
    resources: &[Resource],
    table: &[ResourceBinding],
) -> Result<Vec<u32>, PrepareError> {
    let mut numbers = BTreeMap::new();
    for resource in resources {
        let entry = table
            .iter()
            .find(|e| (e.set, e.unity_binding) == (resource.set, resource.unity_binding))
            .ok_or(PrepareError::Bindings(
                "a resource is not in the shared plan",
            ))?;
        numbers.insert(resource.id, entry.binding);
        if let Some(sampler) = resource.sampler {
            let binding = entry.sampler_binding.ok_or(PrepareError::Bindings(
                "a split sampler has no planned slot",
            ))?;
            numbers.insert(sampler, binding);
        }
    }
    let mut at = HEADER_WORDS;
    while let Some(&head) = words.get(at) {
        let length = ((head >> 16) as usize).max(1);
        let target = match (head & 0xffff, words.get(at..at + length)) {
            (OP_DECORATE, Some(&[_, id, DECORATION_BINDING, _])) => numbers.get(&id),
            _ => None,
        };
        if let Some(&binding) = target {
            words[at + 3] = binding;
        }
        at += length;
    }
    Ok(words)
}
