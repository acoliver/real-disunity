use worldrunner_disunity_ffi::{
    abi::*, scheduling::*, script_messages::disunity_script_execution_order,
};

const UPDATE: u32 = 4;

/// Resumes every callback the advance yields and returns the scripts that received `message`.
unsafe fn drain(world: u64, mut work: DisunityAdvance, message: u32) -> Vec<u64> {
    let mut seen = Vec::new();
    while work.kind == 1 {
        if work.message == message {
            seen.push(work.script);
        }
        let ticket = work.ticket;
        unsafe {
            assert_eq!(
                disunity_world_resume(world, ticket, 0, std::ptr::null(), 0, &mut work).code,
                0
            );
        }
    }
    seen
}

unsafe fn frame(world: u64, message: u32) -> Vec<u64> {
    let mut work = DisunityAdvance::default();
    unsafe {
        assert_eq!(
            disunity_world_advance(world, 20_000_000, 1_000_000_000, &mut work).code,
            0
        );
        drain(world, work, message)
    }
}

/// A world with one script on its own object for each identity, registered in the order given.
unsafe fn world_with_scripts(identities: &[u64]) -> u64 {
    let mut world = 0;
    unsafe {
        assert_eq!(disunity_world_create(&mut world).code, 0);
        for &identity in identities {
            let (mut object, mut script) = (0, 0);
            assert_eq!(disunity_gameobject_create(world, &mut object).code, 0);
            assert_eq!(
                disunity_script_register(world, object, identity, &mut script).code,
                0
            );
            let mut work = DisunityAdvance::default();
            assert_eq!(disunity_object_wake(world, object, &mut work).code, 0);
            drain(world, work, UPDATE);
        }
    }
    world
}

#[test]
fn a_script_with_a_lower_order_receives_update_before_one_registered_earlier() {
    unsafe {
        let world = world_with_scripts(&[10, 20, 30]);
        assert_eq!(disunity_script_execution_order(world, 30, -200).code, 0);
        assert_eq!(disunity_script_execution_order(world, 10, 5).code, 0);
        assert_eq!(frame(world, UPDATE), [30, 20, 10]);
        assert_eq!(frame(world, UPDATE), [30, 20, 10]);
        assert_eq!(disunity_world_destroy(world).code, 0);
    }
}

#[test]
fn unbound_scripts_and_stale_worlds_are_rejected() {
    unsafe {
        let world = world_with_scripts(&[7]);
        assert_ne!(disunity_script_execution_order(world, 99, 1).code, 0);
        assert_eq!(disunity_script_execution_order(world, 7, i32::MIN).code, 0);
        assert_eq!(disunity_world_destroy(world).code, 0);
        assert_eq!(
            disunity_script_execution_order(world, 7, 1).code,
            result_code::ERROR
        );
        assert_eq!(
            disunity_script_execution_order(0, 7, 1).code,
            result_code::ERROR
        );
    }
}
