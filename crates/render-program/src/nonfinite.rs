//! NaN and infinity constants become values each function computes.
//!
//! Compilers write non-finite floats as `OpConstant` bit patterns, alone and inside
//! `OpConstantComposite`. naga turns every constant into a constant expression, and its validator
//! refuses a non-finite literal in any expression, constant or not. So the stage is rewritten
//! before naga reads it: each such constant is removed, and each function that used it builds
//! the value at the top of its entry block from a finite `u32` constant, stored in and loaded
//! from a local variable, and bitcast to the float. A load of a variable is not a constant
//! expression, so nothing in naga folds the bitcast back into a literal, and the bits reach the
//! GPU shader compiler as the module wrote them. A composite becomes an `OpCompositeConstruct`
//! of its parts, finite parts staying the original constants.
//!
//! Only 32-bit `OpConstant`s that are non-finite, and the composites that contain them, change.
//! A module without one is returned borrowed, word for word. A module is also returned borrowed
//! when a non-finite constant has a use a function cannot take over: the initialiser of a global
//! variable or an operand of a specialization constant instruction. naga then refuses it as it
//! always did. Specialization constants are never rewritten, and neither are 64-bit constants.
mod rewrite;
mod scan;
#[cfg(test)]
mod tests;

use std::borrow::Cow;

use rspirv::binary::Assemble;
use rspirv::spirv::Op;

use crate::module::ModuleError;
use crate::spirv::{instructions, opcode};

pub(crate) fn lower(words: &[u32]) -> Result<Cow<'_, [u32]>, ModuleError> {
    if !has_non_finite_constant(words) {
        return Ok(Cow::Borrowed(words));
    }
    let mut module = rspirv::dr::load_words(words)
        .map_err(|error| ModuleError::Parse(format!("SPIR-V: {error}")))?;
    let scan = scan::Scan::new(&module);
    if scan.blocked {
        return Ok(Cow::Borrowed(words));
    }
    rewrite::rewrite(&mut module, &scan)?;
    Ok(Cow::Owned(module.assemble()))
}

// Avoid grammar-parsing modules without such a constant, including extensions unknown to
// rspirv. A type is declared before the constants of that type.
fn has_non_finite_constant(words: &[u32]) -> bool {
    let mut floats = Vec::new();
    for i in instructions(words) {
        if opcode(i) == Op::TypeFloat as u32 && i.len() >= 3 && i[2] == 32 {
            floats.push(i[1]);
        } else if opcode(i) == Op::Constant as u32
            && i.len() == 4
            && floats.contains(&i[1])
            && !f32::from_bits(i[3]).is_finite()
        {
            return true;
        }
    }
    false
}

fn malformed(what: &str) -> ModuleError {
    ModuleError::Parse(format!("SPIR-V non-finite constants: {what}"))
}
