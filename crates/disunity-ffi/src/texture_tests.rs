use super::textures::*;
use crate::{
    DisunityResult,
    abi::{DisunityErrorReason, result_code},
    lock_store,
    store::create_world,
};

const FIRST: u64 = FIRST_PLAYER_TEXTURE;

fn info(identity: u64, length: u32) -> DisunityTextureInfo {
    DisunityTextureInfo {
        identity,
        format: 4,
        width: 2,
        height: 1,
        mips: 1,
        linear: 1,
        length,
        dimension: TEX_2D,
        ..Default::default()
    }
}

fn store(world: u64, identity: u64, bytes: &[u8]) -> DisunityResult {
    let description = info(identity, bytes.len() as u32);
    let pointer = if bytes.is_empty() {
        std::ptr::null()
    } else {
        bytes.as_ptr()
    };
    unsafe { disunity_texture_store(world, identity, &description, pointer, bytes.len() as u32) }
}

fn read(
    world: u64,
    identity: u64,
    capacity: usize,
) -> (DisunityResult, DisunityTextureInfo, Vec<u8>) {
    let mut out = DisunityTextureInfo::default();
    let mut bytes = vec![0xEE; capacity];
    let pointer = if capacity == 0 {
        std::ptr::null_mut()
    } else {
        bytes.as_mut_ptr()
    };
    let result =
        unsafe { disunity_texture_read(world, identity, &mut out, pointer, capacity as u32) };
    (result, out, bytes)
}

fn new_world() -> u64 {
    create_world(&mut lock_store()).unwrap()
}

#[test]
fn a_stored_texture_reads_back_with_its_description() {
    let world = new_world();
    assert_eq!(store(world, FIRST + 3, &[1, 2, 3, 4, 5, 6, 7, 8]).code, 0);
    let (result, out, bytes) = read(world, FIRST + 3, 8);
    assert_eq!(result.code, 0);
    assert_eq!(bytes, [1, 2, 3, 4, 5, 6, 7, 8]);
    assert_eq!(
        (out.identity, out.format, out.width, out.height, out.mips),
        (FIRST + 3, 4, 2, 1, 1)
    );
    assert_eq!((out.linear, out.length, out.present), (1, 8, 1));
}

#[test]
fn storing_an_identity_again_raises_its_generation_and_replaces_the_bytes() {
    let world = new_world();
    store(world, FIRST, &[1; 8]);
    assert_eq!(read(world, FIRST, 0).1.generation, 1);
    store(world, FIRST, &[9; 8]);
    store(world, FIRST + 1, &[5; 8]);
    let (_, out, bytes) = read(world, FIRST, 8);
    assert_eq!((out.generation, bytes), (2, vec![9; 8]));
    assert_eq!(read(world, FIRST + 1, 0).1.generation, 1);
}

#[test]
fn a_zero_capacity_read_reports_the_length_and_copies_nothing() {
    let world = new_world();
    store(world, FIRST, &[7; 8]);
    let (result, out, _) = read(world, FIRST, 0);
    assert_eq!((result.code, out.length, out.present), (0, 8, 1));
}

#[test]
fn a_capacity_below_the_length_is_an_error_and_leaves_the_buffer_alone() {
    let world = new_world();
    store(world, FIRST, &[7; 8]);
    let (result, _, bytes) = read(world, FIRST, 7);
    assert_ne!(result.code, 0);
    assert_eq!(bytes, vec![0xEE; 7]);
}

#[test]
fn an_unknown_identity_reads_as_not_present() {
    let world = new_world();
    let (result, out, bytes) = read(world, FIRST + 9, 4);
    assert_eq!((result.code, out.present, out.identity), (0, 0, FIRST + 9));
    assert_eq!(bytes, vec![0xEE; 4]);
}

#[test]
fn an_identity_below_the_player_range_is_rejected() {
    let world = new_world();
    assert_ne!(store(world, FIRST - 1, &[0; 8]).code, 0);
    assert_eq!(read(world, FIRST - 1, 0).1.present, 0);
}

#[test]
fn a_texture_without_size_or_mips_is_rejected() {
    let world = new_world();
    for (width, height, mips) in [(0, 1, 1), (1, 0, 1), (1, 1, 0)] {
        let description = DisunityTextureInfo {
            width,
            height,
            mips,
            ..info(FIRST, 4)
        };
        let result =
            unsafe { disunity_texture_store(world, FIRST, &description, [0u8; 4].as_ptr(), 4) };
        assert_ne!(result.code, 0, "{width}x{height} with {mips} mips");
    }
}

