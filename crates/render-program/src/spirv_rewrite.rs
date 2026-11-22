//! The id allocation and global declarations shared by the rewrites that add types and
//! constants to a SPIR-V module before naga parses it.

use rspirv::dr::{Instruction, Module, Operand};
use rspirv::spirv::Op;

use crate::module::ModuleError;
use crate::spirv::instruction;

/// Fresh ids above a module's bound and the global declarations made with them.
pub(crate) struct Declarations {
    next: u32,
    declared: Vec<Instruction>,
}

impl Declarations {
    pub(crate) fn new(module: &Module) -> Result<Self, ModuleError> {
        let header = module
            .header
            .as_ref()
            .ok_or_else(|| malformed("module without a header"))?;
        Ok(Self {
            next: header.bound,
            declared: Vec::new(),
        })
    }

    pub(crate) fn allocate(&mut self) -> Result<u32, ModuleError> {
        let id = self.next;
        self.next = id
            .checked_add(1)
            .ok_or_else(|| malformed("ID bound overflow"))?;
        Ok(id)
    }

    /// The id bound the module needs once everything allocated so far is in it.
    pub(crate) fn bound(&self) -> u32 {
        self.next
    }

    pub(crate) fn declare(
        &mut self,
        op: Op,
        ty: Option<u32>,
        operands: Vec<Operand>,
    ) -> Result<u32, ModuleError> {
        let id = self.allocate()?;
        self.declared.push(instruction(op, ty, Some(id), operands));
        Ok(id)
    }

    /// Declares an instruction under an id the caller already chose.
    pub(crate) fn declare_as(&mut self, instruction: Instruction) {
        self.declared.push(instruction);
    }

    /// The id of the type `op` with `operands`, in the module or already declared here, declared
    /// when neither has it.
    pub(crate) fn type_id(
        &mut self,
        module: &Module,
        op: Op,
        operands: Vec<Operand>,
    ) -> Result<u32, ModuleError> {
        let existing = module
            .types_global_values
            .iter()
            .chain(&self.declared)
            .find(|i| i.class.opcode == op && i.operands == operands);
        match existing {
            Some(i) => Ok(i.result_id.unwrap()),
            None => self.declare(op, None, operands),
        }
    }

    /// The declarations made so far, for the caller to place in the module.
    pub(crate) fn take(&mut self) -> Vec<Instruction> {
        std::mem::take(&mut self.declared)
    }
}

fn malformed(what: &str) -> ModuleError {
    ModuleError::Parse(format!("SPIR-V rewrite: {what}"))
}
