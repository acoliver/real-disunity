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