#[test]
fn a_description_that_disagrees_with_the_call_is_rejected() {
    let world = new_world();
    let other = info(FIRST + 1, 4);
    let by_identity = unsafe { disunity_texture_store(world, FIRST, &other, [0u8; 4].as_ptr(), 4) };
    let by_length =
        unsafe { disunity_texture_store(world, FIRST + 1, &other, [0u8; 8].as_ptr(), 8) };
    assert_ne!(by_identity.code, 0);
    assert_ne!(by_length.code, 0);
}

#[test]
fn a_payload_beyond_the_bound_is_rejected_and_null_bytes_need_zero_length() {
    let world = new_world();
    let huge = info(FIRST, u32::MAX);
    let result =
        unsafe { disunity_texture_store(world, FIRST, &huge, [0u8; 1].as_ptr(), u32::MAX) };
    assert_ne!(result.code, 0);
    let result =
        unsafe { disunity_texture_store(world, FIRST, &info(FIRST, 4), std::ptr::null(), 4) };
    assert_ne!(result.code, 0);
    assert_eq!(read(world, FIRST, 0).1.present, 0);
}

#[test]
fn textures_belong_to_their_world_and_unloading_it_drops_them() {
    let first = new_world();
    let second = new_world();
    store(first, FIRST, &[1; 8]);
    assert_eq!(read(second, FIRST, 0).1.present, 0);
    crate::store::destroy_world(&mut lock_store(), first).unwrap();
    assert_ne!(read(first, FIRST, 0).0.code, 0);
    assert_ne!(store(first, FIRST, &[1; 8]).code, 0);
    let reused = new_world();
    assert_eq!(read(reused, FIRST, 0).1.present, 0);
}

#[test]
fn a_distance_field_spread_reads_back_and_follows_each_store() {
    let world = new_world();
    let described = |spread| DisunityTextureInfo {
        distance_spread: spread,
        ..info(FIRST, 2)
    };
    for spread in [4, 0, 7] {
        let description = described(spread);
        let result =
            unsafe { disunity_texture_store(world, FIRST, &description, [1u8, 2].as_ptr(), 2) };
        assert_eq!(result.code, 0);
        assert_eq!(read(world, FIRST, 0).1.distance_spread, spread);
    }
}

#[test]
fn a_texture_stored_without_a_spread_reads_back_with_none() {
    let world = new_world();
    assert_eq!(store(world, FIRST, &[1, 2, 3, 4, 5, 6, 7, 8]).code, 0);
    assert_eq!(read(world, FIRST, 0).1.distance_spread, 0);
    assert_eq!(read(world, FIRST + 1, 0).1.distance_spread, 0);
}

const TEX_2D: u32 = 2;
const TEX_3D: u32 = 3;
const CUBE: u32 = 4;
const TEX_2D_ARRAY: u32 = 5;
/// `GraphicsFormat` of 8-bit RGBA in linear colour.
const RGBA8_LINEAR: u32 = 8;

/// A record of a texture of `dimension` with `depth` layers of 2x2 RGBA texels.
fn layered(identity: u64, dimension: u32, depth: u32) -> DisunityTextureInfo {
    DisunityTextureInfo {
        width: 2,
        height: 2,
        dimension,
        depth,
        graphics_format: RGBA8_LINEAR,
        ..info(identity, 16 * depth)
    }
}

fn store_described(world: u64, description: &DisunityTextureInfo, bytes: &[u8]) -> DisunityResult {
    let identity = description.identity;
    unsafe {
        disunity_texture_store(
            world,
            identity,
            description,
            bytes.as_ptr(),
            bytes.len() as u32,
        )
    }
}

#[test]
fn a_stored_array_reads_back_with_its_layers_dimension_and_format() {
    let world = new_world();
    let bytes: Vec<u8> = (0..64).collect();
    let array = layered(FIRST + 1, TEX_2D_ARRAY, 4);
    assert_eq!(store_described(world, &array, &bytes).code, 0);
    let (result, out, read_back) = read(world, FIRST + 1, 64);
    assert_eq!(result.code, 0);
    assert_eq!(read_back, bytes);
    assert_eq!(
        (out.depth, out.dimension, out.graphics_format),
        (4, TEX_2D_ARRAY, RGBA8_LINEAR)
    );
    assert_eq!(
        (out.width, out.height, out.length, out.present),
        (2, 2, 64, 1)
    );
}

