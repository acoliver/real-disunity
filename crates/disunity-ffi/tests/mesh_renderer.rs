//! The mesh renderer, skinned mesh renderer and mesh filter records: creation, round trip,
//! rejection and one component per object.
use worldrunner_disunity_ffi::{abi::*, mesh_renderer::*};

fn world_and_object() -> (u64, u64) {
    let (mut world, mut object) = (0, 0);
    unsafe {
        assert_eq!(disunity_world_create(&mut world).code, 0);
        assert_eq!(disunity_gameobject_create(world, &mut object).code, 0);
    }
    (world, object)
}

fn renderer_of_kind(kind: u32) -> (u64, u64, u64) {
    let (world, object) = world_and_object();
    let mut renderer = 0;
    let made = unsafe { disunity_mesh_renderer_create(world, object, kind, &mut renderer) };
    assert_eq!(made.code, 0);
    (world, object, renderer)
}

fn read(world: u64, renderer: u64) -> DisunityMeshRenderer {
    let mut value = unsafe { std::mem::zeroed() };
    let result = unsafe { disunity_mesh_renderer_read(world, renderer, &mut value) };
    assert_eq!(result.code, 0);
    value
}

fn write(world: u64, renderer: u64, value: &DisunityMeshRenderer) -> DisunityResult {
    unsafe { disunity_mesh_renderer_write(world, renderer, value) }
}

fn fields(value: &DisunityMeshRenderer) -> [i64; 15] {
    [
        value.kind.into(),
        value.enabled.into(),
        value.force_rendering_off.into(),
        value.receive_shadows.into(),
        value.allow_occlusion_when_dynamic.into(),
        value.static_shadow_caster.into(),
        value.shadow_casting_mode.into(),
        value.motion_vector_mode.into(),
        value.light_probe_usage.into(),
        value.reflection_probe_usage.into(),
        value.rendering_layer_mask.into(),
        value.renderer_priority.into(),
        value.sorting_layer.into(),
        value.sorting_order.into(),
        value.material_count.into(),
    ]
}

#[test]
fn a_new_renderer_has_the_engine_defaults() {
    let (world, _, renderer) = renderer_of_kind(0);
    assert_eq!(
        fields(&read(world, renderer)),
        [0, 1, 0, 1, 1, 0, 1, 1, 1, 1, 1, 0, 0, 0, 0]
    );
    let (world, _, skinned) = renderer_of_kind(1);
    assert_eq!(read(world, skinned).kind, 1);
    let (world, _, particle) = renderer_of_kind(2);
    assert_eq!(
        fields(&read(world, particle)),
        [2, 1, 0, 1, 1, 0, 1, 1, 1, 1, 1, 0, 0, 0, 0]
    );
}

#[test]
fn a_particle_renderer_keeps_its_kind() {
    let (world, _, particle) = renderer_of_kind(2);
    let mut value = read(world, particle);
    value.kind = 0;
    assert_ne!(write(world, particle, &value).code, 0);
    assert_eq!(read(world, particle).kind, 2);
}

#[test]
fn every_field_round_trips() {
    let (world, _, renderer) = renderer_of_kind(1);
    let mut value = read(world, renderer);
    value.enabled = 0;
    value.force_rendering_off = 1;
    value.receive_shadows = 0;
    value.allow_occlusion_when_dynamic = 0;
    value.static_shadow_caster = 1;
    value.shadow_casting_mode = 3;
    value.motion_vector_mode = 2;
    value.light_probe_usage = 4;
    value.reflection_probe_usage = 3;
    value.rendering_layer_mask = 0xF0F0;
    value.renderer_priority = -7;
    value.sorting_order = 42;
    value.material_count = 64;
    assert_eq!(write(world, renderer, &value).code, 0);
    assert_eq!(fields(&read(world, renderer)), fields(&value));
}

