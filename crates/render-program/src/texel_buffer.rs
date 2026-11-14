//! Exact 32-bit buffer-image fetches become ordinary read-only storage-buffer loads.
//! Modules without Buffer images borrow their words unless unused `SampledBuffer` or
//! `ImageBuffer` capabilities need removal. That path copies every other word unchanged.
mod rewrite;
mod scan;
#[cfg(test)]
mod tests;

use std::borrow::Cow;

use rspirv::binary::Assemble;
use rspirv::dr::{Instruction, Operand};
use rspirv::spirv::{Capability, Op};

use crate::module::ModuleError;

pub(crate) fn lower(words: &[u32]) -> Result<Cow<'_, [u32]>, ModuleError> {
    if !has_buffer_image(words) {
        return Ok(without_unused_capabilities(words));
    }
    let mut module = rspirv::dr::load_words(words)
        .map_err(|error| ModuleError::Parse(format!("SPIR-V: {error}")))?;
    let scan = scan::Scan::new(&module)?;
    rewrite::rewrite(&mut module, &scan)?;
    Ok(Cow::Owned(module.assemble()))
}

// Avoid grammar-parsing modules without buffer images, including extensions unknown
// to rspirv. Leave validation and diagnostics for malformed instruction streams to naga.
fn without_unused_capabilities(words: &[u32]) -> Cow<'_, [u32]> {
    let mut output = None;
    let mut copied = 0;
    let mut at = 5;
    while let Some(&head) = words.get(at) {
        let size = (head >> 16) as usize;
        if size == 0 || size > words.len() - at {
            return Cow::Borrowed(words);
        }
        if head & 0xffff == Op::Capability as u32
            && size == 2
            && (words[at + 1] == Capability::SampledBuffer as u32
                || words[at + 1] == Capability::ImageBuffer as u32)
        {
            output
                .get_or_insert_with(Vec::new)
                .extend_from_slice(&words[copied..at]);
            copied = at + size;
        }
        at += size;
    }
    match output {
        Some(mut output) => {
            output.extend_from_slice(&words[copied..]);
            Cow::Owned(output)
        }
        None => Cow::Borrowed(words),
    }
}

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
