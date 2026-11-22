//! `disunity_world_advance_teardown` through the ABI: destroy-phase callbacks only.
use worldrunner_disunity_ffi::object_api::disunity_script_destroy;
use worldrunner_disunity_ffi::scheduling::*;
use worldrunner_disunity_ffi::{abi::*, result_code};

const ON_DISABLE: u32 = 6;
const ON_DESTROY: u32 = 7;
const UPDATE: u32 = 4;
const INVALID_ARGUMENT_REASON: u64 = 1;

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

fn register_script(world: u64, identity: u64) {
    let (mut object, mut script) = (0, 0);
    unsafe {
        assert_eq!(
            disunity_gameobject_create(world, &mut object).code,
            result_code::OK
        );
        assert_eq!(
            disunity_script_register(world, object, identity, &mut script).code,
            result_code::OK
        );
        let mut work = DisunityAdvance::default();
        assert_eq!(
            disunity_object_wake(world, object, &mut work).code,
            result_code::OK
        );
        drain(world, work);
    }
}

fn frame(world: u64) -> (Vec<(u64, u32)>, DisunityAdvance) {
    let mut work = DisunityAdvance::default();
    let code = unsafe { disunity_world_advance(world, 20_000_000, 1_000_000_000, &mut work) }.code;
    assert_eq!(code, result_code::OK);
    drain(world, work)
}

fn teardown(world: u64) -> (Vec<(u64, u32)>, DisunityAdvance) {
    let mut work = DisunityAdvance::default();
    let code = unsafe { disunity_world_advance_teardown(world, &mut work) }.code;
    assert_eq!(code, result_code::OK);
    drain(world, work)
}

#[test]
fn teardown_reports_the_destroy_phase_of_a_destroyed_script_and_no_update() {
    let mut world = 0;
    assert_eq!(
        unsafe { disunity_world_create(&mut world) }.code,
        result_code::OK
    );
    register_script(world, 3);
    register_script(world, 4);
    let (_, last) = frame(world);
    assert_eq!(disunity_script_destroy(world, 3).code, result_code::OK);
    let (seen, done) = teardown(world);
    assert_eq!(seen, [(3, ON_DISABLE), (3, ON_DESTROY)]);
    assert_eq!((done.kind, done.frame), (0, last.frame));
    let (next, _) = frame(world);
    assert!(next.contains(&(4, UPDATE)));
    assert_eq!(
        unsafe { disunity_world_destroy(world) }.code,
        result_code::OK
    );
}

#[test]
fn a_null_output_pointer_is_refused() {
    let mut world = 0;
    assert_eq!(
        unsafe { disunity_world_create(&mut world) }.code,
        result_code::OK
    );
    let refused = unsafe { disunity_world_advance_teardown(world, std::ptr::null_mut()) };
    assert_eq!(
        (refused.code, refused.detail),
        (result_code::INVALID_ARGUMENT, INVALID_ARGUMENT_REASON)
    );
    assert_eq!(
        unsafe { disunity_world_destroy(world) }.code,
        result_code::OK
    );
}

#[test]
fn an_unknown_world_is_refused() {
    let mut work = DisunityAdvance::default();
    let refused = unsafe { disunity_world_advance_teardown(987_654, &mut work) };
    assert_eq!(refused.code, result_code::ERROR);
}
