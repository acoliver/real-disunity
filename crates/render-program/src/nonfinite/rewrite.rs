use std::collections::{BTreeMap, BTreeSet};

use rspirv::dr::{Function, Instruction, Module, Operand};
use rspirv::spirv::{Op, StorageClass};

use super::malformed;
use super::scan::{Scan, Shape, Value};
use crate::module::ModuleError;
use crate::spirv::{id, instruction};

struct Rewrite<'a> {
    scan: &'a Scan,
    next: u32,
    declarations: Vec<Instruction>,
    uint: u32,
    pointer: u32,
    /// The `u32` constant holding each non-finite bit pattern.
    patterns: BTreeMap<u32, u32>,
}

impl Rewrite<'_> {
    fn allocate(&mut self) -> Result<u32, ModuleError> {
        let id = self.next;
        self.next = id
            .checked_add(1)
            .ok_or_else(|| malformed("ID bound overflow"))?;
        Ok(id)
    }

    fn declare(
        &mut self,
        op: Op,
        ty: Option<u32>,
        operands: Vec<Operand>,
    ) -> Result<u32, ModuleError> {
        let id = self.allocate()?;
        self.declarations
            .push(instruction(op, ty, Some(id), operands));
        Ok(id)
    }

    fn type_id(
        &mut self,
        module: &Module,
        op: Op,
        operands: Vec<Operand>,
    ) -> Result<u32, ModuleError> {
        let existing = module
            .types_global_values
            .iter()
            .find(|i| i.class.opcode == op && i.operands == operands);
        match existing {
            Some(i) => Ok(i.result_id.unwrap()),
            None => self.declare(op, None, operands),
        }
    }

    /// The instructions that give a function the value of `value`: variables for its entry
    /// block's start, and the loads, bitcasts and constructions that follow them.
    fn materialize(
        &mut self,
        value: &Value,
        fresh: u32,
        renamed: &BTreeMap<u32, u32>,
        built: &mut Built,
    ) -> Result<(), ModuleError> {
        match &value.shape {
            Shape::Scalar(bits) => {
                let (variable, loaded) = (self.allocate()?, self.allocate()?);
                built.variables.push(instruction(
                    Op::Variable,
                    Some(self.pointer),
                    Some(variable),
                    vec![
                        Operand::StorageClass(StorageClass::Function),
                        Operand::IdRef(self.patterns[bits]),
                    ],
                ));
                built.values.push(instruction(
                    Op::Load,
                    Some(self.uint),
                    Some(loaded),
                    vec![Operand::IdRef(variable)],
                ));
                built.values.push(instruction(
                    Op::Bitcast,
                    Some(value.ty),
                    Some(fresh),
                    vec![Operand::IdRef(loaded)],
                ));
            }
            Shape::Composite(parts) => built.values.push(instruction(
                Op::CompositeConstruct,
                Some(value.ty),
                Some(fresh),
                parts
                    .iter()
                    .map(|part| Operand::IdRef(renamed.get(part).copied().unwrap_or(*part)))
                    .collect(),
            )),
        }
        Ok(())
    }

    /// The non-finite constants `function` uses, directly or as parts of composites it uses.
    fn needed(&self, function: &Function) -> BTreeSet<u32> {
        let mut needed: BTreeSet<u32> = function
            .all_inst_iter()
            .flat_map(|i| i.operands.iter().filter_map(id))
            .filter(|used| self.scan.contains(*used))
            .collect();
        for value in self.scan.values.iter().rev() {
            if let Shape::Composite(parts) = &value.shape
                && needed.contains(&value.id)
            {
                needed.extend(parts.iter().filter(|part| self.scan.contains(**part)));
            }
        }
        needed
    }

    fn function(&mut self, function: &mut Function) -> Result<(), ModuleError> {
        let needed = self.needed(function);
        if needed.is_empty() {
            return Ok(());
        }
        let mut renamed = BTreeMap::new();
        let mut built = Built::default();
        for value in self.scan.values.iter().filter(|v| needed.contains(&v.id)) {
            let fresh = self.allocate()?;
            self.materialize(value, fresh, &renamed, &mut built)?;
            renamed.insert(value.id, fresh);
        }
        let entry = function
            .blocks
            .first_mut()
            .ok_or_else(|| malformed("function without a block"))?;
        let mut head = std::mem::take(&mut entry.instructions);
        let variables = head
            .iter()
            .position(|i| i.class.opcode != Op::Variable)
            .unwrap_or(head.len());
        let tail = head.split_off(variables);
        let stores = take_initializers(&mut head, &renamed);
        entry.instructions = [built.variables, head, built.values, stores, tail].concat();
        for i in function
            .blocks
            .iter_mut()
            .flat_map(|block| &mut block.instructions)
        {
            rename(i, &renamed);
        }
        Ok(())
    }
}

#[derive(Default)]
struct Built {
    variables: Vec<Instruction>,
    values: Vec<Instruction>,
}

// A local variable's initialiser must be a constant, which a rebuilt value is not: the
// variable starts uninitialised and is stored to once the value exists.
fn take_initializers(
    variables: &mut [Instruction],
    renamed: &BTreeMap<u32, u32>,
) -> Vec<Instruction> {
    let mut stores = Vec::new();
    for variable in variables {
        let initial = variable.operands.get(1).and_then(id);
        if let Some(value) = initial.and_then(|initial| renamed.get(&initial)) {
            variable.operands.truncate(1);
            stores.push(instruction(
                Op::Store,
                None,
                None,
                vec![
                    Operand::IdRef(variable.result_id.unwrap()),
                    Operand::IdRef(*value),
                ],
            ));
        }
    }
    stores
}

fn rename(i: &mut Instruction, renamed: &BTreeMap<u32, u32>) {
    for operand in &mut i.operands {
        if let Operand::IdRef(used) = operand
            && let Some(fresh) = renamed.get(used)
        {
            *used = *fresh;
        }
    }
}

pub(super) fn rewrite(module: &mut Module, scan: &Scan) -> Result<(), ModuleError> {
    let mut rewrite = Rewrite {
        scan,
        next: module
            .header
            .as_ref()
            .ok_or_else(|| malformed("header"))?
            .bound,
        declarations: vec![],
        uint: 0,
        pointer: 0,
        patterns: BTreeMap::new(),
    };
    let literals = [Operand::LiteralBit32(32), Operand::LiteralBit32(0)];
    rewrite.uint = rewrite.type_id(module, Op::TypeInt, literals.to_vec())?;
    rewrite.pointer = rewrite.type_id(
        module,
        Op::TypePointer,
        vec![
            Operand::StorageClass(StorageClass::Function),
            Operand::IdRef(rewrite.uint),
        ],
    )?;
    for value in &scan.values {
        if let Shape::Scalar(bits) = value.shape
            && !rewrite.patterns.contains_key(&bits)
        {
            let constant = rewrite.declare(
                Op::Constant,
                Some(rewrite.uint),
                vec![Operand::LiteralBit32(bits)],
            )?;
            rewrite.patterns.insert(bits, constant);
        }
    }
    for function in &mut module.functions {
        rewrite.function(function)?;
    }
    module
        .types_global_values
        .retain(|i| !i.result_id.is_some_and(|result| scan.contains(result)));
    module.types_global_values.append(&mut rewrite.declarations);
    let removed = |i: &Instruction| {
        i.operands
            .first()
            .and_then(id)
            .is_some_and(|target| scan.contains(target))
    };
    module.annotations.retain(|i| !removed(i));
    module.debug_names.retain(|i| !removed(i));
    module.header.as_mut().unwrap().bound = rewrite.next;
    Ok(())
}
