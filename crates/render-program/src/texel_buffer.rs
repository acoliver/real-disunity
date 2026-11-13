//! Exact 32-bit buffer-image fetches become ordinary read-only storage-buffer loads.
mod rewrite;
mod scan;
#[cfg(test)]
mod tests;

use std::borrow::Cow;

use rspirv::binary::Assemble;
use rspirv::dr::{Instruction, Operand};
use rspirv::spirv::Op;

use crate::module::ModuleError;

pub(crate) fn lower(words: &[u32]) -> Result<Cow<'_, [u32]>, ModuleError> {
    if !has_buffer_image(words) {
        return Ok(Cow::Borrowed(words));
    }
    let mut module = rspirv::dr::load_words(words)
        .map_err(|error| ModuleError::Parse(format!("SPIR-V: {error}")))?;
    let scan = scan::Scan::new(&module)?;
    rewrite::rewrite(&mut module, &scan)?;
    Ok(Cow::Owned(module.assemble()))
}

// Do not round-trip or even grammar-parse existing modules without buffer images. Their
// validation and diagnostics stay with naga, including extensions this pass does not know.
fn has_buffer_image(words: &[u32]) -> bool {
    let mut at = 5;
    while let Some(&head) = words.get(at) {
        let size = (head >> 16) as usize;
        if size == 0 || size > words.len() - at {
            return false;
        }
        if head & 0xffff == Op::TypeImage as u32 && size >= 9 && words[at + 3] == 5 {
            return true;
        }
        at += size;
    }
    false
}

fn instruction(op: Op, ty: Option<u32>, id: Option<u32>, operands: Vec<Operand>) -> Instruction {
    Instruction::new(op, ty, id, operands)
}

fn id(operand: &Operand) -> Option<u32> {
    match operand {
        Operand::IdRef(id) => Some(*id),
        _ => None,
    }
}

fn malformed(what: &str) -> ModuleError {
    ModuleError::Parse(format!("SPIR-V texel buffer: {what}"))
}
