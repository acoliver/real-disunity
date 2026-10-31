use worldrunner_disunity_ffi::{abi::*, camera_layers::*, sorting_group::*};

const SORTING_GROUP: u32 = 210;

fn world_object() -> (u64, u64) {
    let (mut world, mut object) = (0, 0);
    unsafe {
        assert_eq!(disunity_world_create(&mut world).code, 0);
        assert_eq!(disunity_gameobject_create(world, &mut object).code, 0);
    }
    (world, object)
}

fn created(world: u64, object: u64) -> u64 {
    let mut group = 0;
    unsafe {
        assert_eq!(
            disunity_camera_layer_create(world, object, SORTING_GROUP, &mut group).code,
            0
        );
    }
    group
}

fn read(world: u64, group: u64) -> [i64; 4] {
    let mut value = DisunitySortingGroup::default();
    unsafe {
        assert_eq!(
            disunity_sorting_group_read(world, group, &mut value).code,
            0
        );
    }
    [
        value.enabled.into(),
        value.sorting_layer.into(),
        value.sorting_order.into(),
        value.sort_at_root.into(),
    ]
}

#[test]
fn a_new_sorting_group_is_enabled_at_layer_zero_and_order_zero() {
    let (world, object) = world_object();
    let group = created(world, object);
    assert_eq!(read(world, group), [1, 0, 0, 0]);
}

#[test]
fn a_sorting_group_is_one_per_object() {
    let (world, object) = world_object();
    created(world, object);
    let mut second = 0;
    let result = unsafe { disunity_camera_layer_create(world, object, SORTING_GROUP, &mut second) };
    assert_ne!(result.code, 0);
}

#[test]
fn written_values_are_read_back_unclamped() {
    let (world, object) = world_object();
    let group = created(world, object);
    let value = DisunitySortingGroup {
        enabled: 0,
        sorting_layer: 3,
        sorting_order: -70_000,
        sort_at_root: 1,
    };
    unsafe {
        assert_eq!(disunity_sorting_group_write(world, group, &value).code, 0);
    }
    assert_eq!(read(world, group), [0, 3, -70_000, 1]);
}

#[test]
fn a_boolean_above_one_is_rejected_and_changes_nothing() {
    let (world, object) = world_object();
    let group = created(world, object);
    for (enabled, sort_at_root) in [(2, 0), (1, 2)] {
        let value = DisunitySortingGroup {
            enabled,
            sorting_layer: 9,
            sorting_order: 9,
            sort_at_root,
        };
        let result = unsafe { disunity_sorting_group_write(world, group, &value) };
        assert_ne!(result.code, 0);
        assert_eq!(read(world, group), [1, 0, 0, 0]);
    }
}

#[test]
fn another_component_and_null_pointers_are_rejected() {
    let (world, object) = world_object();
    let mut flare = 0;
    unsafe {
        assert_eq!(
            disunity_camera_layer_create(world, object, 124, &mut flare).code,
            0
        );
        let mut value = DisunitySortingGroup::default();
        assert_ne!(
            disunity_sorting_group_read(world, flare, &mut value).code,
            0
        );
        assert_ne!(disunity_sorting_group_write(world, flare, &value).code, 0);
        let group = created(world, object);
        assert_ne!(
            disunity_sorting_group_read(world, group, std::ptr::null_mut()).code,
            0
        );
        assert_ne!(
            disunity_sorting_group_write(world, group, std::ptr::null()).code,
            0
        );
    }
}
