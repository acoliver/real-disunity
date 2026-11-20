//! A `Texture2DArray` the portable texture fixture puts on a material, read back by the host.
use worldrunner_disunity_runtime::{DisunityHost, PlayerTexture, TextureDimension};

const FIRST: u64 = 1 << 40;

/// The fixture's array has three layers of two RGBA32 texels in linear colour: layer `s` holds
/// (10 s + 1, 2, 3, 255) and (10 s + 2, 2, 3, 255), and the stored copy is one record under one
/// identity with the layers one after another.
pub fn assert_player_array(host: &DisunityHost) {
    let array = (FIRST..FIRST + 64)
        .filter_map(|id| host.player_texture(id, 0).unwrap())
        .find(|texture| texture.dimension == TextureDimension::D2Array)
        .expect("the fixture's array reaches the host");
    assert_eq!(
        array,
        PlayerTexture {
            format: 4,
            width: 2,
            height: 1,
            mips: 1,
            linear: true,
            generation: 1,
            distance_spread: 0,
            depth: 3,
            dimension: TextureDimension::D2Array,
            graphics_format: 8,
            bytes: vec![
                1, 2, 3, 255, 2, 2, 3, 255, 11, 2, 3, 255, 12, 2, 3, 255, 21, 2, 3, 255, 22, 2, 3,
                255
            ],
        }
    );
}
