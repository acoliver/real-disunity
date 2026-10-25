use worldrunner_disunity_ffi::{abi::*, scheduling::*};

#[test]
fn scheduler_errors_preserve_reason_and_diagnostic() {
    unsafe {
        let mut world = 0;
        assert_eq!(disunity_world_create(&mut world).code, 0);
        let mut work = DisunityAdvance::default();
        let invalid_time = disunity_world_advance(world, 1, 0, &mut work);
        assert_eq!(invalid_time.detail, 2, "invalid time reason");
        let wrong = disunity_world_resume(world, 42, 0, std::ptr::null(), 0, &mut work);
        assert_eq!(wrong.detail, 3, "wrong callback reason");
        assert_eq!(disunity_world_destroy(world).code, 0);
    }
}
