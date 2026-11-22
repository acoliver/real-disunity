use std::collections::{BTreeMap, BTreeSet};

use rspirv::dr::{Instruction, Module, Operand};
use rspirv::spirv::{Capability, Decoration, Op, StorageClass};

use super::scan::{Image, Scan};
use super::{id, instruction, malformed};
use crate::module::ModuleError;
use crate::spirv_rewrite::Declarations;

struct Element {
    ty: u32,
    pointer: u32,
    zero: u32,
    one: u32,
}

struct Rewrite {
    declarations: Declarations,
    annotations: Vec<Instruction>,
    elements: BTreeMap<u32, Element>,
    index_zero: u32,
}

impl Rewrite {
    fn element(
        &mut self,
        module: &Module,
        image_id: u32,
        image: Image,
    ) -> Result<Element, ModuleError> {
        let ty = if image.components == 1 {
            image.scalar
        } else {
            self.declarations.type_id(
                module,
                Op::TypeVector,
                vec![
                    Operand::IdRef(image.scalar),
                    Operand::LiteralBit32(image.components),
                ],
            )?
        };
        let array =
            self.declarations
                .declare(Op::TypeRuntimeArray, None, vec![Operand::IdRef(ty)])?;
        self.decorate_block(image_id, array, image.components);
        self.declarations.declare_as(instruction(
            Op::TypeStruct,
            None,
            Some(image_id),
            vec![Operand::IdRef(array)],
        ));
        let pointer = self.declarations.type_id(
            module,
            Op::TypePointer,
            vec![
                Operand::StorageClass(StorageClass::StorageBuffer),
                Operand::IdRef(ty),
            ],
        )?;
        let zero = self.declarations.declare(
            Op::Constant,
            Some(image.scalar),
            vec![Operand::LiteralBit32(0)],
        )?;
        let one = self.declarations.declare(
            Op::Constant,
            Some(image.scalar),
            vec![Operand::LiteralBit32(if image.float {
                1.0f32.to_bits()
            } else {
                1
            })],
        )?;
        Ok(Element {
            ty,
            pointer,
            zero,
            one,
        })
    }

    fn decorate_block(&mut self, block: u32, array: u32, components: u32) {
        self.annotations.extend([
            decorate(array, Decoration::ArrayStride, &[4 * components]),
            decorate(block, Decoration::Block, &[]),
            instruction(
                Op::MemberDecorate,
                None,
                None,
                vec![
                    Operand::IdRef(block),
                    Operand::LiteralBit32(0),
                    Operand::Decoration(Decoration::Offset),
                    Operand::LiteralBit32(0),
                ],
            ),
            instruction(
                Op::MemberDecorate,
                None,
                None,
                vec![
                    Operand::IdRef(block),
                    Operand::LiteralBit32(0),
                    Operand::Decoration(Decoration::NonWritable),
                ],
            ),
        ]);
    }

    fn fetch(&mut self, i: &Instruction, scan: &Scan) -> Result<Vec<Instruction>, ModuleError> {
        let variable = scan.loads[&id(&i.operands[0]).unwrap()];
        let image = scan.variables[&variable];
        let pointer_id = self.declarations.allocate()?;
        let components = scan.images[&image].components;
        let value_id = if components == 4 {
            i.result_id.unwrap()
        } else {
            self.declarations.allocate()?
        };
        let element = &self.elements[&image];
        let mut result = vec![
            instruction(
                Op::AccessChain,
                Some(element.pointer),
                Some(pointer_id),
                vec![
                    Operand::IdRef(variable),
                    Operand::IdRef(self.index_zero),
                    i.operands[1].clone(),
                ],
            ),
            instruction(
                Op::Load,
                Some(element.ty),
                Some(value_id),
                vec![Operand::IdRef(pointer_id)],
            ),
        ];
        if components == 4 {
            return Ok(result);
        }
        let mut values = vec![Operand::IdRef(value_id)];
        let zero = element.zero;
        let one = element.one;
        if components == 2 {
            values.clear();
            for index in 0..2 {
                let component = self.declarations.allocate()?;
                result.push(instruction(
                    Op::CompositeExtract,
                    Some(scan.images[&image].scalar),
                    Some(component),
                    vec![Operand::IdRef(value_id), Operand::LiteralBit32(index)],
                ));
                values.push(Operand::IdRef(component));
            }
        }
        while values.len() < 3 {
            values.push(Operand::IdRef(zero));
        }
        values.push(Operand::IdRef(one));
        result.push(instruction(
            Op::CompositeConstruct,
            i.result_type,
            i.result_id,
            values,
        ));
        Ok(result)
    }
}

