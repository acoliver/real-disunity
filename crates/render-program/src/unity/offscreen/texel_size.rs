//! The constants a draw gets from the textures it reads: the member `<texture>_TexelSize` of a
//! constant buffer holds (1 / width, 1 / height, width, height) of the texture bound under
//! `<texture>`, the way a blit's source has it (see `steps::source_layer`).

use crate::layers::Layers;
use crate::properties::{PropertySet, PropertyValue};
use crate::unity::buffers::BufferLayout;

use super::textures::TextureTable;

/// The `<texture>_TexelSize` members `buffer` declares, with the size of the first level of the
/// texture a layer binds under `<texture>`. A member whose texture no layer binds is left out,
/// and a draw lists this set below every layer, so a value a layer sets wins.
pub(super) fn texel_sizes(
    textures: &TextureTable,
    layers: &Layers<'_>,
    buffer: &BufferLayout,
) -> PropertySet {
    let mut sizes = PropertySet::new();
    for member in &buffer.members {
        let Ok(name) = member.name.resolve(&[]) else {
            continue;
        };
        let Some(input) = name
            .strip_suffix("_TexelSize")
            .and_then(|texture| layers.texture(texture))
        else {
            continue;
        };
        if let Ok(info) = textures.get(input.texture) {
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
