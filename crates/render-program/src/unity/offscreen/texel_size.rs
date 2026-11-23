//! The constants a draw gets from the textures it reads: the member `<texture>_TexelSize` of a
//! constant buffer holds (1 / width, 1 / height, width, height) of the texture bound under
//! `<texture>`. A blit's source is one of the textures a layer binds, so it has its texel size from
//! this fill like any other. The texture is the one the draw binds for `<texture>`: the first layer's
//! (`Layers::texture`), and for a name a layer sets to none the frame's 2D default, which the binder
//! reads for a texture no layer holds.

use crate::layers::Layers;
use crate::properties::{PropertySet, PropertyValue};
use crate::unity::buffers::BufferLayout;

use super::frame::UndeclaredTextures;
use super::textures::TextureTable;

/// The `<texture>_TexelSize` members `buffer` declares, with the size of the first level of the
/// texture the draw binds under `<texture>`: the one a layer holds, or the 2D default when a layer
/// sets it to none. A member whose texture no layer names is left out, and a draw lists this set
/// below every layer, so a value a layer sets wins.
pub(super) fn texel_sizes(
    textures: &TextureTable,
    stand_in: UndeclaredTextures,
    layers: &Layers<'_>,
    buffer: &BufferLayout,
) -> PropertySet {
    let mut sizes = PropertySet::new();
    for member in &buffer.members {
        let Ok(name) = member.name.resolve(&[]) else {
            continue;
        };
        let Some(texture) = name.strip_suffix("_TexelSize").and_then(|texture| {
            layers
                .texture(texture)
                .map(|input| input.texture)
                .or_else(|| layers.is_none(texture).then_some(stand_in.d2).flatten())
        }) else {
            continue;
        };
        if let Ok(info) = textures.get(texture) {
            let [width, height] = info.size.map(texels);
            sizes.set(
                name,
                PropertyValue::Vector([1.0 / width, 1.0 / height, width, height]),
            );
        }
    }
    sizes
}

/// A side in texels as a float. Sides above 65535 saturate, which no render texture reaches.
fn texels(side: u32) -> f32 {
    f32::from(u16::try_from(side).unwrap_or(u16::MAX))
}
