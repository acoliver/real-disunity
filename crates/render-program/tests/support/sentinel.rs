//! Synthetic SPIR-V with non-finite float constants.
//!
//! naga refuses a non-finite literal, so no naga module can carry one. The modules here are
//! written from WGSL that uses finite sentinel constants, and each sentinel's `OpConstant` words
//! are then replaced with the bit pattern a compiler would emit for NaN or infinity.

use naga::back::spv;
use naga::valid::{Capabilities, ValidationFlags, Validator};
use rspirv::binary::Assemble;
use rspirv::dr::Operand;
use rspirv::spirv::Op;

/// SPIR-V for `wgsl`, written as a Vulkan shader (the position output is not flipped), with
/// every 32-bit float constant equal to a sentinel's value given that sentinel's bits.
///
/// # Panics
/// Panics when `wgsl` does not compile or a sentinel is not a constant of the module, which
/// would make the test check nothing.
pub fn spirv(wgsl: &str, sentinels: &[(f32, u32)]) -> Vec<u32> {
    let module = naga::front::wgsl::parse_str(wgsl).unwrap();
    let info = Validator::new(ValidationFlags::all(), Capabilities::all())
        .validate(&module)
        .unwrap();
    let options = spv::Options {
        flags: spv::WriterFlags::empty(),
        ..spv::Options::default()
    };
    let words = spv::write_vec(&module, &info, &options, None).unwrap();
    let mut module = rspirv::dr::load_words(words).unwrap();
    let floats: Vec<u32> = module
        .types_global_values
        .iter()
        .filter(|i| i.class.opcode == Op::TypeFloat && i.operands == [Operand::LiteralBit32(32)])
        .filter_map(|i| i.result_id)
        .collect();
    let mut replaced = vec![0; sentinels.len()];
    for constant in &mut module.types_global_values {
        let is_float = constant.result_type.is_some_and(|ty| floats.contains(&ty));
        let Some(Operand::LiteralBit32(bits)) = constant.operands.first_mut() else {
            continue;
        };
        if constant.class.opcode != Op::Constant || !is_float {
            continue;
        }
        for (count, (sentinel, replacement)) in replaced.iter_mut().zip(sentinels) {
            if *bits == sentinel.to_bits() {
                *bits = *replacement;
                *count += 1;
                break;
            }
        }
    }
    for ((sentinel, _), count) in sentinels.iter().zip(&replaced) {
        assert!(*count > 0, "the module has no constant {sentinel}");
    }
    module.assemble()
}
