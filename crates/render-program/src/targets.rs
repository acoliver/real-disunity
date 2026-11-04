//! Where a stage's named constant buffers and textures are bound.

use crate::binding::{BindingAddress, BindingSource, PropertyId, ResourceKind};
use crate::constants::BoundConstantLayout;
use crate::module::StageProgram;

/// A texture property and the binding it fills.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextureTarget {
    pub property: PropertyId,
    pub at: BindingAddress,
}

impl StageProgram {
    /// The constant buffers the stage declares, each with the binding it fills.
    #[must_use]
    pub fn constant_targets(&self) -> &[BoundConstantLayout] {
        &self.declarations().constants
    }

    /// The textures the stage declares by property name, in declaration order.
    #[must_use]
    pub fn texture_targets(&self) -> Vec<TextureTarget> {
        self.declarations()
            .resources
            .bindings
            .iter()
            .filter(|binding| matches!(binding.kind, ResourceKind::Texture { .. }))
            .filter_map(|binding| match &binding.source {
                BindingSource::Property(property) => Some(TextureTarget {
                    property: property.clone(),
                    at: binding.at,
                }),
                BindingSource::Unnamed => None,
            })
            .collect()
    }
}
