#[test]
fn previous_body_layout_revision_is_rejected_before_body_access() {
    for previous in [2, 3] {
        assert_eq!(disunity_abi_check(previous).code, 4);
    }
    assert_eq!(disunity_abi_check(DISUNITY_ABI_VERSION).code, 0);
    assert_eq!(
        std::mem::size_of::<worldrunner_disunity_ffi::physics::DisunityBody2d>(),
        40
    );
    assert_eq!(
        std::mem::offset_of!(
            worldrunner_disunity_ffi::physics::DisunityBody2d,
            restitution
        ),
        36
    );
}

#[test]
fn checked_managed_bindings_fit_the_source_file_limit() {
    for source in [
        include_str!("../../../csharp/Disunity/NativeBindings.g.cs"),
        include_str!("../../../csharp/Disunity/NativeBindings.2.g.cs"),
        include_str!("../../../csharp/Disunity/NativeRecords.g.cs"),
    ] {
        assert!(source.lines().count() < 750);
    }
}

use worldrunner_disunity_ffi::{abi::*, scheduling::*};

#[test]
fn callback_records_are_owned_and_calls_release_the_store_lock() {
    unsafe {
        let (mut world, mut object, mut script) = (0, 0, 0);
        assert_eq!(disunity_world_create(&mut world).code, 0);
        assert_eq!(disunity_gameobject_create(world, &mut object).code, 0);
        assert_eq!(
            disunity_script_register(world, object, 42, &mut script).code,
            0
        );
        let mut work = DisunityAdvance::default();
        assert_eq!(disunity_object_wake(world, object, &mut work).code, 0);
        assert_eq!(work.kind, 1);
        assert_eq!(work.script, 42);
        assert_eq!(work.message, 0);
        let owned = work;
        assert_eq!(
            disunity_transform_set_position(world, object, 2.0, 3.0, 4.0).code,
            0
        );
        assert_eq!(
            disunity_world_resume(world, owned.ticket, 0, std::ptr::null(), 0, &mut work).code,
            0
        );
        assert_eq!(work.message, 1);
        assert_eq!(
            disunity_world_resume(world, work.ticket, 0, std::ptr::null(), 0, &mut work).code,
            0
        );
        assert_eq!(work.kind, 2);
        assert_eq!(
            disunity_world_resume(world, owned.ticket, 0, std::ptr::null(), 0, &mut work).code,
            3
        );
        assert_eq!(disunity_world_destroy(world).code, 0);
    }
}

#[test]
fn invalid_utf8_and_outcomes_do_not_consume_callback_tickets() {
    unsafe {
        let (mut world, mut object, mut script) = (0, 0, 0);
        disunity_world_create(&mut world);
        disunity_gameobject_create(world, &mut object);
        disunity_script_register(world, object, 7, &mut script);
        let mut work = DisunityAdvance::default();
        disunity_object_wake(world, object, &mut work);
        let ticket = work.ticket;
        assert_eq!(
            disunity_world_resume(world, ticket, 1, [255].as_ptr(), 1, &mut work).code,
            4
        );
        assert_eq!(
            disunity_world_resume(world, ticket, 9, std::ptr::null(), 0, &mut work).code,
            4
        );
        let message = "échec λ".as_bytes();
        assert_eq!(
            disunity_world_resume(
                world,
                ticket,
                1,
                message.as_ptr(),
                message.len() as u32,
                &mut work
            )
            .code,
            0
        );
        let mut result = 0;
        while work.kind == 1 && result == 0 {
            result =
                disunity_world_resume(world, work.ticket, 0, std::ptr::null(), 0, &mut work).code;
        }
        assert_eq!(result, 3);
        disunity_world_destroy(world);
    }
}

#[test]
fn revision_and_layout_are_explicit() {
    assert_eq!(std::mem::size_of::<DisunityAdvance>(), 32);
    assert_eq!(std::mem::align_of::<DisunityAdvance>(), 8);
    assert_eq!(std::mem::offset_of!(DisunityAdvance, ticket), 8);
    assert_eq!(disunity_abi_check(0).code, 4);
    assert_eq!(disunity_abi_check(DISUNITY_ABI_VERSION).code, 0);
}

#[test]
fn callback_ids_do_not_alias_between_worlds_or_after_unload() {
    unsafe {
        let (mut first, mut second, mut object, mut script) = (0, 0, 0, 0);
        let mut a = DisunityAdvance::default();
        let mut b = DisunityAdvance::default();
        disunity_world_create(&mut first);
        disunity_gameobject_create(first, &mut object);
        disunity_script_register(first, object, 1, &mut script);
        disunity_object_wake(first, object, &mut a);
        disunity_world_destroy(first);
        disunity_world_create(&mut second);
        disunity_gameobject_create(second, &mut object);
        disunity_script_register(second, object, 1, &mut script);
        disunity_object_wake(second, object, &mut b);
        assert_ne!(a.ticket, b.ticket);
        assert_eq!(
            disunity_world_resume(second, a.ticket, 0, std::ptr::null(), 0, &mut a).code,
            3
        );
        assert_eq!(
            disunity_world_resume(second, b.ticket, 0, std::ptr::null(), 0, &mut b).code,
            0
        );
        disunity_world_destroy(second);
    }
}
