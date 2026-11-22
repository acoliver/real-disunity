//! A world material's, a property block's or the globals' values as a property layer.
//!
//! Floats, integers, vectors, matrices and the arrays of vectors and of matrices keep their names.
//! A name that two kinds hold takes the later kind of floats, integers, vectors, matrices, vector
//! arrays, matrix arrays, since a layer holds one value for a name. A matrix keeps its 16 floats
//! in the order the world stores them, column by column, which is the order the layer copies into
//! a constant buffer. An array keeps its elements in order; the pass or kernel that declares fewer
//! elements than it holds refuses it when its constants are packed. A texture slot becomes a
//! texture input under its name. Its scale and offset reach a shader as the vector `<name>_ST` of
//! the fallback layer ([`fallback_layer`]), where they sit below anything a property block or the
//! material sets. A structured buffer a material binds to a property name becomes the buffer
//! input under that name when the host has it on the GPU ([`bind_buffers`]).

use std::collections::{BTreeMap, BTreeSet};

use worldrunner_render_program::ids::TextureId;
use worldrunner_render_program::layers::{PropertyLayer, TextureInput};
use worldrunner_render_program::properties::{PropertySet, PropertyValue};
use worldrunner_render_program::runtime::BufferHandle;
use worldrunner_render_program::sampler::SamplerState;
use worldrunner_world_simulation::graphics::{
    ComputeBufferId, PropertyValue as WorldValue, RenderTextureId, ShaderProperties, TextureRef,
    WorldRenderTexture,
};

use super::assets::sampler_of_world;
use super::defaults::ShaderFill;
use super::ids;
use super::stand_ins::StandInReason;

/// Which textures a frame can name, and which render textures its properties did.
pub struct Textures<'a> {
    render: &'a BTreeMap<RenderTextureId, WorldRenderTexture>,
    stored: &'a BTreeSet<u64>,
    /// The stored textures the host could not upload, by why.
    refused: &'a BTreeMap<u64, StandInReason>,
    used: BTreeSet<RenderTextureId>,
    /// Properties that named no texture the frame can read, by `name: reason`.
    unresolved: BTreeMap<String, usize>,
    /// The values the fallback layers were given, by the rule that gave them.
    filled: BTreeMap<&'static str, usize>,
}

impl<'a> Textures<'a> {
    /// `render` are the live render textures, `stored` the identities of the textures the player
    /// stored that the frame can read, and `refused` the stored textures the host could not
    /// upload and why, so that a property that names one says so.
    #[must_use]
    pub fn new(
        render: &'a BTreeMap<RenderTextureId, WorldRenderTexture>,
        stored: &'a BTreeSet<u64>,
        refused: &'a BTreeMap<u64, StandInReason>,
    ) -> Self {
        Self {
            render,
            stored,
            refused,
            used: BTreeSet::new(),
            unresolved: BTreeMap::new(),
            filled: BTreeMap::new(),
        }
    }

    /// The name of the texture a reference names and the sampler state it is read with, or
    /// `None` for no texture, a render texture that was destroyed and a stored texture the frame
    /// cannot read. A render texture named is remembered and brings its own sampler; a stored
    /// texture's sampler is the one it was uploaded with.
    pub fn resolve(&mut self, texture: &TextureRef) -> Option<(TextureId, Option<SamplerState>)> {
        match *texture {
            TextureRef::None
            | TextureRef::Unsent
            | TextureRef::Global(_)
            | TextureRef::Temporary(_) => None,
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
            TextureRef::Unsent => "a texture the engine did not send",
            TextureRef::Global(_) => "a global the host does not resolve",
            TextureRef::Temporary(_) => "a temporary render texture outside recorded work",
            TextureRef::Render(_) => "a render texture that no longer exists",
            TextureRef::Asset(identity) if ids::stored_texture(*identity).is_none() => {
                "an identity a render texture would share"
            }
            TextureRef::Asset(_) => "asset not readable by the host",
        }
    }

    /// Why a draw that reads a stand-in for `texture`, which resolves to nothing, reads one: the
    /// reference names a texture the host does not hold. `None` for no texture at all, which the
    /// game's data leaves null.
    fn why_missing(&self, texture: &TextureRef) -> Option<StandInReason> {
        match texture {
            TextureRef::None => None,
            TextureRef::Unsent => Some(StandInReason::NotLoaded),
            TextureRef::Asset(identity) => Some(
                self.refused
                    .get(identity)
                    .copied()
                    .unwrap_or(StandInReason::ExistsNotLoaded),
            ),
            TextureRef::Global(_) | TextureRef::Temporary(_) | TextureRef::Render(_) => {
                Some(StandInReason::ExistsNotLoaded)
            }
        }
    }

    /// Counts a property under `reason`, the text of why it names no texture the frame can read.
    fn count_unresolved(&mut self, name: &str, reason: &str) {
        *self
            .unresolved
            .entry(format!("{name}: {reason}"))
            .or_default() += 1;
    }

