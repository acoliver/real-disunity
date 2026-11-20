//! SPIR-V words as a list of instructions, for the edits that rspirv's module layout cannot hold:
//! a line marker before an `OpLabel` and an `OpExtInst` outside every function.

use rspirv::spirv::Op;

use super::{INFINITY, NAN};
use crate::spirv::{instructions, opcode};

const FUNCTION_STORAGE: u32 = 7;

/// The opcodes a module may hold before its annotations: capabilities, extensions, imports,
/// the memory model, entry points, execution modes and the debug instructions.
const BEFORE_ANNOTATIONS: [Op; 14] = [
    Op::Capability,
    Op::Extension,
    Op::ExtInstImport,
    Op::MemoryModel,
    Op::EntryPoint,
    Op::ExecutionMode,
    Op::ExecutionModeId,
    Op::String,
    Op::SourceExtension,
    Op::Source,
    Op::SourceContinued,
    Op::Name,
    Op::MemberName,
    Op::ModuleProcessed,
];

pub struct Spirv {
    header: Vec<u32>,
    list: Vec<Vec<u32>>,
}

/// A literal string as words: UTF-8 bytes in little-endian order, then a NUL.
pub fn text(text: &str) -> Vec<u32> {
    let mut bytes = text.as_bytes().to_vec();
    bytes.push(0);
    bytes
        .chunks(4)
        .map(|chunk| {
            let mut word = [0; 4];
            word[..chunk.len()].copy_from_slice(chunk);
            u32::from_le_bytes(word)
        })
        .collect()
}

impl Spirv {
    pub fn new(words: &[u32]) -> Self {
        Self {
            header: words[..5].to_vec(),
            list: instructions(words).map(<[u32]>::to_vec).collect(),
        }
    }

    pub fn words(&self) -> Vec<u32> {
        [self.header.clone(), self.list.concat()].concat()
    }

    /// An id the module does not use yet.
    pub fn fresh_id(&mut self) -> u32 {
        let id = self.header[3];
        self.header[3] += 1;
        id
    }

    pub fn insert(&mut self, at: usize, op: Op, operands: &[u32]) {
        let size = u32::try_from(operands.len() + 1).unwrap();
        let head = (size << 16) | op as u32;
        self.list.insert(at, [&[head], operands].concat());
    }

    pub fn first(&self, op: Op) -> usize {
        self.list
            .iter()
            .position(|i| opcode(i) == op as u32)
            .unwrap_or_else(|| panic!("the module has no {op:?}"))
    }

    pub fn last(&self, op: Op) -> usize {
        self.list
            .iter()
            .rposition(|i| opcode(i) == op as u32)
            .unwrap_or_else(|| panic!("the module has no {op:?}"))
    }

    /// Where the debug names and strings end: before the first annotation, type or constant.
    pub fn after_debug(&self) -> usize {
        self.list
            .iter()
            .position(|i| !BEFORE_ANNOTATIONS.iter().any(|op| *op as u32 == opcode(i)))
            .unwrap()
    }

    /// The result id of the first instruction of `op`, for opcodes with no result type, whose
    /// result id is the first operand word.
    pub fn result_of(&self, op: Op) -> u32 {
        self.list[self.first(op)][1]
    }

    /// The result id of the first Function-storage variable.
    pub fn local_variable(&self) -> u32 {
        let variable = self
            .list
            .iter()
            .find(|i| opcode(i) == Op::Variable as u32 && i[3] == FUNCTION_STORAGE);
        variable.expect("the module has no local variable")[2]
    }

    /// The opcode of the instruction at `at`.
    pub fn opcode_at(&self, at: usize) -> u32 {
        opcode(&self.list[at])
    }

    /// The index of the `OpLabel` that starts the block whose first instruction is a local
    /// variable initialised with a non-finite constant: the shape that ends the entry block's
    /// variable prefix early once a line marker follows the label.
    pub fn entry_label(&self) -> usize {
        let non_finite = self.non_finite_constants();
        (0..self.list.len() - 1)
            .find(|&at| {
                opcode(&self.list[at]) == Op::Label as u32
                    && self.is_non_finite_local(at + 1, &non_finite)
            })
            .expect("no entry block starts with a variable initialised by a non-finite constant")
    }

    /// The index of the first local variable initialised with a non-finite constant.
    pub fn non_finite_local(&self) -> usize {
        let non_finite = self.non_finite_constants();
        (0..self.list.len())
            .find(|&at| self.is_non_finite_local(at, &non_finite))
            .expect("no local variable is initialised by a non-finite constant")
    }

    fn non_finite_constants(&self) -> Vec<u32> {
        self.list
            .iter()
            .filter(|i| opcode(i) == Op::Constant as u32 && [NAN, INFINITY].contains(&i[3]))
            .map(|i| i[2])
            .collect()
    }

    fn is_non_finite_local(&self, at: usize, non_finite: &[u32]) -> bool {
        let i = &self.list[at];
        opcode(i) == Op::Variable as u32
            && i[3] == FUNCTION_STORAGE
            && i.get(4).is_some_and(|init| non_finite.contains(init))
    }
}
