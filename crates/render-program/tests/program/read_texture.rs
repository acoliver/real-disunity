//! Reading a texture of the runtime back by its handle, for counts of what it holds.

use worldrunner_render_program::texels::texel_stats;

use crate::common::{runtime, submit};

/// Half floats 0.0, 0.5, 2.0 and 0.0 as a 2x2 `R16Float` image.
const HEIGHTS: [u16; 4] = [0x0000, 0x3800, 0x4000, 0x0000];

#[test]
fn a_texture_read_back_by_its_handle_holds_the_bytes_it_was_made_with() {
    let mut runtime = runtime();
    let bytes: Vec<u8> = HEIGHTS.iter().flat_map(|v| v.to_le_bytes()).collect();
    let texture = runtime
        .create_texture_2d("heights", [2, 2], wgpu::TextureFormat::R16Float, &bytes)
        .unwrap();
    let (pending, index) = submit(&mut runtime, |runtime, encoder| {
        runtime.read_texture(encoder, texture)
    })
    .unwrap();
    let image = pending.resolve(index).unwrap();
    assert_eq!(image.size, [2, 2]);
    assert_eq!(image.format, wgpu::TextureFormat::R16Float);
    assert_eq!(image.bytes, bytes);
    let stats = texel_stats(&image.bytes, image.format);
    assert_eq!((stats.texels, stats.nonzero), (4, 2));
    assert_eq!(stats.range(), Some((0.0, 2.0)));
}

#[test]
fn a_handle_the_runtime_does_not_hold_is_a_named_error() {
    let mut runtime = runtime();
    let texture = runtime
        .create_texture_2d("t", [1, 1], wgpu::TextureFormat::Rgba8Unorm, &[0; 4])
        .unwrap();
    runtime.release_texture(texture).unwrap();
    let error = submit(&mut runtime, |runtime, encoder| {
        runtime.read_texture(encoder, texture)
    })
    .err()
    .unwrap();
    assert!(error.to_string().contains("texture"), "{error}");
}
