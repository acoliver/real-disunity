use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
};

use worldrunner_disunity_ffi::{
    DisunityCommand, DisunityObjectState, abi::*, command_kind, result_code, with_native_world,
};

struct CountingAllocator;

thread_local! {
    static ALLOCATED: Cell<Option<usize>> = const { Cell::new(None) };
}

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATED.with(|count| {
            if let Some(bytes) = count.get() {
                count.set(Some(bytes + layout.size()));
            }
        });
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

fn allocated<T>(f: impl FnOnce() -> T) -> (T, usize) {
    ALLOCATED.with(|count| count.set(Some(0)));
    let value = f();
    let bytes = ALLOCATED.with(|count| count.replace(None).unwrap());
    (value, bytes)
}

struct Session(u64);

impl Session {
    fn new() -> Self {
        let mut world = 0;
        assert_eq!(
            unsafe { disunity_world_create(&mut world) }.code,
            result_code::OK
        );
        Self(world)
    }

    fn object(&self) -> u64 {
        let mut object = 0;
        assert_eq!(
            unsafe { disunity_gameobject_create(self.0, &mut object) }.code,
            result_code::OK
        );
        object
    }

    fn submit(&self, commands: &[DisunityCommand]) {
        assert_eq!(
            unsafe {
                disunity_command_buffer_submit(self.0, commands.as_ptr(), commands.len() as u32)
            }
            .code,
            result_code::OK
        );
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        assert_eq!(
            unsafe { disunity_world_destroy(self.0) }.code,
            result_code::OK
        );
    }
}

fn material(object: u64, slot: u32) -> DisunityCommand {
    DisunityCommand {
        object,
        kind: command_kind::SET_MATERIAL_SLOT,
        x: slot as f32,
        y: 0.0,
        z: 0.0,
    }
}

fn empty_state() -> DisunityObjectState {
    DisunityObjectState {
        object: 0,
        x: 0.0,
        y: 0.0,
        z: 0.0,
        rotation_y_radians: 0.0,
        material_slot: 0,
    }
}

fn assert_allocation_free_capacity_probe(session: &Session) {
    let mut written = 0;
    let (result, bytes) = allocated(|| unsafe {
        disunity_world_export_objects(session.0, std::ptr::null_mut(), 0, &mut written)
    });
    assert_eq!(result.code, result_code::OK);
    assert_eq!(written, 64);
    assert_eq!(bytes, 0, "capacity probing must not construct a snapshot");
}

fn assert_truncated_export(session: &Session, states: &mut [DisunityObjectState]) {
    let complete = states.to_vec();
    states.fill(empty_state());
    let mut written = 0;
    assert_eq!(
        unsafe { disunity_world_export_objects(session.0, states.as_mut_ptr(), 3, &mut written) }
            .code,
        result_code::OK
    );
    assert_eq!(written, 64);
    assert_eq!(&states[..3], &complete[..3]);
    assert!(states[3..].iter().all(|state| *state == empty_state()));
}

#[test]
fn bulk_export_reads_live_handles_without_snapshot_allocations() {
    let session = Session::new();
    let mut objects: Vec<_> = (0..64).map(|_| session.object()).collect();
    let commands: Vec<_> = objects
        .iter()
        .enumerate()
        .map(|(index, &object)| material(object, index as u32 + 1))
        .collect();
    session.submit(&commands);
    assert_eq!(
        unsafe { advance_frame(session.0, 0.0, 1) }.code,
        result_code::OK
    );
    let removed = objects.remove(7);
    assert_eq!(
        unsafe { disunity_gameobject_destroy(session.0, removed) }.code,
        result_code::OK
    );
    assert_eq!(
        unsafe { advance_frame(session.0, 0.0, 2) }.code,
        result_code::OK
    );
    let replacement = session.object();
    assert!(!objects.contains(&replacement));
    objects.push(replacement);

    assert_allocation_free_capacity_probe(&session);

    let mut written = 0;
    let mut states = vec![empty_state(); 64];
    let (result, bytes) = allocated(|| unsafe {
        disunity_world_export_objects(session.0, states.as_mut_ptr(), 64, &mut written)
    });
    assert_eq!(result.code, result_code::OK);
    assert_eq!(bytes, 0, "bulk export must read native values in place");
    for state in &states {
        assert!(objects.contains(&state.object));
        let expected = commands
            .iter()
            .find(|c| c.object == state.object)
            .map_or(0, |c| c.x as u32);
        assert_eq!(state.material_slot, expected);
    }
    assert_truncated_export(&session, &mut states);
}

#[test]
fn diagnostics_allocate_only_the_message_not_world_or_history() {
    let session = Session::new();
    let objects: Vec<_> = (0..64).map(|_| session.object()).collect();
    let commands: Vec<_> = objects.iter().map(|&object| material(object, 3)).collect();
    session.submit(&commands);
    assert_eq!(
        unsafe { advance_frame(session.0, 0.0, 1) }.code,
        result_code::OK
    );
    session.submit(&commands);
    let before = with_native_world(session.0, |world| world.snapshot()).unwrap();
    let mut buffer = [0; 128];
    let mut length = 0;
    let (result, bytes) = allocated(|| unsafe {
        disunity_error_message(session.0, buffer.as_mut_ptr(), 128, &mut length)
    });
    assert_eq!(result.code, result_code::OK);
    assert_eq!(length, 0);
    assert_eq!(bytes, 0, "an absent error must not clone unrelated storage");

    assert_eq!(
        unsafe { advance_frame(session.0, f32::NAN, 2) }.code,
        result_code::ERROR
    );
    let expected = b"time is outside rational bounds";
    for capacity in [0, 128] {
        let (result, bytes) = allocated(|| unsafe {
            disunity_error_message(session.0, buffer.as_mut_ptr(), capacity, &mut length)
        });
        assert_eq!(
            result.code,
            if capacity == 0 {
                result_code::BUDGET
            } else {
                result_code::OK
            }
        );
        assert_eq!(length as usize, expected.len());
        assert_eq!(bytes, expected.len(), "only the message may be cloned");
    }
    assert_eq!(&buffer[..length as usize], expected);
    assert_eq!(
        with_native_world(session.0, |world| world.snapshot()).unwrap(),
        before
    );
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
