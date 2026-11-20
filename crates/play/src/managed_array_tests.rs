//! A `Texture2DArray` managed code puts on a material, as the host reads it for the renderer.
//!
//! The portable texture fixture builds a three layer array of 2x1 RGBA32 texels in linear colour
//! (layer `s` holds (10 s + 1, 2, 3, 255) and (10 s + 2, 2, 3, 255)) and gives it to a material.
//! The array's own bytes reach the host through the library, and the renderer's texture is
//! made from them with the host's own conversion. The draw that samples each layer of such a
//! source is `offscreen::tests::array_gpu`.

use std::path::PathBuf;

use worldrunner_render_program::ids::TextureId;
use worldrunner_render_program::unity::offscreen::frame::SourceDimension;

use crate::offscreen::{Assets, texture_of};
use crate::retained_driver::ui_tests::{booted_fixture_host, fixture_turn};

const FIRST: u64 = 1 << 40;

#[test]
fn an_array_a_material_holds_becomes_a_source_array_of_its_layers() {
    let _turn = fixture_turn();
    let mut host = booted_fixture_host();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    host.run_slice(&root.join("csharp/PortableFixture/texture.fixture"))
        .unwrap();
    let assets: &dyn Assets = &host;
    let stored = (FIRST..FIRST + 64)
        .filter_map(|id| assets.texture(id, 0).unwrap())
        .find(|texture| texture.depth > 0)
        .expect("the fixture's array reaches the host");
    let source = texture_of(TextureId(5), &stored, None, true).unwrap();
    assert_eq!(source.dimension, SourceDimension::D2Array(3));
    assert_eq!(source.size, [2, 1]);
    assert_eq!(source.graphics_format, 8, "RGBA8 in linear colour");
    assert_eq!(
        source.bytes,
        [
            1, 2, 3, 255, 2, 2, 3, 255, 11, 2, 3, 255, 12, 2, 3, 255, 21, 2, 3, 255, 22, 2, 3, 255
        ]
    );
    host.unload_slice().unwrap();
}
