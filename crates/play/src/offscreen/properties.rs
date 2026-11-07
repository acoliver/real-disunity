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
use worldrunner_render_program::properties::{PropertySet, PropertyValue};
use worldrunner_render_program::sampler::SamplerState;
use worldrunner_world_simulation::graphics::{
    RenderTextureId, ShaderProperties, TextureRef, WorldRenderTexture,
};

use super::assets::sampler_of_world;
use super::ids;

/// Why a texture property is counted that was given its shader's default.
const DEFAULT_READ: &str = "no texture set, the shader's default read";

/// Which textures a frame can name, and which render textures its properties did.
pub struct Textures<'a> {
    render: &'a BTreeMap<RenderTextureId, WorldRenderTexture>,
    stored: &'a BTreeSet<u64>,
    used: BTreeSet<RenderTextureId>,
    /// Properties that named no texture the frame can read, by `name: reason`.
    unresolved: BTreeMap<String, usize>,
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
            unresolved: BTreeMap::new(),
        }
    }

    /// The name of the texture a reference names and the sampler state it is read with, or
    /// `None` for no texture, a render texture that was destroyed and a stored texture the frame
    /// cannot read. A render texture named is remembered and brings its own sampler; a stored
    /// texture's sampler is the one it was uploaded with.
    pub fn resolve(&mut self, texture: &TextureRef) -> Option<(TextureId, Option<SamplerState>)> {
        match *texture {
            TextureRef::None | TextureRef::Global(_) => None,
            TextureRef::Render(id) => self.render.get(&id).map(|description| {
                self.used.insert(id);
                (
                    ids::render_texture(id),
                    Some(sampler_of_world(&description.sampler)),
                )
            }),
            TextureRef::Asset(identity) => self
                .stored
                .contains(&identity)
                .then(|| ids::stored_texture(identity))
                .flatten()
                .map(|id| (id, None)),
        }
    }

    /// Why `texture` resolves to nothing.
    fn why_unresolved(texture: &TextureRef) -> &'static str {
        match texture {
            TextureRef::None => "no texture set",
            TextureRef::Global(_) => "a global the host does not resolve",
            TextureRef::Render(_) => "a render texture that no longer exists",
            TextureRef::Asset(identity) if ids::stored_texture(*identity).is_none() => {
                "an identity a render texture would share"
            }
            TextureRef::Asset(_) => "asset not readable by the host",
        }
    }

    /// The properties that named no texture the frame can read, by `name: reason`.
    #[must_use]
    pub fn unresolved(&self) -> &BTreeMap<String, usize> {
        &self.unresolved
    }

    /// The render textures named so far.
    #[must_use]
    pub fn used(&self) -> &BTreeSet<RenderTextureId> {
        &self.used
    }
}

/// The values of `properties` by name. A name that two kinds hold takes the later kind of
/// floats, integers, vectors, matrices.
#[must_use]
pub fn values(properties: &ShaderProperties) -> PropertySet {
    let mut set = PropertySet::new();
    for (name, value) in &properties.floats {
        set.set(name, PropertyValue::Float(*value));
    }
    for (name, value) in &properties.ints {
        set.set(name, PropertyValue::Int(*value));
    }
    for (name, value) in &properties.vectors {
        set.set(name, PropertyValue::Vector(*value));
    }
    for (name, value) in &properties.matrices {
        set.set(name, PropertyValue::Matrix(*value));
    }
    set
}

/// The layer of `properties`.
pub fn layer(properties: &ShaderProperties, textures: &mut Textures<'_>) -> PropertyLayer {
    layer_with_defaults(properties, textures, &[])
}

/// The layer of a material's `properties`, where a texture property that `defaults` names (the
/// shader's declared default texture) and that the material sets to no texture, or lists no
/// slot for, reads the default. A property set to a texture the frame cannot read keeps no
/// texture, because the default is not what the material holds.
pub fn layer_with_defaults(
    properties: &ShaderProperties,
    textures: &mut Textures<'_>,
    defaults: &[(String, TextureId)],
) -> PropertyLayer {
    let mut layer = PropertyLayer {
        values: values(properties),
        ..PropertyLayer::default()
    };
    let default_of = |name: &str| defaults.iter().find(|(held, _)| held == name);
    for (name, slot) in &properties.textures {
        if let Some((texture, sampler)) = textures.resolve(&slot.texture) {
            layer
                .textures
                .insert(name.clone(), TextureInput { texture, sampler });
        } else if let (TextureRef::None, Some((_, texture))) = (&slot.texture, default_of(name)) {
            layer.textures.insert(
                name.clone(),
                TextureInput {
                    texture: *texture,
                    sampler: None,
                },
            );
            *textures
                .unresolved
                .entry(format!("{name}: {DEFAULT_READ}"))
                .or_default() += 1;
        } else {
            let reason = Textures::why_unresolved(&slot.texture);
            *textures
                .unresolved
                .entry(format!("{name}: {reason}"))
                .or_default() += 1;
        }
    }
    for (name, texture) in defaults {
        if !properties.textures.iter().any(|(held, _)| held == name) {
            layer.textures.insert(
                name.clone(),
                TextureInput {
                    texture: *texture,
                    sampler: None,
                },
            );
            *textures
                .unresolved
                .entry(format!("{name}: {DEFAULT_READ}"))
                .or_default() += 1;
        }
    }
    layer
}
