use std::ffi::CString;
use worldrunner_disunity_ffi::{
    abi::*, scheduling::*, script_messages::disunity_script_type_messages,
};

const FIXED_UPDATE: u32 = 3;
const UPDATE: u32 = 4;
const LATE_UPDATE: u32 = 5;

fn report(world: u64, assembly: &str, type_name: &str, messages: u32) -> i32 {
    let (assembly, type_name) = (
        CString::new(assembly).unwrap(),
        CString::new(type_name).unwrap(),
    );
    unsafe {
        disunity_script_type_messages(
            world,
            assembly.as_ptr().cast(),
            type_name.as_ptr().cast(),
            messages,
        )
        .code
    }
}

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

/// A world with one script bound to `identity`, after its type reported `messages` if given.
unsafe fn world_with_script(identity: u64, messages: Option<u32>) -> u64 {
    let (mut world, mut object, mut script) = (0, 0, 0);
    unsafe {
        assert_eq!(disunity_world_create(&mut world).code, 0);
        assert_eq!(disunity_gameobject_create(world, &mut object).code, 0);
        if let Some(messages) = messages {
            assert_eq!(report(world, "Managed", &identity.to_string(), messages), 0);
        }
        assert_eq!(
            disunity_script_register(world, object, identity, &mut script).code,
            0
        );
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
fn a_reported_type_receives_only_the_messages_it_defines() {
    unsafe {
        let world = world_with_script(7, Some(0b110));
        let seen = frame(world);
        assert!(seen.contains(&UPDATE) && seen.contains(&LATE_UPDATE));
        assert!(!seen.contains(&FIXED_UPDATE));
        disunity_world_destroy(world);
    }
}

#[test]
fn a_type_reported_after_its_script_registered_takes_effect() {
    unsafe {
        let world = world_with_script(7, None);
        assert_eq!(report(world, "Managed", "7", 0), 0);
        assert!(frame(world).is_empty());
        disunity_world_destroy(world);
    }
}

#[test]
fn an_unreported_type_receives_all_three_messages() {
    unsafe {
        let world = world_with_script(7, None);
        assert_eq!(frame(world), [FIXED_UPDATE, UPDATE, LATE_UPDATE]);
        disunity_world_destroy(world);
    }
}

#[test]
fn unknown_bits_null_text_invalid_utf8_and_stale_worlds_are_rejected() {
    unsafe {
        let mut world = 0;
        assert_eq!(disunity_world_create(&mut world).code, 0);
        let valid = b"Managed\0";
        let invalid: [u8; 3] = [0xff, 0xfe, 0];
        let call = |assembly: *const u8, name: *const u8, bits| {
            disunity_script_type_messages(world, assembly, name, bits).code
        };
        let invalid_argument = result_code::INVALID_ARGUMENT;
        assert_eq!(call(valid.as_ptr(), valid.as_ptr(), 8), invalid_argument);
        assert_eq!(
            call(valid.as_ptr(), valid.as_ptr(), u32::MAX),
            invalid_argument
        );
        assert_eq!(call(std::ptr::null(), valid.as_ptr(), 1), invalid_argument);
        assert_eq!(call(valid.as_ptr(), std::ptr::null(), 1), invalid_argument);
        assert_eq!(call(invalid.as_ptr(), valid.as_ptr(), 1), invalid_argument);
        assert_eq!(call(valid.as_ptr(), invalid.as_ptr(), 1), invalid_argument);
        assert_eq!(call(valid.as_ptr(), valid.as_ptr(), 7), 0);
        assert_eq!(disunity_world_destroy(world).code, 0);
        assert_eq!(call(valid.as_ptr(), valid.as_ptr(), 7), result_code::ERROR);
    }
}

#[test]
fn an_empty_type_name_is_rejected_by_the_world() {
    unsafe {
        let mut world = 0;
        assert_eq!(disunity_world_create(&mut world).code, 0);
        assert_ne!(report(world, "Managed", "", 1), 0);
        assert_eq!(disunity_world_destroy(world).code, 0);
    }
}
