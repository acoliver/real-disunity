use worldrunner_disunity_ffi::{
    abi::*, construction::*, object_api::disunity_object_alive, result_code, scheduling::*,
    with_native_world,
};

fn counts(world: u64) -> (usize, usize) {
    with_native_world(world, |native| native.storage_counts()).unwrap()
}

fn assert_alive(world: u64, object: u64, expected: u32) {
    let mut alive = 0;
    assert_eq!(
        unsafe { disunity_object_alive(world, object, &mut alive) }.code,
        result_code::OK
    );
    assert_eq!(alive, expected);
}

fn abort_one_object(world: u64) -> u64 {
    let (mut construction, mut created) = (0, 0);
    unsafe {
        assert_eq!(
            disunity_construction_begin(world, &mut construction).code,
            result_code::OK
        );
        assert_eq!(
            disunity_construction_object_create(world, construction, &mut created).code,
            result_code::OK
        );
    }
    assert_eq!(
        disunity_construction_abort(world, construction).code,
        result_code::OK
    );
    created
}

#[test]
fn pending_object_is_not_a_construction_and_abort_preserves_its_lifetime() {
    unsafe {
        let (mut world, mut pending) = (0, 0);
        assert_eq!(disunity_world_create(&mut world).code, result_code::OK);
        assert_eq!(
            disunity_gameobject_create(world, &mut pending).code,
            result_code::OK
        );
        assert_eq!(
            disunity_gameobject_destroy(world, pending).code,
            result_code::OK
        );
        let mut rejected = 0;
        assert_eq!(
            disunity_construction_object_create(world, pending, &mut rejected).code,
            result_code::ERROR
        );
        assert_eq!(rejected, 0);
        assert_eq!(counts(world), (1, 1));
        let created = abort_one_object(world);
        assert_eq!(counts(world), (1, 1));
        assert_alive(world, pending, 1);
        assert_alive(world, created, 0);
        let mut work = DisunityAdvance::default();
        assert_eq!(
            disunity_world_advance(world, 0, 1, &mut work).code,
            result_code::OK
        );
        assert_alive(world, pending, 0);
        assert_eq!(counts(world), (0, 0));
        assert_eq!(disunity_world_destroy(world).code, result_code::OK);
    }
}
