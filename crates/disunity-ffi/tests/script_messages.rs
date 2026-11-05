use worldrunner_disunity_ffi::{abi::*, scheduling::*, script_messages::disunity_script_messages};

const FIXED_UPDATE: u32 = 3;
const UPDATE: u32 = 4;
const LATE_UPDATE: u32 = 5;

/// Resumes every callback the advance yields and returns the messages in order.
unsafe fn drain(world: u64, mut work: DisunityAdvance) -> Vec<u32> {
    let mut seen = Vec::new();
    while work.kind == 1 {
        seen.push(work.message);
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

/// A world with one script bound to `identity`, after it reported `messages` if given.
unsafe fn world_with_script(identity: u64, messages: Option<u32>) -> u64 {
    let (mut world, mut object, mut script) = (0, 0, 0);
    unsafe {
        assert_eq!(disunity_world_create(&mut world).code, 0);
        assert_eq!(disunity_gameobject_create(world, &mut object).code, 0);
        assert_eq!(
            disunity_script_register(world, object, identity, &mut script).code,
            0
        );
        if let Some(messages) = messages {
            assert_eq!(disunity_script_messages(world, identity, messages).code, 0);
        }
        let mut work = DisunityAdvance::default();
        assert_eq!(disunity_object_wake(world, object, &mut work).code, 0);
        drain(world, work);
        // The first frame runs the one-time Start.
        frame(world);
    }
    world
}

unsafe fn frame(world: u64) -> Vec<u32> {
    let mut work = DisunityAdvance::default();
    unsafe {
        assert_eq!(
            disunity_world_advance(world, 20_000_000, 1_000_000_000, &mut work).code,
            0
        );
        drain(world, work)
    }
}

#[test]
fn a_reported_script_receives_only_the_messages_it_defines() {
    unsafe {
        let world = world_with_script(7, Some(0b110));
        let seen = frame(world);
        assert!(seen.contains(&UPDATE) && seen.contains(&LATE_UPDATE));
        assert!(!seen.contains(&FIXED_UPDATE));
        disunity_world_destroy(world);
    }
}

#[test]
fn a_report_made_after_the_first_frame_takes_effect() {
    unsafe {
        let world = world_with_script(7, None);
        assert_eq!(disunity_script_messages(world, 7, 0).code, 0);
        assert!(frame(world).is_empty());
        disunity_world_destroy(world);
    }
}

#[test]
fn an_unreported_script_receives_all_three_messages() {
    unsafe {
        let world = world_with_script(7, None);
        assert_eq!(frame(world), [FIXED_UPDATE, UPDATE, LATE_UPDATE]);
        disunity_world_destroy(world);
    }
}

#[test]
fn a_report_applies_to_the_named_script_only() {
    unsafe {
        let world = world_with_script(7, None);
        let (mut object, mut other) = (0, 0);
        assert_eq!(disunity_gameobject_create(world, &mut object).code, 0);
        assert_eq!(
            disunity_script_register(world, object, 8, &mut other).code,
            0
        );
        assert_eq!(disunity_script_messages(world, 8, 0).code, 0);
        let mut work = DisunityAdvance::default();
        assert_eq!(disunity_object_wake(world, object, &mut work).code, 0);
        drain(world, work);
        frame(world);
        assert_eq!(frame(world), [FIXED_UPDATE, UPDATE, LATE_UPDATE]);
        disunity_world_destroy(world);
    }
}

#[test]
fn unknown_bits_unbound_scripts_and_stale_worlds_are_rejected() {
    unsafe {
        let world = world_with_script(7, None);
        let invalid_argument = result_code::INVALID_ARGUMENT;
        assert_eq!(disunity_script_messages(world, 7, 8).code, invalid_argument);
        assert_eq!(
            disunity_script_messages(world, 7, u32::MAX).code,
            invalid_argument
        );
        assert_ne!(disunity_script_messages(world, 99, 1).code, 0);
        assert_ne!(disunity_script_messages(world, 0, 1).code, 0);
        assert_eq!(disunity_script_messages(world, 7, 7).code, 0);
        assert_eq!(disunity_world_destroy(world).code, 0);
        assert_eq!(
            disunity_script_messages(world, 7, 7).code,
            result_code::ERROR
        );
        assert_eq!(disunity_script_messages(0, 7, 7).code, result_code::ERROR);
    }
}
