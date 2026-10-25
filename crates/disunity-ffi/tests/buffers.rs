use worldrunner_disunity_ffi::{abi::*, buffers::*, scheduling::*};

#[test]
fn diagnostics_have_explicit_capacity_and_survive_world_unload_until_free() {
    unsafe {
        let mut world = 0;
        disunity_world_create(&mut world);
        let mut work = DisunityAdvance::default();
        assert_eq!(disunity_world_advance(world, 1, 0, &mut work).code, 3);
        let mut buffer = DisunityBuffer::default();
        assert_eq!(disunity_error_take(world, &mut buffer).code, 0);
        assert!(buffer.length > 0);
        disunity_world_destroy(world);
        let mut written = 0;
        assert_eq!(
            disunity_buffer_copy(buffer.handle, std::ptr::null_mut(), 0, &mut written).code,
            2
        );
        assert_eq!(written, buffer.length);
        let mut bytes = vec![0; written as usize];
        assert_eq!(
            disunity_buffer_copy(buffer.handle, bytes.as_mut_ptr(), written - 1, &mut written).code,
            2
        );
        assert!(bytes.iter().all(|&b| b == 0));
        assert_eq!(
            disunity_buffer_copy(buffer.handle, bytes.as_mut_ptr(), written, &mut written).code,
            0
        );
        assert_eq!(
            std::str::from_utf8(&bytes).unwrap(),
            "time is outside rational bounds"
        );
        assert_eq!(disunity_buffer_free(buffer.handle).code, 0);
        assert_eq!(disunity_buffer_free(buffer.handle).code, 1);
    }
}

#[test]
fn buffer_and_clock_layouts_match_generated_managed_records() {
    assert_eq!(std::mem::size_of::<DisunityBuffer>(), 16);
    assert_eq!(std::mem::align_of::<DisunityBuffer>(), 8);
    assert_eq!(std::mem::offset_of!(DisunityBuffer, length), 8);
    assert_eq!(std::mem::size_of::<DisunityClock>(), 24);
    assert_eq!(std::mem::offset_of!(DisunityClock, time), 8);
    assert_eq!(std::mem::offset_of!(DisunityClock, frame), 16);
}