#[test]
fn a_one_layer_array_is_an_array_and_storing_it_again_replaces_the_layers() {
    let world = new_world();
    let one = layered(FIRST, TEX_2D_ARRAY, 1);
    assert_eq!(store_described(world, &one, &[1; 16]).code, 0);
    let three = layered(FIRST, TEX_2D_ARRAY, 3);
    assert_eq!(store_described(world, &three, &[2; 48]).code, 0);
    let (_, out, bytes) = read(world, FIRST, 48);
    assert_eq!(
        (out.depth, out.dimension, out.generation),
        (3, TEX_2D_ARRAY, 2)
    );
    assert_eq!(bytes, vec![2; 48]);
}

#[test]
fn an_array_whose_length_is_not_a_whole_number_of_layers_is_refused() {
    let world = new_world();
    let array = layered(FIRST, TEX_2D_ARRAY, 4);
    for length in [0usize, 1, 63, 65] {
        let bytes = vec![0; length];
        let described = DisunityTextureInfo {
            length: length as u32,
            ..array
        };
        assert_ne!(
            store_described(world, &described, &bytes).code,
            0,
            "{length} bytes"
        );
    }
    assert_eq!(read(world, FIRST, 0).1.present, 0);
}

#[test]
fn an_array_without_a_graphics_format_is_refused() {
    let world = new_world();
    let array = DisunityTextureInfo {
        graphics_format: 0,
        ..layered(FIRST, TEX_2D_ARRAY, 2)
    };
    assert_ne!(store_described(world, &array, &[0; 32]).code, 0);
    assert_eq!(read(world, FIRST, 0).1.present, 0);
}

#[test]
fn an_array_needs_between_one_layer_and_the_depth_limit() {
    let world = new_world();
    let top = DisunityTextureInfo {
        width: 1,
        height: 1,
        graphics_format: RGBA8_LINEAR,
        dimension: TEX_2D_ARRAY,
        ..info(FIRST, 2048)
    };
    let described = |depth, length| DisunityTextureInfo {
        depth,
        length,
        ..top
    };
    assert_eq!(
        store_described(world, &described(2048, 2048), &[0; 2048]).code,
        0
    );
    assert_ne!(
        store_described(world, &described(2049, 2049), &[0; 2049]).code,
        0
    );
    assert_ne!(store_described(world, &described(0, 4), &[0; 4]).code, 0);
}

#[test]
fn a_texture_with_no_depth_reads_back_as_it_did() {
    let world = new_world();
    assert_eq!(store(world, FIRST, &[1, 2, 3, 4, 5, 6, 7, 8]).code, 0);
    let (_, plain, _) = read(world, FIRST, 8);
    assert_eq!(
        (
            plain.depth,
            plain.dimension,
            plain.graphics_format,
            plain.reserved
        ),
        (0, TEX_2D, 0, 0)
    );
    for (identity, dimension, depth) in [(FIRST + 1, TEX_2D, 0), (FIRST + 2, TEX_2D, 1)] {
        let described = DisunityTextureInfo {
            dimension,
            depth,
            ..info(identity, 8)
        };
        assert_eq!(store_described(world, &described, &[0; 8]).code, 0);
        let (_, out, _) = read(world, identity, 0);
        assert_eq!((out.dimension, out.depth), (dimension, depth));
    }
}

#[test]
fn a_cube_and_a_3d_record_read_back_unchanged() {
    let world = new_world();
    let cube = layered(FIRST, CUBE, 6);
    let volume = DisunityTextureInfo {
        height: 1,
        ..layered(FIRST + 1, TEX_3D, 5)
    };
    let volume = DisunityTextureInfo {
        length: 40,
        ..volume
    };
    assert_eq!(store_described(world, &cube, &[7; 96]).code, 0);
    assert_eq!(store_described(world, &volume, &[9; 40]).code, 0);
    let (_, out, bytes) = read(world, FIRST, 96);
    assert_eq!(
        (out.dimension, out.depth, out.graphics_format),
        (CUBE, 6, RGBA8_LINEAR)
    );
    assert_eq!(bytes, vec![7; 96]);
    let (_, out, bytes) = read(world, FIRST + 1, 40);
    assert_eq!((out.dimension, out.depth, out.height), (TEX_3D, 5, 1));
    assert_eq!(bytes, vec![9; 40]);
}

#[test]
fn a_cube_needs_six_square_faces_and_a_format() {
    let world = new_world();
    let cube = layered(FIRST, CUBE, 6);
    let bad = [
        DisunityTextureInfo {
            depth: 5,
            length: 80,
            ..cube
        },
        DisunityTextureInfo {
            height: 4,
            length: 96,
            ..cube
        },
        DisunityTextureInfo {
            graphics_format: 0,
            ..cube
        },
    ];
    for described in bad {
        let bytes = vec![0; described.length as usize];
        assert_ne!(
            store_described(world, &described, &bytes).code,
            0,
            "{described:?}"
        );
    }
    assert_eq!(read(world, FIRST, 0).1.present, 0);
}

