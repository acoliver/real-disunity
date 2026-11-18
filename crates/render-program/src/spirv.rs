//! Helpers shared by the rewrites that prepare SPIR-V before naga parses it.

use rspirv::dr::{Instruction, Operand};
use rspirv::spirv::Op;

pub(crate) fn instruction(
    op: Op,
    ty: Option<u32>,
    id: Option<u32>,
    operands: Vec<Operand>,
) -> Instruction {
    Instruction::new(op, ty, id, operands)
}

pub(crate) fn id(operand: &Operand) -> Option<u32> {
    match operand {
        Operand::IdRef(id) => Some(*id),
        _ => None,
    }
}

/// The instructions of a module's words after the five-word header, each with its head word.
/// The walk stops at the first instruction whose length does not fit, leaving diagnostics for a
/// malformed stream to naga.
pub(crate) fn instructions(words: &[u32]) -> impl Iterator<Item = &[u32]> {
    let mut rest = words.get(5..).unwrap_or_default();
    std::iter::from_fn(move || {
        let size = (*rest.first()? >> 16) as usize;
        if size == 0 || size > rest.len() {
            return None;
        }
        let (instruction, tail) = rest.split_at(size);
        rest = tail;
        Some(instruction)
    })
}

/// The opcode in an instruction's head word.
pub(crate) fn opcode(instruction: &[u32]) -> u32 {
    instruction[0] & 0xffff
}
