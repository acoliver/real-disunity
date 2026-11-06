//! The default GPU flush: a host that has no world yet registers the callback that every world
//! created afterwards starts with. This file holds the one test that changes the default, because
//! the default is process-wide.
use worldrunner_disunity_ffi::{abi::*, gpu_flush::*};

static CALLS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

unsafe extern "C" fn count(_: u64, user: u64, _: *mut u8, _: u32, _: *mut u32) -> DisunityResult {
    CALLS.fetch_add(user as u32, std::sync::atomic::Ordering::SeqCst);
    DisunityResult { code: 0, detail: 0 }
}

fn new_world() -> u64 {
    let mut world = 0;
    unsafe { assert_eq!(disunity_world_create(&mut world).code, 0) };
    world
}

fn calls() -> u32 {
    CALLS.load(std::sync::atomic::Ordering::SeqCst)
}

#[test]
fn worlds_created_after_the_default_is_registered_start_with_it() {
    let before = new_world();
    unsafe { assert_eq!(disunity_gpu_flush_register_default(Some(count), 5).code, 0) };
    assert_eq!(disunity_gpu_flush(before).code, 0);
    assert_eq!(
        calls(),
        0,
        "a world made before the default does not get it"
    );

    let after = new_world();
    assert_eq!(disunity_gpu_flush(after).code, 0);
    assert_eq!(
        calls(),
        5,
        "a world made after it runs the callback with the default's word"
    );

    unsafe { assert_eq!(disunity_gpu_flush_register(after, None, 0).code, 0) };
    assert_eq!(disunity_gpu_flush(after).code, 0);
    assert_eq!(calls(), 5, "a world can drop the default for itself");

    unsafe { assert_eq!(disunity_gpu_flush_register_default(None, 0).code, 0) };
    let last = new_world();
    assert_eq!(disunity_gpu_flush(last).code, 0);
    assert_eq!(
        calls(),
        5,
        "clearing the default leaves later worlds without a callback"
    );
    let kept = new_world();
    unsafe { assert_eq!(disunity_gpu_flush_register(kept, Some(count), 2).code, 0) };
    assert_eq!(disunity_gpu_flush(kept).code, 0);
    assert_eq!(
        calls(),
        7,
        "a callback registered on the world itself still works"
    );
}