    /// The properties that named no texture the frame can read, by `name: reason`.
    #[must_use]
    pub fn unresolved(&self) -> &BTreeMap<String, usize> {
        &self.unresolved
    }

    /// The values the fallback layers were given so far, by the rule that gave them.
    #[must_use]
    pub fn filled(&self) -> &BTreeMap<&'static str, usize> {
        &self.filled
    }

    /// The render textures named so far.
    #[must_use]
    pub fn used(&self) -> &BTreeSet<RenderTextureId> {
        &self.used
    }
}

/// The values of `properties` by name. A name that two kinds hold takes the later kind of
/// floats, integers, vectors, matrices, vector arrays, matrix arrays.
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
    for (name, value) in &properties.vector_arrays {
        set.set(name, PropertyValue::VectorArray(value.clone()));
    }
    for (name, value) in &properties.matrix_arrays {
        set.set(name, PropertyValue::MatrixArray(value.clone()));
    }
    set
}

/// Sets `value` in the global `layer`: a texture the frame can read replaces the one of its name
/// and one it cannot read takes the name away, since a global names one texture at most. A null
/// texture leaves the name set to none.
pub fn set_value(layer: &mut PropertyLayer, value: &WorldValue, textures: &mut Textures<'_>) {
    match value {
        WorldValue::Float(name, held) => layer.values.set(name, PropertyValue::Float(*held)),
        WorldValue::Int(name, held) => layer.values.set(name, PropertyValue::Int(*held)),
        WorldValue::Vector(name, held) => layer.values.set(name, PropertyValue::Vector(*held)),
        WorldValue::Matrix(name, held) => layer.values.set(name, PropertyValue::Matrix(*held)),
        WorldValue::VectorArray(name, held) => {
            layer
                .values
                .set(name, PropertyValue::VectorArray(held.clone()));
        }
        WorldValue::MatrixArray(name, held) => {
            layer
                .values
                .set(name, PropertyValue::MatrixArray(held.clone()));
        }
        WorldValue::Texture(name, slot) => {
            layer.stand_ins.remove(name);
            layer.none_textures.remove(name);
            if let Some((texture, sampler)) = textures.resolve(&slot.texture) {
                layer
                    .textures
                    .insert(name.clone(), TextureInput { texture, sampler });
            } else {
                layer.textures.remove(name);
                let why = textures.why_missing(&slot.texture);
                if why.is_none() {
                    layer.none_textures.insert(name.clone());
                }
                explain(layer, name, why);
            }
        }
    }
}

/// Says in `layer` why the property `name` has no texture, when the reference named one.
fn explain(layer: &mut PropertyLayer, name: &str, why: Option<StandInReason>) {
    if let Some(reason) = why {
        layer
            .stand_ins
            .insert(name.to_owned(), reason.label().to_owned());
    }
}

/// Gives the property `name` of `layer` the stand-in `texture`, for `reason`.
fn stand_in(layer: &mut PropertyLayer, name: &str, texture: TextureId, reason: StandInReason) {
    layer.textures.insert(
        name.to_owned(),
        TextureInput {
            texture,
            sampler: None,
        },
    );
    layer
        .stand_ins
        .insert(name.to_owned(), reason.label().to_owned());
}

/// The layer of `properties`.
pub fn layer(properties: &ShaderProperties, textures: &mut Textures<'_>) -> PropertyLayer {
    layer_with_defaults(properties, textures, &[], None)
}

/// The layers of a renderer's slot blocks, by slot index. A block of a slot at or past `slots`, the
/// number of slots the renderer draws, is left out, as no draw reads its textures.
pub fn slot_layers(
    blocks: &BTreeMap<u32, ShaderProperties>,
    slots: usize,
    textures: &mut Textures<'_>,
) -> BTreeMap<usize, PropertyLayer> {
    blocks
        .iter()
        .filter_map(|(slot, block)| {
            let slot = usize::try_from(*slot).ok().filter(|slot| *slot < slots)?;
            Some((slot, layer(block, textures)))
        })
        .collect()
}

/// Binds each of a material's `bindings` (property name, buffer) that `uploaded` holds to its
/// name in `layer`. A buffer the host does not hold, as one destroyed after it was bound, is left
/// unbound, so a draw that reads it is left out and counted by the name.
pub fn bind_buffers(
    layer: &mut PropertyLayer,
    bindings: &[(String, ComputeBufferId)],
    uploaded: &BTreeMap<ComputeBufferId, BufferHandle>,
) {
    for (name, id) in bindings {
        if let Some(handle) = uploaded.get(id) {
            layer.buffers.insert(name.clone(), *handle);
        }
    }
}

/// The texture a property that no layer sets and the shader does not declare reads: the grey
/// default texture, when the host registered it, and the names the shader declares.
#[derive(Clone, Copy, Debug)]
pub struct Undeclared<'a> {
    pub texture: TextureId,
    pub declared: &'a [String],
}

