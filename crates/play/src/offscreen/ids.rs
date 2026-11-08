//! The names render-program's offscreen frame uses for the world's resources.
//!
//! The world's ids are 64-bit values of their own. Shaders and materials keep them. One texture
//! name space holds render textures and the textures the player stored, so render textures take
//! the top bit and a stored texture whose identity already has it cannot be named.

use worldrunner_render_program::ids::{MaterialId, MeshId, ShaderId, TextureId};
use worldrunner_world_simulation::graphics::{
    MaterialId as WorldMaterialId, RenderTextureId, ShaderId as WorldShaderId,
};

/// Set in the id of every render texture, never in a stored texture's.
const RENDER_TEXTURE: u64 = 1 << 63;
/// The names of the default textures: the last sixteen below the render texture bit, which no
/// stored texture takes.
const DEFAULT_TEXTURES: u64 = RENDER_TEXTURE - 16;
/// Set in the id of the scratch copy of a render texture that a test render draws into.
const PROBE_SCRATCH: u64 = 1 << 62;
/// Where the number of a test render's variant sits in a scratch copy's id, above any
/// generation a world's render texture reaches. Variant 0 is the render as declared.
const PROBE_VARIANT_SHIFT: u32 = 52;
/// Set in the id of a material made from a world material and a renderer's own render queue.
const QUEUED_MATERIAL: u64 = 1 << 63;

#[must_use]
pub fn shader(id: WorldShaderId) -> ShaderId {
    ShaderId(id.to_bits())
}

/// The material of a world material. A world material id never has the top bit.
#[must_use]
pub fn material(id: WorldMaterialId) -> MaterialId {
    MaterialId(id.to_bits() & !QUEUED_MATERIAL)
}

/// The `variant`-th material of the frame that is a world material with a render queue of a
/// renderer's own.
#[must_use]
pub fn queued_material(variant: u32) -> MaterialId {
    MaterialId(QUEUED_MATERIAL | u64::from(variant))
}

#[must_use]
pub fn render_texture(id: RenderTextureId) -> TextureId {
    TextureId(id.to_bits() | RENDER_TEXTURE)
}

/// The scratch copy a test render of the render texture `id` draws into.
#[must_use]
pub fn probe_scratch(id: TextureId) -> TextureId {
    TextureId(id.0 | PROBE_SCRATCH)
}

/// The scratch copy that the `variant`-th changed test render (counted from 0) of the render
/// texture `id` draws into, which differs from [`probe_scratch`] and from every other variant's.
#[must_use]
pub fn probe_variant(id: TextureId, variant: u8) -> TextureId {
    TextureId(probe_scratch(id).0 | ((u64::from(variant) + 1) << PROBE_VARIANT_SHIFT))
}

/// Whether `id` names a scratch copy.
#[must_use]
pub fn is_probe_scratch(id: TextureId) -> bool {
    id.0 & PROBE_SCRATCH != 0 && id.0 & RENDER_TEXTURE != 0
}

/// The name of a texture the player stored, or `None` when its identity is one a render texture
/// would share.
#[must_use]
pub fn stored_texture(identity: u64) -> Option<TextureId> {
    (identity < DEFAULT_TEXTURES).then_some(TextureId(identity))
}

/// The name of the `index`-th default texture, below 16.
#[must_use]
pub fn default_texture(index: u64) -> TextureId {
    TextureId(DEFAULT_TEXTURES + index.min(15))
}

#[must_use]
pub fn mesh(identity: u64) -> MeshId {
    MeshId(identity)
}

/// The render texture a name stands for, or `None` for a stored texture's name and a scratch
/// copy's.
#[must_use]
pub fn render_texture_of(id: TextureId) -> Option<RenderTextureId> {
    (id.0 & RENDER_TEXTURE != 0 && id.0 & PROBE_SCRATCH == 0)
        .then(|| RenderTextureId::from_bits(id.0 & !RENDER_TEXTURE))
}