#[test]
fn a_3d_texture_needs_a_format_and_whole_slices() {
    let world = new_world();
    let volume = layered(FIRST, TEX_3D, 4);
    let bad = [
        DisunityTextureInfo {
            graphics_format: 0,
            ..volume
        },
        DisunityTextureInfo {
            length: 63,
            ..volume
        },
        DisunityTextureInfo {
            depth: 0,
            length: 64,
            ..volume
        },
    ];
    for described in bad {
        let bytes = vec![0; described.length as usize];
        assert_ne!(
            store_described(world, &described, &bytes).code,
            0,
            "{described:?}"
        );
    }
    assert_eq!(read(world, FIRST, 0).1.present, 0);
}

#[test]
fn a_3d_texture_with_a_mip_chain_need_not_be_a_whole_number_of_slices() {
    let world = new_world();
    // 3x3x3 texels of four bytes with a 1x1x1 second level: 108 + 4 bytes, which three slices
    // do not divide. A layered texture of one level still does.
    let chain = DisunityTextureInfo {
        width: 3,
        height: 3,
        mips: 2,
        length: 112,
        ..layered(FIRST, TEX_3D, 3)
    };
    assert_eq!(store_described(world, &chain, &[5; 112]).code, 0);
    let (_, out, bytes) = read(world, FIRST, 112);
    assert_eq!((out.dimension, out.depth, out.mips), (TEX_3D, 3, 2));
    assert_eq!(bytes, vec![5; 112]);
    let single = DisunityTextureInfo { mips: 1, ..chain };
    assert_ne!(
        store_described(
            world,
            &DisunityTextureInfo {
                identity: FIRST + 1,
                ..single
            },
            &[5; 112]
        )
        .code,
        0
    );
    // An array's layers each hold a chain, so its length is whole layers whatever its levels.
    let array = DisunityTextureInfo {
        mips: 2,
        length: 112,
        ..layered(FIRST + 2, TEX_2D_ARRAY, 3)
    };
    assert_ne!(store_described(world, &array, &[5; 112]).code, 0);
}

#[test]
fn a_dimension_the_engine_does_not_define_or_a_2d_texture_with_layers_is_refused() {
    let world = new_world();
    let mut bad = vec![DisunityTextureInfo {
        depth: 2,
        length: 8,
        ..info(FIRST, 8)
    }];
    for dimension in [1, 6, 7, 99] {
        bad.push(DisunityTextureInfo {
            dimension,
            ..layered(FIRST, TEX_2D_ARRAY, 2)
        });
    }
    bad.push(DisunityTextureInfo {
        reserved: 1,
        ..info(FIRST, 8)
    });
    for described in bad {
        let bytes = vec![0; described.length as usize];
        assert_ne!(
            store_described(world, &described, &bytes).code,
            0,
            "{described:?}"
        );
    }
    assert_eq!(read(world, FIRST, 0).1.present, 0);
}

fn sampler_of(world: u64, identity: u64, known: u64) -> (DisunityResult, DisunitySampler) {
    let mut out = DisunitySampler {
        generation: known,
        ..Default::default()
    };
    let result = unsafe { disunity_texture_sampler_read(world, identity, &mut out) };
    (result, out)
}

fn write_sampler(world: u64, identity: u64, sampler: DisunitySampler) -> DisunityResult {
    unsafe { disunity_texture_sampler_write(world, identity, &sampler) }
}

fn clamped() -> DisunitySampler {
    DisunitySampler {
        wrap_u: 1,
        wrap_v: 2,
        wrap_w: 3,
        filter: 2,
        aniso_level: 8,
        mip_bias: -0.5,
        ..Default::default()
    }
}

#[test]
fn a_stored_texture_has_the_engine_default_sampler() {
    let world = new_world();
    store(world, FIRST, &[0; 8]);
    let (result, out) = sampler_of(world, FIRST, 0);
    assert_eq!(result.code, 0);
    assert_eq!((out.present, out.changed, out.generation), (1, 1, 1));
    assert_eq!((out.wrap_u, out.wrap_v, out.wrap_w), (0, 0, 0));
    assert_eq!((out.filter, out.aniso_level), (1, 1));
    assert_eq!(out.mip_bias.to_bits(), 0.0f32.to_bits());
}

