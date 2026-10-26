use crate::{abi::*, callbacks::*, scheduling::*, store::lock_store};

fn world() -> u64 {
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
        drain(world, work);
        world
    }
}
fn drain(world: u64, mut work: DisunityAdvance) {
    while work.kind == 1 {
        assert_eq!(
            unsafe { disunity_world_resume(world, work.ticket, 0, std::ptr::null(), 0, &mut work) }
                .code,
            0
        );
    }
}

#[test]
fn completion_releases_native_coroutine_bindings_and_payloads() {
    let world = world();
    unsafe {
        let mut id = 0;
        assert_eq!(
            disunity_coroutine_start(world, 42, 0, 0, 1, &mut id).code,
            0
        );
        let mut work = DisunityAdvance::default();
        assert_eq!(disunity_world_advance(world, 1, 100, &mut work).code, 0);
        let mut moved = false;
        while work.kind == 1 {
            let mut payload = DisunityCallback::default();
            assert_eq!(disunity_callback_read(world, 42, &mut payload).code, 0);
            if work.message == 8 {
                assert_eq!(payload.routine, id);
                moved = true;
            }
            assert_eq!(
                disunity_world_resume(world, work.ticket, 0, std::ptr::null(), 0, &mut work).code,
                0
            );
        }
        assert!(moved);
        {
            let store = lock_store();
            let session = store.session(world).unwrap();
            assert!(session.dispatch.routines.is_empty());
            assert!(session.dispatch.payloads.is_empty());
            assert!(session.dispatch.waits.is_empty());
            assert_eq!(session.native.coroutine_count(), 0);
        }
        assert_eq!(disunity_world_destroy(world).code, 0);
    }
}

#[test]
fn cancellations_transfer_once_and_invalid_waits_do_not_allocate() {
    let world = world();
    unsafe {
        let mut id = 999;
        assert_ne!(
            disunity_coroutine_start(world, 42, 9, 0, 1, &mut id).code,
            0
        );
        assert_eq!(id, 999);
        assert_ne!(
            disunity_coroutine_start(world, 42, 2, 1, 0, &mut id).code,
            0
        );
        assert_eq!(
            disunity_coroutine_start(world, 42, 2, 1, 1, &mut id).code,
            0
        );
        assert_eq!(disunity_coroutine_stop(world, id).code, 0);
        let mut cancelled = 0;
        assert_eq!(disunity_coroutine_cancelled(world, &mut cancelled).code, 0);
        assert_eq!(cancelled, id);
        assert_eq!(disunity_coroutine_cancelled(world, &mut cancelled).code, 0);
        assert_eq!(cancelled, 0);
        assert_ne!(disunity_coroutine_stop(world, id).code, 0);
        assert_eq!(disunity_world_destroy(world).code, 0);
    }
}

#[test]
fn foreign_input_and_payload_validation_do_not_mutate_the_world() {
    let world = world();
    unsafe {
        assert_eq!(
            disunity_callback_read(world, 42, std::ptr::null_mut()).code,
            4
        );
        assert_ne!(disunity_callback_wait(world, 999, 0, 0, 1).code, 0);
        assert_ne!(
            crate::input::disunity_input_push(world, 0, 1, 0, 1, 0.5).code,
            0
        );
        assert_ne!(
            crate::input::disunity_input_push(world, 0, 1, 1, 0, f32::NAN).code,
            0
        );
        let (mut keys, mut axis) = (99, 99.0);
        assert_eq!(
            crate::input::disunity_input_read(world, 2, 0, &mut keys, &mut axis).code,
            4
        );
        assert_eq!((keys, axis), (99, 99.0));
        assert_eq!(disunity_world_destroy(world).code, 0);
    }
}
