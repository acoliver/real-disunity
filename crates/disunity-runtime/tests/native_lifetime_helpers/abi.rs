use libloading::Library;

#[repr(C)]
struct NativeResult {
    code: i32,
    detail: u64,
}

pub(super) fn initialize_thread(path: &std::path::Path) -> Library {
    // SAFETY: uses the explicitly selected genuine image's ABI 2 exports.
    let library = unsafe { Library::new(path).unwrap() };
    let create: unsafe extern "C" fn(*mut u64) -> NativeResult =
        unsafe { *library.get(b"disunity_world_create\0").unwrap() };
    let destroy: unsafe extern "C" fn(u64) -> NativeResult =
        unsafe { *library.get(b"disunity_world_destroy\0").unwrap() };
    let mut world = 0;
    assert_eq!(unsafe { create(&mut world) }.code, 0);
    assert_eq!(unsafe { destroy(world) }.code, 0);
    library
}
