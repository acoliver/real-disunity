//! Store behavior tests through the Rust API: handle invalidation, command
//! replay determinism, budget rejection, and the load/unload soak.

use worldrunner_disunity_ffi::abi::{
    DisunityCommand, disunity_command_buffer_submit, disunity_gameobject_create,
    disunity_gameobject_destroy, disunity_store_statistics, disunity_world_create,
    disunity_world_destroy,
};
use worldrunner_disunity_ffi::{DisunityObject, command_kind, result_code};

/// The native store is a process global, so the tests here run one at a time;
/// otherwise a soak's drain assertion races with another test's live world.
static SERIALIZE: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn serialized() -> std::sync::MutexGuard<'static, ()> {
    SERIALIZE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn make_world() -> u64 {
    let mut world = 0;
    let result = unsafe { disunity_world_create(&mut world) };
    assert_eq!(result.code, result_code::OK);
    world
}

#[test]
fn destroyed_object_handles_are_invalid() {
    let _guard = serialized();
    let world = make_world();
    let mut object = 0;
    assert_eq!(
        unsafe { disunity_gameobject_create(world, &mut object) }.code,
        result_code::OK
    );
    assert_eq!(
        unsafe { disunity_gameobject_destroy(world, object) }.code,
        result_code::OK
    );
    assert_eq!(
        unsafe { advance_frame(world, 0.0, 0) }.code,
        result_code::OK
    );
    // The stale handle must not address an object even after slot reuse.
    let stale = object;
    let mut replacement = 0;
    assert_eq!(
        unsafe { disunity_gameobject_create(world, &mut replacement) }.code,
        result_code::OK
    );
    assert_ne!(stale, replacement);
    let bad = DisunityCommand {
        object: stale,
        kind: command_kind::SET_POSITION,
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };
    assert_eq!(
        unsafe { disunity_command_buffer_submit(world, &bad, 1) }.code,
        result_code::OK
    );
    assert_eq!(
        unsafe { advance_frame(world, 1.0 / 60.0, 1) }.code,
        result_code::ERROR
    );
    unsafe { disunity_world_destroy(world) };
}

#[test]
fn repeated_edit_batches_are_deterministic() {
    let _guard = serialized();
    let world = make_world();
    let mut objects = [0_u64; 4];
    for handle in &mut objects {
        assert_eq!(
            unsafe { disunity_gameobject_create(world, handle) }.code,
            result_code::OK
        );
    }
    let commands: Vec<DisunityCommand> = objects
        .iter()
        .enumerate()
        .flat_map(|(index, &object)| {
            [
                DisunityCommand {
                    object,
                    kind: command_kind::SET_POSITION,
                    x: index as f32,
                    y: 0.5,
                    z: -(index as f32),
                },
                DisunityCommand {
                    object,
                    kind: command_kind::SET_MATERIAL_SLOT,
                    x: (index + 1) as f32,
                    y: 0.0,
                    z: 0.0,
                },
            ]
        })
        .collect();
    assert_eq!(
        unsafe { disunity_command_buffer_submit(world, commands.as_ptr(), commands.len() as u32) }
            .code,
        result_code::OK
    );
    assert_eq!(
        unsafe { advance_frame(world, 1.0 / 60.0, 1) }.code,
        result_code::OK
    );
    assert_replay_matches(world, &commands);
    unsafe { disunity_world_destroy(world) };
}

