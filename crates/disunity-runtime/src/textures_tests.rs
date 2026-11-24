//! The host's read of a stored texture's record, through a library that answers with a fixed record.
use crate::{
    HostError,
    native::{DisunityResult, DisunityTextureInfo},
    textures::{PlayerTexture, TextureDimension, read_texture},
};

const OK: DisunityResult = DisunityResult { code: 0, detail: 0 };

/// A library that has one stored texture: `dimension` code, `depth` layers and a graphics format
/// 8, as the ABI describes a record, with a 2x2 RGBA layer's bytes for each layer.
fn answer(
    dimension: u32,
    depth: u32,
    info: *mut DisunityTextureInfo,
    bytes: *mut u8,
    capacity: u32,
) {
    let length = 16 * depth.max(1);
    unsafe {
        *info = DisunityTextureInfo {
            identity: 1 << 40,
            generation: 3,
            format: 4,
            width: 2,
            height: 2,
            mips: 1,
            linear: 1,
            length,
            present: 1,
            depth,
            graphics_format: 8,
            dimension,
            ..Default::default()
        };
        if capacity >= length {
            for index in 0..length {
                *bytes.add(index as usize) = index as u8;
            }
        }
    }
}

unsafe extern "C" fn stored_array(
    _: u64,
    _: u64,
    info: *mut DisunityTextureInfo,
    bytes: *mut u8,
    capacity: u32,
) -> DisunityResult {
    answer(5, 3, info, bytes, capacity);
    OK
}

unsafe extern "C" fn stored_plain(
    _: u64,
    _: u64,
    info: *mut DisunityTextureInfo,
    bytes: *mut u8,
    capacity: u32,
) -> DisunityResult {
    answer(2, 0, info, bytes, capacity);
    OK
}

unsafe extern "C" fn stored_unstated(
    _: u64,
    _: u64,
    info: *mut DisunityTextureInfo,
    bytes: *mut u8,
    capacity: u32,
) -> DisunityResult {
    answer(0, 0, info, bytes, capacity);
    OK
}

unsafe extern "C" fn stored_cube(
    _: u64,
    _: u64,
    info: *mut DisunityTextureInfo,
    bytes: *mut u8,
    capacity: u32,
) -> DisunityResult {
    answer(4, 6, info, bytes, capacity);
    OK
}

unsafe extern "C" fn stored_volume(
    _: u64,
    _: u64,
    info: *mut DisunityTextureInfo,
    bytes: *mut u8,
    capacity: u32,
) -> DisunityResult {
    answer(3, 2, info, bytes, capacity);
    OK
}

unsafe extern "C" fn stored_unknown(
    _: u64,
    _: u64,
    info: *mut DisunityTextureInfo,
    bytes: *mut u8,
    capacity: u32,
) -> DisunityResult {
    answer(9, 2, info, bytes, capacity);
    OK
}

fn lookup(read: crate::textures::Read) -> Result<Option<PlayerTexture>, HostError> {
    read_texture(1, read, 1 << 40, 0, |result| {
        assert_eq!(result.code, 0);
        Ok(())
    })
}

#[test]
fn the_players_texture_lookup_returns_the_layers_dimension_and_format_of_an_array() {
    let texture = lookup(stored_array).unwrap().unwrap();
    assert_eq!(texture.depth, 3);
    assert_eq!(texture.dimension, TextureDimension::D2Array);
    assert_eq!(texture.graphics_format, 8);
    assert_eq!(
        (texture.width, texture.height, texture.generation),
        (2, 2, 3)
    );
    assert_eq!(texture.bytes, (0..48).collect::<Vec<u8>>());
}

#[test]
fn a_2d_record_is_a_plain_2d_texture() {
    let texture = lookup(stored_plain).unwrap().unwrap();
    assert_eq!(
        (texture.depth, texture.dimension, texture.graphics_format),
        (0, TextureDimension::D2, 8)
    );
}

#[test]
fn a_cube_and_a_volume_keep_their_own_dimension_and_layers() {
    let cube = lookup(stored_cube).unwrap().unwrap();
    assert_eq!((cube.dimension, cube.depth), (TextureDimension::Cube, 6));
    let volume = lookup(stored_volume).unwrap().unwrap();
    assert_eq!((volume.dimension, volume.depth), (TextureDimension::D3, 2));
}

#[test]
fn a_dimension_code_the_host_does_not_know_is_an_error_naming_the_texture() {
    let error = lookup(stored_unknown).unwrap_err();
    assert!(
        matches!(
            error,
            HostError::TextureShape {
                identity,
                dimension: 9
            } if identity == 1 << 40
        ),
        "{error:?}"
    );
}

#[test]
fn a_record_that_states_no_dimension_is_an_error_naming_the_texture() {
    let error = lookup(stored_unstated).unwrap_err();
    assert!(
        matches!(
            error,
            HostError::TextureShape {
                identity,
                dimension: 0
            } if identity == 1 << 40
        ),
        "{error:?}"
    );
}