pub(super) fn rewrite(module: &mut Module, scan: &Scan) -> Result<(), ModuleError> {
    let mut rewrite = Rewrite {
        declarations: Declarations::new(module)?,
        annotations: vec![],
        elements: BTreeMap::new(),
        index_zero: 0,
    };
    let index_type = match module.types_global_values.iter().find(|i| {
        i.class.opcode == Op::TypeInt
            && i.operands == [Operand::LiteralBit32(32), Operand::LiteralBit32(0)]
    }) {
        Some(i) => i.result_id.unwrap(),
        None => rewrite.declarations.declare(
            Op::TypeInt,
            None,
            vec![Operand::LiteralBit32(32), Operand::LiteralBit32(0)],
        )?,
    };
    rewrite.index_zero = rewrite.declarations.declare(
        Op::Constant,
        Some(index_type),
        vec![Operand::LiteralBit32(0)],
    )?;
    for (&image_id, &image) in &scan.images {
        let element = rewrite.element(module, image_id, image)?;
        rewrite.elements.insert(image_id, element);
    }
    replace_globals(module, scan, &mut rewrite)?;
    for function in &mut module.functions {
        for block in &mut function.blocks {
            let mut instructions = Vec::new();
            for i in std::mem::take(&mut block.instructions) {
                if i.result_id.is_some_and(|id| scan.loads.contains_key(&id)) {
                    continue;
                }
                if i.class.opcode == Op::ImageFetch
                    && i.operands
                        .first()
                        .and_then(id)
                        .is_some_and(|id| scan.loads.contains_key(&id))
                {
                    instructions.extend(rewrite.fetch(&i, scan)?);
                } else {
                    instructions.push(i);
                }
            }
            block.instructions = instructions;
        }
    }
    module.header.as_mut().unwrap().bound = rewrite.declarations.bound();
    module
        .capabilities
        .retain(|i| i.operands != [Operand::Capability(Capability::SampledBuffer)]);
    add_storage_extension(module);
    Ok(())
}

fn replace_globals(
    module: &mut Module,
    scan: &Scan,
    rewrite: &mut Rewrite,
) -> Result<(), ModuleError> {
    module
        .types_global_values
        .retain(|i| !i.result_id.is_some_and(|id| scan.images.contains_key(&id)));
    for i in &mut module.types_global_values {
        if i.result_id
            .is_some_and(|id| scan.pointers.contains_key(&id))
        {
            i.operands[0] = Operand::StorageClass(StorageClass::StorageBuffer);
        }
        if i.result_id
            .is_some_and(|id| scan.variables.contains_key(&id))
        {
            i.operands[0] = Operand::StorageClass(StorageClass::StorageBuffer);
        }
    }
    module
        .types_global_values
        .append(&mut rewrite.declarations.take());
    module.types_global_values = order(std::mem::take(&mut module.types_global_values))?;
    module.annotations.append(&mut rewrite.annotations);
    for &variable in scan.variables.keys() {
        let annotation = decorate(variable, Decoration::NonWritable, &[]);
        if !module.annotations.contains(&annotation) {
            module.annotations.push(annotation);
        }
    }
    module.annotations.retain(|i| {
        !i.operands
            .first()
            .and_then(id)
            .is_some_and(|id| scan.loads.contains_key(&id))
    });
    module.debug_names.retain(|i| {
        !i.operands
            .first()
            .and_then(id)
            .is_some_and(|id| scan.loads.contains_key(&id))
    });
    Ok(())
}

fn add_storage_extension(module: &mut Module) {
    let extension = vec![Operand::LiteralString(
        "SPV_KHR_storage_buffer_storage_class".into(),
    )];
    if module.header.as_ref().unwrap().version < 0x0001_0300
        && !module.extensions.iter().any(|i| i.operands == extension)
    {
        module
            .extensions
            .push(instruction(Op::Extension, None, None, extension));
    }
}

// DXC may declare the element vector after the image type. Stable dependency ordering
// puts every new runtime array and block after its element without moving function bodies.
fn order(mut pending: Vec<Instruction>) -> Result<Vec<Instruction>, ModuleError> {
    let ids: BTreeSet<_> = pending.iter().filter_map(|i| i.result_id).collect();
    let mut defined = BTreeSet::new();
    let mut ordered = Vec::new();
    while !pending.is_empty() {
        let before = pending.len();
        let mut waiting = Vec::new();
        for i in pending {
            let ready = i
                .result_type
                .into_iter()
                .chain(i.operands.iter().filter_map(id))
                .all(|id| !ids.contains(&id) || defined.contains(&id));
            if ready {
                defined.extend(i.result_id);
                ordered.push(i);
            } else {
                waiting.push(i);
            }
        }
        if waiting.len() == before {
            return Err(malformed("cyclic type declarations"));
        }
        pending = waiting;
    }
    Ok(ordered)
}

fn decorate(target: u32, kind: Decoration, literals: &[u32]) -> Instruction {
    let mut operands = vec![Operand::IdRef(target), Operand::Decoration(kind)];
    operands.extend(literals.iter().copied().map(Operand::LiteralBit32));
    instruction(Op::Decorate, None, None, operands)
}
