//! Property layers: where a draw finds the value of a constant buffer member or a texture.

use std::collections::BTreeMap;

use crate::ids::TextureId;
use crate::properties::PropertySet;
use crate::sampler::SamplerState;

/// A texture a property names, and the sampler state it is read with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextureInput {
    pub texture: TextureId,
    /// Overrides the state of the texture. `None` leaves the texture's own state, or the fixed
    /// sampler when the texture has none.
    pub sampler: Option<SamplerState>,
}

/// The values and textures one source of properties supplies, by name.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PropertyLayer {
    pub values: PropertySet,
    pub textures: BTreeMap<String, TextureInput>,
}

/// The four layers a draw reads, from highest to lowest: the caller's per-draw values, the
/// renderer's property block, the material and the globals. The first layer that has a name
/// wins. The crate names no built-in value of Unity: the caller supplies them in a layer.
#[derive(Clone, Copy, Debug)]
pub struct Layers<'a> {
    highest_first: [&'a PropertyLayer; 4],
}

impl<'a> Layers<'a> {
    #[must_use]
    pub fn new(
        per_draw: &'a PropertyLayer,
        block: &'a PropertyLayer,
        material: &'a PropertyLayer,
        globals: &'a PropertyLayer,
    ) -> Self {
        Self {
            highest_first: [per_draw, block, material, globals],
        }
    }

    /// The value sets in the order [`crate::constants::pack_layout`] reads them.
    #[must_use]
    pub fn values(&self) -> Vec<&'a PropertySet> {
        self.highest_first
            .iter()
            .map(|layer| &layer.values)
            .collect()
    }

    /// The texture input of the first layer that names `name`.
    #[must_use]
    pub fn texture(&self, name: &str) -> Option<&'a TextureInput> {
        self.highest_first
            .iter()
            .find_map(|layer| layer.textures.get(name))
    }
}
