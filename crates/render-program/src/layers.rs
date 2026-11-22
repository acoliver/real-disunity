//! Property layers: where a draw finds the value of a constant buffer member or a texture.

use std::collections::{BTreeMap, BTreeSet};

use crate::ids::TextureId;
use crate::properties::PropertySet;
use crate::resources::BufferHandle;
use crate::sampler::SamplerState;

/// A texture a property names, and the sampler state it is read with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextureInput {
    pub texture: TextureId,
    /// Overrides the state of the texture. `None` leaves the texture's own state, or the fixed
    /// sampler when the texture has none.
    pub sampler: Option<SamplerState>,
}

/// The values, textures and structured buffers one source of properties supplies, by name.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PropertyLayer {
    pub values: PropertySet,
    pub textures: BTreeMap<String, TextureInput>,
    /// The texture properties the layer sets to none, as a script's `SetGlobalTexture(name, null)`
    /// does. A name is in `textures` or here, not both. A draw's texture choice ignores this set;
    /// it tells [`Layers::is_none`] that a name is set to none and not unset.
    pub none_textures: BTreeSet<String>,
    /// The runtime buffers a read-only structured buffer slot of a graphics program reads, by the
    /// property name the shader declares. The handle is the one a compute dispatch binds, made
    /// with [`ProgramRuntime::create_buffer`](crate::runtime::ProgramRuntime::create_buffer).
    pub buffers: BTreeMap<String, BufferHandle>,
    /// Why the host stands in for a texture the material, block or globals do not really hold,
    /// by property name: the host's own label for the reason. A name listed here whose texture is
    /// in `textures` reads a stand-in texture the host supplied. A name listed here with no
    /// texture explains the stand-in the frame's `undeclared_textures` provide for it.
    pub stand_ins: BTreeMap<String, String>,
}

/// The layers a draw reads, highest first. A draw reads the caller's per-draw values, the
/// renderer's property block, the material and the globals, in that order, and a caller may put
/// more layers of its own among them. The first layer that has a name wins. The crate names no
/// built-in value of Unity: the caller supplies them in a layer.
#[derive(Clone, Copy, Debug)]
pub struct Layers<'a> {
    highest_first: &'a [&'a PropertyLayer],
}

impl<'a> Layers<'a> {
    #[must_use]
    pub fn new(highest_first: &'a [&'a PropertyLayer]) -> Self {
        Self { highest_first }
    }

    /// The value sets in the order [`crate::constants::pack_layout`] reads them.
    #[must_use]
    pub fn values(&self) -> Vec<&'a PropertySet> {
        self.highest_first
            .iter()
            .map(|layer| &layer.values)
            .collect()
    }

    /// Why the texture [`texture`](Self::texture) gives for `name` is a stand-in. `None` for a real
    /// texture and for a name no layer sets a texture for.
    ///
    /// The texture is a stand-in when the layer it comes from says so. The reason is the highest
    /// layer's that explains `name`, down to that layer: a layer above it that names a texture the
    /// host does not hold says why the draw reads the stand-in of a lower layer.
    #[must_use]
    pub fn stand_in(&self, name: &str) -> Option<&'a str> {
        let mut reason = None;
        for layer in self.highest_first {
            reason = reason.or_else(|| layer.stand_ins.get(name));
            if layer.textures.contains_key(name) {
                let stand_in = layer.stand_ins.contains_key(name);
                return reason.filter(|_| stand_in).map(String::as_str);
            }
        }
        None
    }

    /// The reason the first layer that explains `name` gives for the texture it has none of.
    #[must_use]
    pub fn explanation(&self, name: &str) -> Option<&'a str> {
        self.highest_first
            .iter()
            .find_map(|layer| layer.stand_ins.get(name))
            .map(String::as_str)
    }

    /// The texture input of the first layer that names `name`.
    #[must_use]
    pub fn texture(&self, name: &str) -> Option<&'a TextureInput> {
        self.highest_first
            .iter()
            .find_map(|layer| layer.textures.get(name))
    }

    /// Whether a layer sets `name` to none and no layer holds a texture for it: the draw reads a
    /// default texture for `name`, and [`Layers::texture`] is `None` for it.
    #[must_use]
    pub fn is_none(&self, name: &str) -> bool {
        self.texture(name).is_none()
            && self
                .highest_first
                .iter()
                .any(|layer| layer.none_textures.contains(name))
    }

    /// The buffer of the first layer that names `name`.
    #[must_use]
    pub fn buffer(&self, name: &str) -> Option<BufferHandle> {
        self.highest_first
            .iter()
            .find_map(|layer| layer.buffers.get(name).copied())
    }
}
