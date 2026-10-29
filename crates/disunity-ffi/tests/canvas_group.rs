//! The canvas group record: creation, round trip of every field, clamping and rejection.
use worldrunner_disunity_ffi::{abi::*, canvas_group::*, ui::*};

fn group_world() -> (u64, u64, u64) {
    let (mut world, mut object, mut group) = (0, 0, 0);
    unsafe {
        assert_eq!(disunity_world_create(&mut world).code, 0);
        assert_eq!(disunity_gameobject_create(world, &mut object).code, 0);
        assert_eq!(
            disunity_canvas_group_create(world, object, &mut group).code,
            0
        );
    }
    (world, object, group)
}

fn read(world: u64, group: u64) -> DisunityCanvasGroup {
    let mut value = DisunityCanvasGroup::default();
    assert_eq!(
        unsafe { disunity_canvas_group_read(world, group, &mut value) }.code,
        0
    );
    value
}

fn write(world: u64, group: u64, value: DisunityCanvasGroup) -> DisunityResult {
    unsafe { disunity_canvas_group_write(world, group, &value) }
}

fn fields(value: DisunityCanvasGroup) -> [u32; 5] {
    [
        value.enabled,
        value.alpha.to_bits(),
        value.interactable,
        value.blocks_raycasts,
        value.ignore_parent_groups,
    ]
}

/// Every setting differs from its default.
fn distinct() -> DisunityCanvasGroup {
    DisunityCanvasGroup {
        enabled: 0,
        alpha: 0.25,
        interactable: 0,
        blocks_raycasts: 0,
        ignore_parent_groups: 1,
    }
}

#[test]
fn a_new_group_has_the_engine_defaults() {
    let (world, _, group) = group_world();
    let value = read(world, group);
    assert_eq!(fields(value), [1, 1f32.to_bits(), 1, 1, 0]);
    assert_eq!(fields(value), fields(DisunityCanvasGroup::default()));
}

#[test]
fn every_field_round_trips() {
    let (world, _, group) = group_world();
    assert_eq!(write(world, group, distinct()).code, 0);
    assert_eq!(fields(read(world, group)), fields(distinct()));
    for index in 0..4 {
        let mut value = DisunityCanvasGroup::default();
        match index {
            0 => value.enabled = 0,
            1 => value.interactable = 0,
            2 => value.blocks_raycasts = 0,
            _ => value.ignore_parent_groups = 1,
        }
        assert_eq!(write(world, group, value).code, 0);
        assert_eq!(fields(read(world, group)), fields(value), "flag {index}");
    }
}

#[test]
fn alpha_is_clamped_and_nan_is_rejected() {
    let (world, _, group) = group_world();
    for (given, expected) in [(-2., 0.), (3., 1.), (f32::INFINITY, 1.), (0.5, 0.5)] {
        let value = DisunityCanvasGroup {
            alpha: given,
            ..DisunityCanvasGroup::default()
        };
        assert_eq!(write(world, group, value).code, 0);
        assert_eq!(read(world, group).alpha, expected, "alpha {given}");
    }
    assert_eq!(write(world, group, distinct()).code, 0);
    let nan = DisunityCanvasGroup {
        alpha: f32::NAN,
        ..DisunityCanvasGroup::default()
    };
    assert_ne!(write(world, group, nan).code, 0);
    assert_eq!(fields(read(world, group)), fields(distinct()));
}

#[test]
fn a_boolean_above_one_is_rejected_and_changes_nothing() {
    let (world, _, group) = group_world();
    assert_eq!(write(world, group, distinct()).code, 0);
    for index in 0..4 {
        let mut value = DisunityCanvasGroup::default();
        match index {
            0 => value.enabled = 2,
            1 => value.interactable = 2,
            2 => value.blocks_raycasts = 2,
            _ => value.ignore_parent_groups = 2,
        }
        assert_ne!(write(world, group, value).code, 0, "flag {index}");
        assert_eq!(fields(read(world, group)), fields(distinct()));
    }
}

#[test]
fn null_pointers_and_other_components_are_rejected() {
    let (world, object, group) = group_world();
    unsafe {
        assert_ne!(
            disunity_canvas_group_create(world, object, std::ptr::null_mut()).code,
            0
        );
        assert_ne!(
            disunity_canvas_group_read(world, group, std::ptr::null_mut()).code,
            0
        );
        assert_ne!(
            disunity_canvas_group_write(world, group, std::ptr::null()).code,
            0
        );
    }
    let mut again = 0;
    assert_ne!(
        unsafe { disunity_canvas_group_create(world, object, &mut again) }.code,
        0,
        "one group per object"
    );
    let mut canvas = 0;
    assert_eq!(
        unsafe { disunity_ui_create(world, object, 1, 0, &mut canvas) }.code,
        0
    );
    let mut value = DisunityCanvasGroup::default();
    assert_ne!(
        unsafe { disunity_canvas_group_read(world, canvas, &mut value) }.code,
        0
    );
}
