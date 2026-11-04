//! The descriptor resources a Vulkan stage module declares: its decorated bindings, read to
//! check them against the parameters a player stores for the variant.

use std::collections::BTreeMap;

/// What a module resource is, coarsely.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ResourceClass {
    /// An image or a combined image sampler.
    Image,
    Sampler,
    /// A struct in the uniform or storage buffer storage class.
    Block,
}

/// One variable of a module that has a binding decoration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModuleResource {
    pub set: u32,
    pub binding: u32,
    pub class: ResourceClass,
    /// The `Offset` decoration of each member of a block, in member order. Empty for other
    /// classes.
    pub member_offsets: Vec<u32>,
}

const OP_TYPE_IMAGE: u16 = 25;
const OP_TYPE_SAMPLER: u16 = 26;
const OP_TYPE_SAMPLED_IMAGE: u16 = 27;
const OP_TYPE_ARRAY: u16 = 28;
const OP_TYPE_RUNTIME_ARRAY: u16 = 29;
const OP_TYPE_STRUCT: u16 = 30;
const OP_TYPE_POINTER: u16 = 32;
const OP_VARIABLE: u16 = 59;
const OP_DECORATE: u16 = 71;
const OP_MEMBER_DECORATE: u16 = 72;
const DECORATION_BINDING: u32 = 33;
const DECORATION_DESCRIPTOR_SET: u32 = 34;
const DECORATION_OFFSET: u32 = 35;

/// The first instruction of a module, after its five header words.
const FIRST_INSTRUCTION: usize = 5;

#[derive(Default)]
struct Declarations {
    sets: BTreeMap<u32, u32>,
    bindings: BTreeMap<u32, u32>,
    /// Member offsets by struct ID, as (member index, offset).
    offsets: BTreeMap<u32, Vec<(u32, u32)>>,
    pointees: BTreeMap<u32, u32>,
    elements: BTreeMap<u32, u32>,
    classes: BTreeMap<u32, ResourceClass>,
    /// Variables as (pointer type ID, ID).
    variables: Vec<(u32, u32)>,
}

impl Declarations {
    fn record(&mut self, opcode: u16, operands: &[u32]) {
        match (opcode, operands) {
            (OP_DECORATE, &[target, DECORATION_DESCRIPTOR_SET, set, ..]) => {
                self.sets.insert(target, set);
            }
            (OP_DECORATE, &[target, DECORATION_BINDING, binding, ..]) => {
                self.bindings.insert(target, binding);
            }
            (OP_MEMBER_DECORATE, &[owner, member, DECORATION_OFFSET, offset, ..]) => {
                self.offsets
                    .entry(owner)
                    .or_default()
                    .push((member, offset));
            }
            (OP_TYPE_POINTER, &[id, _, pointee, ..]) => {
                self.pointees.insert(id, pointee);
            }
            (OP_TYPE_ARRAY | OP_TYPE_RUNTIME_ARRAY, &[id, element, ..]) => {
                self.elements.insert(id, element);
            }
            (OP_VARIABLE, &[pointer, id, ..]) => self.variables.push((pointer, id)),
            (OP_TYPE_IMAGE | OP_TYPE_SAMPLED_IMAGE, &[id, ..]) => {
                self.classes.insert(id, ResourceClass::Image);
            }
            (OP_TYPE_SAMPLER, &[id]) => {
                self.classes.insert(id, ResourceClass::Sampler);
            }
            (OP_TYPE_STRUCT, &[id, ..]) => {
                self.classes.insert(id, ResourceClass::Block);
            }
            _ => {}
        }
    }

    /// The type a variable's pointer type points at, through any arrays.
    fn pointee(&self, pointer: u32) -> Option<u32> {
        let mut ty = *self.pointees.get(&pointer)?;
        while let Some(&element) = self.elements.get(&ty) {
            ty = element;
        }
        Some(ty)
    }

    fn resource(&self, pointer: u32, id: u32) -> Option<ModuleResource> {
        let binding = *self.bindings.get(&id)?;
        let ty = self.pointee(pointer)?;
        let class = *self.classes.get(&ty)?;
        let mut offsets = self.offsets.get(&ty).cloned().unwrap_or_default();
        offsets.sort_unstable();
        Some(ModuleResource {
            set: self.sets.get(&id).copied().unwrap_or_default(),
            binding,
            class,
            member_offsets: offsets.into_iter().map(|(_, offset)| offset).collect(),
        })
    }
}

/// Lists the bound resources of a SPIR-V module, ordered by set and binding. Instructions
/// that are malformed end the scan; the module is expected to have passed
/// [`worldrunner_unity_serialized::smolv::entry_points`].
#[must_use]
pub fn module_resources(words: &[u32]) -> Vec<ModuleResource> {
    let mut declarations = Declarations::default();
    let mut at = FIRST_INSTRUCTION;
    while let Some(&head) = words.get(at) {
        let count = usize::try_from(head >> 16).unwrap_or_default();
        let Some(instruction) = words.get(at..at + count).filter(|_| count > 0) else {
            break;
        };
        let opcode = u16::try_from(head & 0xffff).unwrap_or_default();
        declarations.record(opcode, &instruction[1..]);
        at += count;
    }
    let mut resources: Vec<_> = declarations
        .variables
        .iter()
        .filter_map(|&(pointer, id)| declarations.resource(pointer, id))
        .collect();
    resources.sort_by_key(|resource| (resource.set, resource.binding));
    resources
}