#[test]
fn a_written_sampler_reads_back_and_raises_the_generation() {
    let world = new_world();
    store(world, FIRST, &[0; 8]);
    assert_eq!(write_sampler(world, FIRST, clamped()).code, 0);
    let (_, out) = sampler_of(world, FIRST, 1);
    assert_eq!((out.present, out.changed, out.generation), (1, 1, 2));
    assert_eq!((out.wrap_u, out.wrap_v, out.wrap_w), (1, 2, 3));
    assert_eq!((out.filter, out.aniso_level), (2, 8));
    assert_eq!(out.mip_bias.to_bits(), (-0.5f32).to_bits());
}

#[test]
fn writing_the_values_already_held_keeps_the_generation() {
    let world = new_world();
    store(world, FIRST, &[0; 8]);
    write_sampler(world, FIRST, clamped());
    write_sampler(world, FIRST, clamped());
    let (_, out) = sampler_of(world, FIRST, 2);
    assert_eq!((out.generation, out.changed), (2, 0));
}

#[test]
fn the_sampler_survives_storing_the_texture_again() {
    let world = new_world();
    store(world, FIRST, &[0; 8]);
    write_sampler(world, FIRST, clamped());
    store(world, FIRST, &[1; 8]);
    let (_, out) = sampler_of(world, FIRST, 0);
    assert_eq!((out.wrap_w, out.filter, out.generation), (3, 2, 2));
}

#[test]
fn a_sampler_of_an_identity_nothing_stored_reads_as_not_present_and_cannot_be_written() {
    let world = new_world();
    let (result, out) = sampler_of(world, FIRST + 5, 0);
    assert_eq!((result.code, out.present), (0, 0));
    assert_ne!(write_sampler(world, FIRST + 5, clamped()).code, 0);
    assert_eq!(sampler_of(world, FIRST + 5, 0).1.present, 0);
}

#[test]
fn a_sampler_outside_the_engine_values_is_rejected_and_changes_nothing() {
    let world = new_world();
    store(world, FIRST, &[0; 8]);
    let bad = [
        DisunitySampler {
            wrap_u: 4,
            ..clamped()
        },
        DisunitySampler {
            wrap_v: 4,
            ..clamped()
        },
        DisunitySampler {
            wrap_w: 4,
            ..clamped()
        },
        DisunitySampler {
            filter: 3,
            ..clamped()
        },
        DisunitySampler {
            aniso_level: 17,
            ..clamped()
        },
        DisunitySampler {
            mip_bias: f32::NAN,
            ..clamped()
        },
        DisunitySampler {
            mip_bias: f32::INFINITY,
            ..clamped()
        },
    ];
    for sampler in bad {
        assert_ne!(write_sampler(world, FIRST, sampler).code, 0, "{sampler:?}");
    }
    let (_, out) = sampler_of(world, FIRST, 0);
    assert_eq!((out.generation, out.wrap_u, out.filter), (1, 0, 1));
    let null = unsafe { disunity_texture_sampler_write(world, FIRST, std::ptr::null()) };
    assert_ne!(null.code, 0);
    let unread = unsafe { disunity_texture_sampler_read(world, FIRST, std::ptr::null_mut()) };
    assert_ne!(unread.code, 0);
}

#[test]
fn a_sampler_at_the_ends_of_each_engine_range_is_accepted() {
    let world = new_world();
    store(world, FIRST, &[0; 8]);
    let top = DisunitySampler {
        wrap_u: 3,
        wrap_v: 3,
        wrap_w: 3,
        filter: 2,
        aniso_level: 16,
        mip_bias: 8.0,
        ..Default::default()
    };
    let bottom = DisunitySampler {
        aniso_level: 0,
        mip_bias: -8.0,
        ..Default::default()
    };
    for sampler in [top, bottom] {
        assert_eq!(write_sampler(world, FIRST, sampler).code, 0, "{sampler:?}");
        let (_, out) = sampler_of(world, FIRST, 0);
        assert_eq!(
            (out.wrap_u, out.filter, out.aniso_level),
            (sampler.wrap_u, sampler.filter, sampler.aniso_level)
        );
    }
}

#[test]
fn a_dimension_the_engine_does_not_define_is_refused_naming_its_value() {
    let world = new_world();
    for dimension in [0, 1, 6, 7, 99] {
        let described = DisunityTextureInfo {
            dimension,
            ..info(FIRST, 8)
        };
        let result = store_described(world, &described, &[0; 8]);
        assert_eq!(result.code, result_code::ERROR, "dimension {dimension}");
        assert_eq!(result.detail, DisunityErrorReason::InvalidArgument as u64);
        let message = crate::store::last_error(world).unwrap().unwrap();
        let named = format!("dimension {dimension} is refused");
        assert!(message.contains(&named), "{message}");
        assert_eq!(read(world, FIRST, 0).1.present, 0);
    }
}