fn assert_replay_matches(world: u64, log: &[DisunityCommand]) {
    // Repeat caller-owned edits against newly allocated identities.
    let replay_world = make_world();
    let mut replay_objects = [0_u64; 4];
    for handle in &mut replay_objects {
        unsafe { disunity_gameobject_create(replay_world, handle) };
    }
    // The log's object handles were minted in the first world; replay requires
    // the same slot order, which the equal-size creates guarantee here.
    let remapped: Vec<DisunityCommand> = log
        .iter()
        .enumerate()
        .map(|(i, command)| {
            let mut c = *command;
            c.object = replay_objects[i / 2];
            c
        })
        .collect();
    unsafe {
        disunity_command_buffer_submit(replay_world, remapped.as_ptr(), remapped.len() as u32)
    };
    unsafe { advance_frame(replay_world, 1.0 / 60.0, 1) };
    let expected: Vec<DisunityObject> = worldrunner_disunity_ffi::with_world(world, |w| {
        w.objects.iter().flatten().copied().collect::<Vec<_>>()
    })
    .unwrap();
    let replayed: Vec<DisunityObject> = worldrunner_disunity_ffi::with_world(replay_world, |w| {
        w.objects.iter().flatten().copied().collect::<Vec<_>>()
    })
    .unwrap();
    assert_eq!(expected, replayed);
    unsafe { disunity_world_destroy(replay_world) };
}

#[test]
fn oversize_submit_is_a_budget_result() {
    let _guard = serialized();
    let world = make_world();
    let count = worldrunner_disunity_ffi::MAX_COMMANDS_PER_SUBMIT + 1;
    assert_eq!(
        unsafe { disunity_command_buffer_submit(world, std::ptr::null(), count) }.code,
        result_code::BUDGET
    );
    unsafe { disunity_world_destroy(world) };
}

#[test]
fn error_message_round_trips_after_a_failed_step() {
    let _guard = serialized();
    let world = make_world();
    let mut object = 0;
    unsafe { disunity_gameobject_create(world, &mut object) };
    let bad = DisunityCommand {
        object,
        kind: 999,
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };
    unsafe { disunity_command_buffer_submit(world, &bad, 1) };
    assert_eq!(
        unsafe { advance_frame(world, 1.0 / 60.0, 1) }.code,
        result_code::ERROR
    );
    let mut buffer = [0_u8; 256];
    let mut length = 0_u32;
    assert_eq!(
        unsafe {
            disunity_error_message(world, buffer.as_mut_ptr(), buffer.len() as u32, &mut length)
        }
        .code,
        result_code::OK
    );
    let message = std::str::from_utf8(&buffer[..length as usize]).unwrap();
    assert_eq!(message, "unknown command kind");
    unsafe { disunity_world_destroy(world) };
}

use worldrunner_disunity_ffi::abi::disunity_error_message;

#[test]
fn load_unload_soak_drains_the_table() {
    let _guard = serialized();
    let (live_before, allocated_before) = {
        let mut live = 0;
        let mut allocated = 0;
        unsafe { disunity_store_statistics(&mut live, &mut allocated) };
        (live, allocated)
    };
    for _ in 0..100 {
        let world = make_world();
        let mut object = 0;
        unsafe { disunity_gameobject_create(world, &mut object) };
        assert_eq!(
            unsafe { disunity_gameobject_destroy(world, object) }.code,
            result_code::OK
        );
        assert_eq!(
            unsafe { disunity_world_destroy(world) }.code,
            result_code::OK
        );
    }
    let mut live = 0;
    let mut allocated = 0;
    unsafe { disunity_store_statistics(&mut live, &mut allocated) };
    assert_eq!(live, live_before, "live worlds must drain after 100 cycles");
    // Slots are reused, so allocated must not grow by 100 either.
    assert!(allocated <= allocated_before + 100);
    assert!(allocated < 100 + allocated_before);
}

unsafe fn advance_frame(
    world: u64,
    delta: f32,
    _frame: u64,
) -> worldrunner_disunity_ffi::DisunityResult {
    let mut work = worldrunner_disunity_ffi::scheduling::DisunityAdvance::default();
    let elapsed = std::time::Duration::try_from_secs_f32(delta)
        .ok()
        .and_then(|d| u64::try_from(d.as_nanos()).ok());
    unsafe {
        worldrunner_disunity_ffi::scheduling::disunity_world_advance(
            world,
            elapsed.unwrap_or(0),
            if elapsed.is_some() { 1_000_000_000 } else { 0 },
            &mut work,
        )
    }
}
