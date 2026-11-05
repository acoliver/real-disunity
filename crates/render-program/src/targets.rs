//! Where a stage's named constant buffers and textures are bound in the prepared module.

use worldrunner_unity_serialized::shader_program::ConstantBuffer;

use crate::declare::entry;
use crate::layout::LayoutError;
use crate::module::StageProgram;

/// A constant buffer's layout and the binding it fills.
#[derive(Clone, Copy, Debug)]
pub struct ConstantBufferTarget<'a> {
    pub set: u32,
    pub binding: u32,
    pub buffer: &'a ConstantBuffer,
}

/// A texture property's name index and the binding it fills.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextureTarget {
    pub name_index: i32,
    pub set: u32,
    pub binding: u32,
}

impl StageProgram {
    /// The constant buffers the parameters describe, each with its prepared set and binding.
    ///
    /// # Errors
    /// Fails on a binding outside the binding table, or one with no constant buffer record.
    pub fn constant_buffer_targets(&self) -> Result<Vec<ConstantBufferTarget<'_>>, LayoutError> {
        let parameters = self.parameters();
        let mut targets = Vec::new();
        for binding in &parameters.constant_buffer_bindings {
            let buffer = parameters
                .constant_buffers
                .iter()
                .find(|buffer| buffer.name_index == binding.name_index)
                .ok_or(LayoutError::UndescribedConstantBuffer {
                    name_index: binding.name_index,
                })?;
            let placed = entry(&self.module().bindings, binding.index)?;
            targets.push(ConstantBufferTarget {
                set: placed.set,
                binding: placed.binding,
                buffer,
            });
        }
        Ok(targets)
    }

    /// The texture parameters with their prepared set and binding.
    ///
    /// # Errors
    /// Fails on a binding outside the binding table.
    pub fn texture_targets(&self) -> Result<Vec<TextureTarget>, LayoutError> {
        self.parameters()
            .textures
            .iter()
            .map(|texture| {
                let placed = entry(&self.module().bindings, texture.index)?;
                Ok(TextureTarget {
                    name_index: texture.name_index,
                    set: placed.set,
                    binding: placed.binding,
                })
            })
            .collect()
    }
}