#[test]
fn bad_values_a_new_kind_and_big_counts_are_rejected_and_change_nothing() {
    let (world, _, renderer) = renderer_of_kind(0);
    let before = fields(&read(world, renderer));
    let bad: [fn(&mut DisunityMeshRenderer); 9] = [
        |v| v.enabled = 2,
        |v| v.force_rendering_off = 2,
        |v| v.receive_shadows = 2,
        |v| v.shadow_casting_mode = 4,
        |v| v.motion_vector_mode = 3,
        |v| v.light_probe_usage = 3,
        |v| v.reflection_probe_usage = 4,
        |v| v.material_count = 65,
        |v| v.kind = 1,
    ];
    for change in bad {
        let mut value = read(world, renderer);
        change(&mut value);
        value.sorting_order = 9;
        assert_ne!(write(world, renderer, &value).code, 0);
        assert_eq!(fields(&read(world, renderer)), before);
    }
}

#[test]
fn one_renderer_and_one_filter_per_object() {
    let (world, object, _) = renderer_of_kind(0);
    let mut again = 0;
    unsafe {
        assert_ne!(
            disunity_mesh_renderer_create(world, object, 1, &mut again).code,
            0
        );
        assert_eq!(
            disunity_mesh_filter_create(world, object, &mut again).code,
            0
        );
        assert_ne!(disunity_mesh_filter_create(world, object, &mut 0).code, 0);
    }
}

#[test]
fn a_filter_stores_its_mesh_identity() {
    let (world, object) = world_and_object();
    let (mut filter, mut mesh) = (0, 7);
    unsafe {
        assert_eq!(
            disunity_mesh_filter_create(world, object, &mut filter).code,
            0
        );
        assert_eq!(disunity_mesh_filter_read(world, filter, &mut mesh).code, 0);
        assert_eq!(mesh, 0);
        assert_eq!(disunity_mesh_filter_write(world, filter, 123).code, 0);
        assert_eq!(disunity_mesh_filter_read(world, filter, &mut mesh).code, 0);
    }
    assert_eq!(mesh, 123);
}

#[test]
fn null_pointers_unknown_kinds_and_other_components_are_rejected() {
    let (world, object, renderer) = renderer_of_kind(0);
    let (mut filter, mut mesh) = (0, 0);
    unsafe {
        assert_ne!(
            disunity_mesh_renderer_create(world, object, 0, std::ptr::null_mut()).code,
            0
        );
        assert_ne!(
            disunity_mesh_renderer_read(world, renderer, std::ptr::null_mut()).code,
            0
        );
        assert_ne!(
            disunity_mesh_renderer_write(world, renderer, std::ptr::null()).code,
            0
        );
        assert_ne!(
            disunity_mesh_filter_read(world, renderer, &mut mesh).code,
            0,
            "a renderer is not a filter"
        );
        assert_eq!(
            disunity_mesh_filter_create(world, object, &mut filter).code,
            0
        );
        let mut record = std::mem::zeroed();
        assert_ne!(
            disunity_mesh_renderer_read(world, filter, &mut record).code,
            0,
            "a filter is not a renderer"
        );
    }
    let (world, object) = world_and_object();
    let mut made = 0;
    assert_ne!(
        unsafe { disunity_mesh_renderer_create(world, object, 3, &mut made) }.code,
        0
    );
}

const PLAYER_TEXTURE: u64 = 1 << 40;
const PLAYER_MESH: u64 = 1 << 41;

fn slot(r: f32, texture: u64) -> DisunityDrawnMaterial {
    DisunityDrawnMaterial {
        r,
        g: 0.5,
        b: 0.25,
        a: 1.0,
        texture,
    }
}

fn write_slots(world: u64, renderer: u64, slots: &[DisunityDrawnMaterial]) -> DisunityResult {
    let pointer = if slots.is_empty() {
        std::ptr::null()
    } else {
        slots.as_ptr()
    };
    unsafe { disunity_mesh_renderer_slots_write(world, renderer, pointer, slots.len() as u32) }
}

fn read_slots(world: u64, renderer: u64, capacity: usize) -> (u32, Vec<DisunityDrawnMaterial>) {
    let mut slots = vec![DisunityDrawnMaterial::default(); capacity];
    let mut count = 99;
    let pointer = if capacity == 0 {
        std::ptr::null_mut()
    } else {
        slots.as_mut_ptr()
    };
    let result = unsafe {
        disunity_mesh_renderer_slots_read(world, renderer, pointer, capacity as u32, &mut count)
    };
    assert_eq!(result.code, 0);
    (count, slots)
}

