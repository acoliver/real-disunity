//! The children query: an object's children by identity, with the size a small buffer needs.
use worldrunner_disunity_ffi::{abi::*, hierarchy::*, result_code, scheduling::*};

fn new_object(world: u64) -> u64 {
    let mut object = 0;
    assert_eq!(
        unsafe { disunity_gameobject_create(world, &mut object) }.code,
        result_code::OK
    );
    object
}

fn parent(world: u64, child: u64, parent: u64) {
    let mut work = DisunityAdvance::default();
    assert_eq!(
        unsafe { disunity_transform_parent(world, child, parent, 0, &mut work) }.code,
        result_code::OK
    );
}

/// A world with a parent holding three children, created in order.
fn family() -> (u64, u64, [u64; 3]) {
    let mut world = 0;
    assert_eq!(unsafe { disunity_world_create(&mut world) }.code, 0);
    let top = new_object(world);
    let kids = [(); 3].map(|()| new_object(world));
    for kid in kids {
        parent(world, kid, top);
    }
    (world, top, kids)
}

fn children(world: u64, object: u64, capacity: usize) -> (DisunityResult, Vec<u64>, u32) {
    let mut buffer = vec![0xa5a5_u64; capacity.max(1)];
    let mut written = 0;
    let result = unsafe {
        disunity_transform_children(
            world,
            object,
            buffer.as_mut_ptr(),
            capacity as u32,
            &mut written,
        )
    };
    (result, buffer, written)
}

fn sorted(mut values: Vec<u64>) -> Vec<u64> {
    values.sort_unstable();
    values
}

#[test]
fn children_are_the_objects_parented_to_the_object() {
    let (world, top, kids) = family();
    let (result, buffer, written) = children(world, top, 8);
    assert_eq!(result.code, result_code::OK);
    assert_eq!(written, 3);
    assert_eq!(sorted(buffer[..3].to_vec()), sorted(kids.to_vec()));
    assert_eq!(
        buffer[3], 0xa5a5,
        "the buffer past the children is untouched"
    );
    let (_, _, leaf) = children(world, kids[0], 8);
    assert_eq!(leaf, 0, "a leaf has no children");
}

#[test]
fn children_follow_reparenting() {
    let (world, top, kids) = family();
    let other = new_object(world);
    parent(world, kids[1], other);
    let (_, buffer, written) = children(world, top, 8);
    assert_eq!(
        sorted(buffer[..written as usize].to_vec()),
        sorted(vec![kids[0], kids[2]])
    );
    let (_, buffer, written) = children(world, other, 8);
    assert_eq!(buffer[..written as usize], [kids[1]]);
    parent(world, kids[1], 0);
    assert_eq!(
        children(world, other, 8).2,
        0,
        "a detached child leaves its parent"
    );
}

#[test]
fn a_small_buffer_reports_the_size_needed_and_writes_nothing() {
    let (world, top, _) = family();
    let (result, buffer, written) = children(world, top, 2);
    assert_eq!(result.code, result_code::BUDGET);
    assert_eq!(written, 3);
    assert_eq!(buffer[..2], [0xa5a5, 0xa5a5]);
    let (result, _, written) = children(world, top, 0);
    assert_eq!((result.code, written), (result_code::BUDGET, 3));
    let (result, buffer, written) = children(world, top, 3);
    assert_eq!(
        (result.code, written),
        (result_code::OK, 3),
        "an exact fit succeeds"
    );
    assert!(buffer.iter().all(|&id| id != 0xa5a5));
}

#[test]
fn unknown_objects_and_missing_outputs_are_errors() {
    let (world, top, _) = family();
    assert_eq!(children(world, 0xdead_beef, 8).0.code, result_code::ERROR);
    assert_eq!(children(world + 1000, top, 8).0.code, result_code::ERROR);
    let mut buffer = [0u64; 4];
    let mut written = 0;
    unsafe {
        assert_eq!(
            disunity_transform_children(world, top, buffer.as_mut_ptr(), 4, std::ptr::null_mut())
                .code,
            result_code::INVALID_ARGUMENT
        );
        assert_eq!(
            disunity_transform_children(world, top, std::ptr::null_mut(), 4, &mut written).code,
            result_code::INVALID_ARGUMENT
        );
    }
}
