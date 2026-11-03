//! Where a stage's named constant buffers and textures are bound in the prepared module.

use crate::buffers::{BufferLayout, ParamName};
use crate::declare::{applies, entry};
use crate::layout::LayoutError;
use crate::module::StageProgram;
use crate::source::Source;

/// A constant buffer's layout and the binding it fills.
#[derive(Clone, Debug)]
pub struct ConstantBufferTarget {
    pub set: u32,
    pub binding: u32,
    pub buffer: BufferLayout,
}

/// A texture property's name and the binding it fills.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextureTarget {
    pub name: ParamName,
    pub set: u32,
    pub binding: u32,
}

impl StageProgram {
    /// The constant buffers the parameters describe for this stage, each with its prepared set
    /// and binding.
    ///
    /// # Errors
    /// Fails on a binding outside the binding table, or one with no layout.
    pub fn constant_buffer_targets(&self) -> Result<Vec<ConstantBufferTarget>, LayoutError> {
        let kind = self.module().kind;
        let mut targets = Vec::new();
        for wanted in self.parameters().constant_buffers() {
            if !applies(wanted.stages, kind) {
                continue;
            }
            let buffer = wanted.layout?;
            let placed = entry(&self.module().bindings, wanted.at)?;
            targets.push(ConstantBufferTarget {
                set: placed.set,
                binding: placed.binding,
                buffer,
            });
        }
        Ok(targets)
    }

    /// The texture parameters of this stage with their prepared set and binding.
    ///
    /// # Errors
    /// Fails on a binding outside the binding table.
    pub fn texture_targets(&self) -> Result<Vec<TextureTarget>, LayoutError> {
        let kind = self.module().kind;
        self.parameters()
            .textures()
            .into_iter()
            .filter(|wanted| applies(wanted.stages, kind))
            .map(|wanted| {
                let placed = entry(&self.module().bindings, wanted.at)?;
                Ok(TextureTarget {
                    name: wanted.name,
                    set: placed.set,
                    binding: placed.binding,
                })
            })
            .collect()
    }
}
