//! A world material's, a property block's or the globals' values as a property layer.
//!
//! Floats, integers, vectors and matrices keep their names. A name that two kinds hold takes the
//! later kind of floats, integers, vectors, matrices, since a layer holds one value for a name.
//! A matrix keeps its 16 floats in the order the world stores them, column by column, which is
//! the order the layer copies into a constant buffer. A texture slot becomes a texture input
//! under its name. Its scale and offset are not mapped: render-program's property convention
//! names no member that carries them (see the crate documentation).

use std::collections::{BTreeMap, BTreeSet};

use worldrunner_render_program::ids::TextureId;
use worldrunner_render_program::layers::{PropertyLayer, TextureInput};
use worldrunner_render_program::properties::PropertyValue;
use worldrunner_world_simulation::graphics::{
    RenderTextureId, ShaderProperties, TextureRef, WorldRenderTexture,
};

use super::ids;

/// Which textures a frame can name, and which render textures its properties did.
pub struct Textures<'a> {
    render: &'a BTreeMap<RenderTextureId, WorldRenderTexture>,
    stored: &'a BTreeSet<u64>,
    used: BTreeSet<RenderTextureId>,
}

impl<'a> Textures<'a> {
    /// `render` are the live render textures and `stored` the identities of the textures the
    /// player stored that the frame can read.
    #[must_use]
    pub fn new(
        render: &'a BTreeMap<RenderTextureId, WorldRenderTexture>,
        stored: &'a BTreeSet<u64>,
    ) -> Self {
        Self {
            render,
            stored,
            used: BTreeSet::new(),
        }
    }

    /// The name of the texture a reference names, or `None` for no texture, a render texture
    /// that was destroyed and a stored texture the frame cannot read. A render texture named is
    /// remembered.
    pub fn resolve(&mut self, texture: &TextureRef) -> Option<TextureId> {
        match *texture {
            TextureRef::None | TextureRef::Global(_) => None,
            TextureRef::Render(id) => self.render.contains_key(&id).then(|| {
                self.used.insert(id);
                ids::render_texture(id)
            }),
            TextureRef::Asset(identity) => self
                .stored
                .contains(&identity)
                .then(|| ids::stored_texture(identity))
                .flatten(),
        }
    }

    /// The render textures named so far.
    #[must_use]
    pub fn used(&self) -> &BTreeSet<RenderTextureId> {
        &self.used
    }
}

/// The layer of `properties`.
pub fn layer(properties: &ShaderProperties, textures: &mut Textures<'_>) -> PropertyLayer {
    let mut layer = PropertyLayer::default();
    for (name, value) in &properties.floats {
        layer.values.set(name, PropertyValue::Float(*value));
    }
    for (name, value) in &properties.ints {
        layer.values.set(name, PropertyValue::Int(*value));
    }
    for (name, value) in &properties.vectors {
        layer.values.set(name, PropertyValue::Vector(*value));
    }
    for (name, value) in &properties.matrices {
        layer.values.set(name, PropertyValue::Matrix(*value));
    }
    for (name, slot) in &properties.textures {
        if let Some(texture) = textures.resolve(&slot.texture) {
            layer.textures.insert(
                name.clone(),
                TextureInput {
                    texture,
                    sampler: None,
                },
            );
        }
    }
    layer
}
