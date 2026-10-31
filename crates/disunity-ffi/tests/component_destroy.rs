//! Component destruction through the ABI: pending until the frame ends, cleanup messages once, stale
//! identities refused.
use worldrunner_disunity_ffi::canvas_group::disunity_canvas_group_create;
use worldrunner_disunity_ffi::object_api::{disunity_component_destroy, disunity_script_destroy};
use worldrunner_disunity_ffi::scheduling::*;
use worldrunner_disunity_ffi::{abi::*, result_code, with_native_world};

const ON_DISABLE: u32 = 6;
const ON_DESTROY: u32 = 7;
const INVALID_ARGUMENT: u64 = 1;

fn world_with_object() -> (u64, u64) {
    let (mut world, mut object) = (0, 0);
    unsafe {
        assert_eq!(disunity_world_create(&mut world).code, result_code::OK);
        assert_eq!(
            disunity_gameobject_create(world, &mut object).code,
            result_code::OK
        );
    }
    (world, object)
}

/// Resumes every callback until `work` is not a callback, returning the script and message of each.
fn drain(world: u64, mut work: DisunityAdvance) -> (Vec<(u64, u32)>, DisunityAdvance) {
    let mut seen = Vec::new();
    while work.kind == 1 {
        seen.push((work.script, work.message));
        let ticket = work.ticket;
        let code =
            unsafe { disunity_world_resume(world, ticket, 0, std::ptr::null(), 0, &mut work).code };
        assert_eq!(code, result_code::OK);
    }
    (seen, work)
}

fn advance(world: u64) -> (Vec<(u64, u32)>, DisunityAdvance) {
    let mut work = DisunityAdvance::default();
    assert_eq!(
        unsafe { disunity_world_advance(world, 0, 1, &mut work) }.code,
        result_code::OK
    );
    drain(world, work)
}

fn register_script(world: u64, object: u64, identity: u64) {
    let mut script = 0;
    assert_eq!(
        unsafe { disunity_script_register(world, object, identity, &mut script) }.code,
        result_code::OK
    );
    let mut work = DisunityAdvance::default();
    assert_eq!(
        unsafe { disunity_object_wake(world, object, &mut work) }.code,
        result_code::OK
    );
    drain(world, work);
}

#[test]
fn a_destroyed_script_gets_its_cleanup_once_and_retires_at_frame_end() {
    let (world, object) = world_with_object();
    register_script(world, object, 9);
    advance(world);
    assert_eq!(disunity_script_destroy(world, 9).code, result_code::OK);
    assert_eq!(disunity_script_destroy(world, 9).code, result_code::OK);
    let (messages, _) = advance(world);
    let cleanup: Vec<_> = messages.iter().filter(|m| m.1 >= ON_DISABLE).collect();
    assert_eq!(cleanup, [&(9, ON_DISABLE), &(9, ON_DESTROY)]);
    let retired = disunity_script_destroy(world, 9);
    assert_eq!(
        (retired.code, retired.detail),
        (result_code::ERROR, INVALID_ARGUMENT)
    );
    let (later, _) = advance(world);
    assert!(later.iter().all(|m| m.1 < ON_DISABLE));
    assert_eq!(
        unsafe { disunity_world_destroy(world) }.code,
        result_code::OK
    );
}

#[test]
fn a_destroyed_native_component_stays_until_the_frame_ends() {
    let (world, object) = world_with_object();
    let mut group = 0;
    assert_eq!(
        unsafe { disunity_canvas_group_create(world, object, &mut group) }.code,
        result_code::OK
    );
    let count = |w: u64| with_native_world(w, |n| n.snapshot().components.len()).unwrap();
    let before = count(world);
    assert_eq!(
        disunity_component_destroy(world, group).code,
        result_code::OK
    );
    assert_eq!(count(world), before);
    advance(world);
    assert_eq!(count(world), before - 1);
    let retired = disunity_component_destroy(world, group);
    assert_eq!(
        (retired.code, retired.detail),
        (result_code::ERROR, INVALID_ARGUMENT)
    );
    assert_eq!(
        unsafe { disunity_world_destroy(world) }.code,
        result_code::OK
    );
}

#[test]
fn unknown_identities_are_refused() {
    let (world, _) = world_with_object();
    for result in [
        disunity_component_destroy(world, 987_654),
        disunity_script_destroy(world, 987_654),
        disunity_component_destroy(0, 1),
    ] {
        assert_eq!(
            (result.code, result.detail),
            (result_code::ERROR, INVALID_ARGUMENT)
        );
    }
    assert_eq!(
        unsafe { disunity_world_destroy(world) }.code,
        result_code::OK
    );
}

#[test]
fn flushing_runs_the_queued_cleanup_now_and_the_frame_does_not_repeat_it() {
    let (world, object) = world_with_object();
    register_script(world, object, 5);
    advance(world);
    assert_eq!(disunity_script_destroy(world, 5).code, result_code::OK);
    let mut work = DisunityAdvance::default();
    assert_eq!(
        unsafe { disunity_cleanup_flush(world, &mut work) }.code,
        result_code::OK
    );
    let (messages, last) = drain(world, work);
    assert_eq!(messages, [(5, ON_DISABLE), (5, ON_DESTROY)]);
    assert_eq!(last.kind, 2);
    let (frame, _) = advance(world);
    assert!(frame.iter().all(|m| m.1 < ON_DISABLE));
    assert_eq!(
        unsafe { disunity_world_destroy(world) }.code,
        result_code::OK
    );
}