fn with_material_count(world: u64, renderer: u64, count: u32) {
    let mut value = read(world, renderer);
    value.material_count = count;
    assert_eq!(write(world, renderer, &value).code, 0);
}

#[test]
fn slots_round_trip_and_replace_each_other() {
    let (world, _, renderer) = renderer_of_kind(0);
    with_material_count(world, renderer, 3);
    assert_eq!(read_slots(world, renderer, 0).0, 0);
    let first = [slot(0.1, PLAYER_TEXTURE), slot(2.0, 0)];
    assert_eq!(write_slots(world, renderer, &first).code, 0);
    assert_eq!(read_slots(world, renderer, 2), (2, first.to_vec()));
    assert_eq!(read_slots(world, renderer, 0).0, 2);
    assert_eq!(write_slots(world, renderer, &[slot(0.7, 0)]).code, 0);
    assert_eq!(read_slots(world, renderer, 1), (1, vec![slot(0.7, 0)]));
}

#[test]
fn slots_beyond_the_material_count_or_not_drawable_are_rejected_and_change_nothing() {
    let (world, _, renderer) = renderer_of_kind(0);
    with_material_count(world, renderer, 1);
    assert_eq!(write_slots(world, renderer, &[slot(0.5, 0)]).code, 0);
    let too_many = [slot(0.1, 0), slot(0.2, 0)];
    let negative = [slot(-0.1, 0)];
    let not_a_number = [slot(f32::NAN, 0)];
    let low_texture = [slot(0.1, PLAYER_TEXTURE - 1)];
    for bad in [&too_many[..], &negative, &not_a_number, &low_texture] {
        assert_ne!(write_slots(world, renderer, bad).code, 0);
        assert_eq!(read_slots(world, renderer, 1), (1, vec![slot(0.5, 0)]));
    }
    let null = unsafe { disunity_mesh_renderer_slots_write(world, renderer, std::ptr::null(), 1) };
    assert_ne!(null.code, 0);
}

#[test]
fn a_settings_write_keeps_the_slots_and_a_lower_material_count_drops_the_extra_ones() {
    let (world, _, renderer) = renderer_of_kind(0);
    with_material_count(world, renderer, 2);
    write_slots(world, renderer, &[slot(0.1, 0), slot(0.2, 0)]);
    let mut value = read(world, renderer);
    value.sorting_order = 4;
    assert_eq!(write(world, renderer, &value).code, 0);
    assert_eq!(read_slots(world, renderer, 2).0, 2);
    with_material_count(world, renderer, 1);
    assert_eq!(read_slots(world, renderer, 1), (1, vec![slot(0.1, 0)]));
}

#[test]
fn the_drawn_mesh_of_a_filter_is_set_apart_and_naming_another_mesh_resets_it() {
    let (world, object) = world_and_object();
    let mut filter = 0;
    assert_eq!(
        unsafe { disunity_mesh_filter_create(world, object, &mut filter) }.code,
        0
    );
    assert_eq!(disunity_mesh_filter_write(world, filter, 7).code, 0);
    assert_eq!(
        disunity_mesh_filter_drawn_write(world, filter, PLAYER_MESH + 1).code,
        0
    );
    assert_eq!(disunity_mesh_filter_write(world, filter, 7).code, 0);
    assert_eq!(
        drawn_mesh(world, filter),
        PLAYER_MESH + 1,
        "naming the same mesh keeps it"
    );
    assert_eq!(disunity_mesh_filter_write(world, filter, 8).code, 0);
    assert_eq!(drawn_mesh(world, filter), 0);
    assert_ne!(
        disunity_mesh_filter_drawn_write(world, filter, PLAYER_MESH - 1).code,
        0
    );
    assert_ne!(
        disunity_mesh_filter_drawn_write(world, 12345, PLAYER_MESH).code,
        0
    );
}

fn drawn_mesh(world: u64, filter: u64) -> u64 {
    let mut identity = 99;
    let result = unsafe { disunity_mesh_filter_drawn_read(world, filter, &mut identity) };
    assert_eq!(result.code, 0);
    identity
}
