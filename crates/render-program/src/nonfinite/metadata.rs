//! The records that are dropped from a module before it is rewritten: source-line markers and
//! the records of the shader debug-info instruction set.
//!
//! rspirv hoists an `OpLine` that precedes an `OpLabel` out of its function, and refuses an
//! `OpExtInst` outside every function, which naga accepts for this one set. A marker between an
//! entry block's label and its first variable also ends the prefix of variables the rewrite
//! reads. Dropping them from the words keeps all three from reaching the rewrite.

use std::collections::BTreeSet;

use rspirv::dr::{Module, Operand};
use rspirv::spirv::Op;

use crate::spirv::{HEADER_WORDS, instructions, opcode};

/// The one instruction set whose records naga ignores, as `OpExtInstImport` names it.
const DEBUG_INFO_SET: &str = "NonSemantic.Shader.DebugInfo.100";

pub(super) struct Stripped {
    /// The module's words without the dropped records.
    pub words: Vec<u32>,
    /// The result ids of the dropped debug-info records.
    dropped: BTreeSet<u32>,
}

impl Stripped {
    pub fn new(words: &[u32]) -> Self {
        let sets = debug_info_sets(words);
        let dropped = instructions(words)
            .filter(|i| is_record(i, &sets))
            .map(|i| i[2])
            .collect();
        let header = words.len().min(HEADER_WORDS);
        let mut kept = words[..header].to_vec();
        let mut covered = header;
        for i in instructions(words) {
            covered += i.len();
            if !is_marker(i) && !is_record(i, &sets) && !names_dropped(i, &dropped) {
                kept.extend_from_slice(i);
            }
        }
        // Words that are not whole instructions are left for the parser to refuse.
        kept.extend_from_slice(&words[covered..]);
        Self {
            words: kept,
            dropped,
        }
    }

    /// Whether an instruction left in `module` still reads a dropped record's result.
    pub fn is_used_by(&self, module: &Module) -> bool {
        module.all_inst_iter().any(|i| {
            let operands = i.operands.iter().filter_map(any_id);
            i.result_type
                .into_iter()
                .chain(operands)
                .any(|used| self.dropped.contains(&used))
        })
    }
}

/// The id an operand names, in any of the operand kinds that hold one.
fn any_id(operand: &Operand) -> Option<u32> {
    match operand {
        Operand::IdRef(id) | Operand::IdScope(id) | Operand::IdMemorySemantics(id) => Some(*id),
        _ => None,
    }
}

/// The ids that `OpExtInstImport` gives the debug-info set.
fn debug_info_sets(words: &[u32]) -> BTreeSet<u32> {
    instructions(words)
        .filter(|i| opcode(i) == Op::ExtInstImport as u32 && i.len() >= 3)
        .filter(|i| literal_is(&i[2..], DEBUG_INFO_SET))
        .map(|i| i[1])
        .collect()
}

/// Whether a literal string, UTF-8 bytes in little-endian words and ended by a NUL, is `text`.
fn literal_is(words: &[u32], text: &str) -> bool {
    let bytes: Vec<u8> = words.iter().flat_map(|word| word.to_le_bytes()).collect();
    bytes
        .iter()
        .position(|byte| *byte == 0)
        .is_some_and(|end| &bytes[..end] == text.as_bytes())
}

fn is_marker(instruction: &[u32]) -> bool {
    [Op::Line as u32, Op::NoLine as u32].contains(&opcode(instruction))
}

/// An `OpExtInst` of the debug-info set: head, result type, result id, set, instruction.
fn is_record(instruction: &[u32], sets: &BTreeSet<u32>) -> bool {
    opcode(instruction) == Op::ExtInst as u32
        && instruction.len() >= 5
        && sets.contains(&instruction[3])
}

/// An `OpName` of a dropped record. `OpMemberName` names a struct type, which a record never is.
fn names_dropped(instruction: &[u32], dropped: &BTreeSet<u32>) -> bool {
    opcode(instruction) == Op::Name as u32
        && instruction.len() >= 2
        && dropped.contains(&instruction[1])
}
