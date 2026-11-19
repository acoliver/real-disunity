use std::collections::BTreeSet;

use rspirv::dr::{Module, Operand};
use rspirv::spirv::Op;

use crate::spirv::id;

/// What a non-finite constant is made of.
pub(super) enum Shape {
    /// The bit pattern of a 32-bit float constant.
    Scalar(u32),
    /// The constants of a composite, at least one of them non-finite.
    Composite(Vec<u32>),
}

pub(super) struct Value {
    pub id: u32,
    pub ty: u32,
    pub shape: Shape,
}

pub(super) struct Scan {
    /// The non-finite constants and composites containing them, in declaration order, so every
    /// composite follows its parts.
    pub values: Vec<Value>,
    /// Whether a non-finite constant has a use that a function cannot take over.
    pub blocked: bool,
    ids: BTreeSet<u32>,
}

impl Scan {
    pub fn new(module: &Module) -> Self {
        let floats: BTreeSet<u32> = module
            .types_global_values
            .iter()
            .filter(|i| {
                i.class.opcode == Op::TypeFloat && i.operands == [Operand::LiteralBit32(32)]
            })
            .filter_map(|i| i.result_id)
            .collect();
        let mut scan = Self {
            values: Vec::new(),
            blocked: false,
            ids: BTreeSet::new(),
        };
        for i in &module.types_global_values {
            let parts = || i.operands.iter().filter_map(id);
            match (
                i.class.opcode,
                i.operands.as_slice(),
                i.result_id,
                i.result_type,
            ) {
                (Op::Constant, [Operand::LiteralBit32(bits)], Some(result), Some(ty))
                    if floats.contains(&ty) && !f32::from_bits(*bits).is_finite() =>
                {
                    scan.add(result, ty, Shape::Scalar(*bits));
                }
                (Op::ConstantComposite, _, Some(result), Some(ty))
                    if parts().any(|part| scan.contains(part)) =>
                {
                    scan.add(result, ty, Shape::Composite(parts().collect()));
                }
                _ => scan.blocked |= parts().any(|part| scan.contains(part)),
            }
        }
        scan
    }

    pub fn contains(&self, id: u32) -> bool {
        self.ids.contains(&id)
    }

    fn add(&mut self, id: u32, ty: u32, shape: Shape) {
        self.ids.insert(id);
        self.values.push(Value { id, ty, shape });
    }
}