/// Why a slot that names no readable texture reads the shader's declared default: the game's data
/// leaves it null, or names a texture the engine did not send.
fn default_reason(texture: &TextureRef) -> StandInReason {
    match texture {
        TextureRef::Unsent => StandInReason::NotLoaded,
        _ => StandInReason::DeclaredDefault,
    }
}

/// The layer of a material's `properties`, where a texture property that `defaults` names (the
/// shader's declared default texture) and that the material sets to no texture, or lists no
/// slot for, reads the default. A property set to a texture the frame cannot read keeps no
/// texture, because the default is not what the material holds.
pub fn layer_with_defaults(
    properties: &ShaderProperties,
    textures: &mut Textures<'_>,
    defaults: &[(String, TextureId)],
    undeclared: Option<Undeclared<'_>>,
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
        } else if slot.texture.is_unbound()
            && let Some((_, texture)) = default_of(name)
        {
            stand_in(&mut layer, name, *texture, default_reason(&slot.texture));
        } else if slot.texture.is_unbound()
            && let Some(rule) = undeclared
            && !rule.declared.iter().any(|held| held == name)
        {
            stand_in(&mut layer, name, rule.texture, StandInReason::Undeclared);
        } else {
            textures.count_unresolved(name, Textures::why_unresolved(&slot.texture));
            explain(&mut layer, name, textures.why_missing(&slot.texture));
        }
    }
    for (name, texture) in defaults {
        if !properties.textures.iter().any(|(held, _)| held == name) {
            stand_in(&mut layer, name, *texture, StandInReason::Unset);
        }
    }
    layer
}

/// The rule that gives `<name>_ST` the scale and offset of a texture slot the material lists.
pub const RULE_SLOT_SCALE_OFFSET: &str = "the material's texture scale and offset";
/// The rule that gives `<name>_ST` the world's default scale and offset to a texture property
/// the shader declares and the material lists no slot for.
pub const RULE_DEFAULT_SCALE_OFFSET: &str = "the default scale and offset of an unlisted texture";
/// The rules that give a property the default its shader declares.
pub const RULE_DEFAULT_FLOAT: &str = "the shader's default float";
pub const RULE_DEFAULT_VECTOR: &str = "the shader's default vector";
pub const RULE_DEFAULT_INT: &str = "the shader's default integer";

/// The values a material has no property for, as a layer below the material's own:
///
/// * `<name>_ST` of every texture the material lists or its shader declares, as the four floats
///   scale x, scale y, offset x, offset y (the world's default scale of one and offset of zero for
///   a texture the material lists no slot for);
/// * the default the shader declares for each colour, vector, float, range and integer property.
///
/// A name the material sets itself, as any kind of value, is left to it. A renderer's property
/// block sits above the material, so what it sets wins over the fallback too.
pub fn fallback_layer(
    properties: &ShaderProperties,
    fill: &ShaderFill,
    textures: &mut Textures<'_>,
) -> PropertyLayer {
    let held = values(properties);
    let mut layer = PropertyLayer::default();
    let mut note = |rule: &'static str| *textures.filled.entry(rule).or_default() += 1;
    let slots = properties
        .textures
        .iter()
        .map(|(name, slot)| (name.as_str(), Some(slot)))
        .chain(
            fill.textures
                .iter()
                .filter(|name| !properties.textures.iter().any(|(held, _)| held == *name))
                .map(|name| (name.as_str(), None)),
        );
    for (name, slot) in slots {
        let st = format!("{name}_ST");
        if held.get(&st).is_some() {
            continue;
        }
        let (scale, offset) = slot.map_or(([1.0; 2], [0.0; 2]), |slot| (slot.scale, slot.offset));
        layer.values.set(
            &st,
            PropertyValue::Vector([scale[0], scale[1], offset[0], offset[1]]),
        );
        note(if slot.is_some() {
            RULE_SLOT_SCALE_OFFSET
        } else {
            RULE_DEFAULT_SCALE_OFFSET
        });
    }
    for (name, value) in &fill.values {
        if held.get(name).is_some() || layer.values.get(name).is_some() {
            continue;
        }
        layer.values.set(name, value.clone());
        note(match value {
            PropertyValue::Float(_) => RULE_DEFAULT_FLOAT,
            PropertyValue::Int(_) => RULE_DEFAULT_INT,
            _ => RULE_DEFAULT_VECTOR,
        });
    }
    layer
}

/// Splits the values of `fallback` named in `host_filled` out into a layer of their own, and
/// gives back the rest of `fallback` first. The frame's globals hold what the host fills for
/// those names, so their declared defaults belong below the globals and not above them.
#[must_use]
pub fn split_below_globals(
    fallback: PropertyLayer,
    host_filled: &[&str],
) -> (PropertyLayer, PropertyLayer) {
    let mut above = PropertyLayer {
        textures: fallback.textures,
        ..PropertyLayer::default()
    };
    let mut below = PropertyLayer::default();
    for (name, value) in fallback.values.iter() {
        let target = if host_filled.contains(&name) {
            &mut below
        } else {
            &mut above
        };
        target.values.set(name, value.clone());
    }
    (above, below)
}
