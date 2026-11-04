use super::textures::*;
use crate::{DisunityResult, lock_store, store::create_world};

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
