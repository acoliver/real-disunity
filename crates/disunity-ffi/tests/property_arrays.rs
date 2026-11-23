//! Vector and matrix array globals through the ABI: the shader setters validate their shape before
//! they read, copy their input before they return and change nothing when they refuse. The world
//! orders each write in front of the next offscreen record.
#[path = "common/frame.rs"]
mod frame;

use worldrunner_disunity_ffi::{
    abi::*, graphics::*, graphics_commands::*, graphics_offscreen::*, graphics_properties::*,
    with_native_world,
};
use worldrunner_world_simulation::graphics::{
    GraphicsSnapshot, OffscreenWork, PropertyValue, ShaderProperties,
};

type Setter = unsafe extern "C" fn(u64, *const u8, u32, *const f32, u32) -> DisunityResult;

const SETTERS: [(Setter, usize); 2] = [
    (disunity_shader_global_vector_array_write, 4),
    (disunity_shader_global_matrix_array_write, 16),
];

fn new_world() -> u64 {
    let mut world = 0;
    unsafe { assert_eq!(disunity_world_create(&mut world).code, 0) };
    world
}

fn text(name: &str) -> (*const u8, u32) {
    (name.as_ptr(), u32::try_from(name.len()).unwrap())
}

/// `count` elements of `width` floats where element `e` slot `s` is `e * 100 + s`.
fn floats_of(count: usize, width: usize) -> Vec<f32> {
    (0..count * width)
        .map(|index| (index / width * 100 + index % width) as f32)
        .collect()
}

fn call(setter: Setter, world: u64, name: &str, count: u32, values: &[f32]) -> DisunityResult {
    let (name, length) = text(name);
    unsafe { setter(world, name, length, values.as_ptr(), count) }
}

fn render_texture(world: u64) -> u64 {
    let desc = DisunityRenderTexture {
        width: 8,
        height: 8,
        format: 8,
        mip_count: 1,
        filter: 1,
        aniso_level: 1,
        dimension: 2,
        volume_depth: 1,
        ..Default::default()
    };
    let mut id = 0;
    unsafe {
        assert_eq!(
            disunity_render_texture_create(world, &desc, &mut id).code,
            0
        )
    };
    id
}

/// The snapshot of a world after it records one blit, which makes the snapshot carry the global
/// state, and the ordered work the world holds.
fn snapshot_after_a_blit(world: u64) -> GraphicsSnapshot {
    let target = render_texture(world);
    let record = DisunityOffscreenWork {
        kind: 1,
        target,
        pass: -1,
        ..Default::default()
    };
    unsafe { assert_eq!(disunity_offscreen_record(world, &record).code, 0) };
    frame::snapshot(world)
}

/// The values of the world's global state. A snapshot replays from the state before its first
/// write, so the writes of its own frame are ordered work and not part of that state.
fn global_properties(world: u64) -> ShaderProperties {
    with_native_world(world, |native| {
        native.global_shader_state().properties.clone()
    })
    .unwrap()
}

fn vectors(floats: &[f32]) -> Vec<[f32; 4]> {
    floats
        .chunks_exact(4)
        .map(|chunk| <[f32; 4]>::try_from(chunk).unwrap())
        .collect()
}

fn matrices(floats: &[f32]) -> Vec<[f32; 16]> {
    floats
        .chunks_exact(16)
        .map(|chunk| <[f32; 16]>::try_from(chunk).unwrap())
        .collect()
}

#[test]
fn the_shader_setters_store_ordered_elements_in_the_global_state() {
    let world = new_world();
    let vector_values = floats_of(3, 4);
    let matrix_values = floats_of(2, 16);
    for (setter, values, count) in [
        (SETTERS[0].0, &vector_values, 3),
        (SETTERS[1].0, &matrix_values, 2),
    ] {
        assert_eq!(call(setter, world, "_Arr", count, values).code, 0);
    }
    let state = global_properties(world);
    assert_eq!(state.vector_array("_Arr"), Some(vectors(&vector_values)));
    assert_eq!(state.matrix_array("_Arr"), Some(matrices(&matrix_values)));
    let held = state.matrix_array("_Arr").unwrap();
    assert_eq!(held[1][4], 104.0, "element 4 is column one, row zero");
}

#[test]
fn array_writes_are_ordered_in_front_of_the_next_record_with_their_last_value() {
    let world = new_world();
    let (first, second) = (floats_of(2, 4), floats_of(1, 16));
    let third = floats_of(5, 4);
    assert_eq!(call(SETTERS[0].0, world, "_Arr", 2, &first).code, 0);
    assert_eq!(call(SETTERS[1].0, world, "_Arr", 1, &second).code, 0);
    assert_eq!(call(SETTERS[0].0, world, "_Arr", 5, &third).code, 0);

    let held = global_properties(world);
    assert_eq!(held.vector_array("_Arr"), Some(vectors(&third)));
    assert_eq!(held.matrix_array("_Arr"), Some(matrices(&second)));
    let snapshot = snapshot_after_a_blit(world);
    // The replay starts from the state before the writes, which the ordered work carries.
    let start = snapshot.global.unwrap().properties;
    assert_eq!(start.vector_array("_Arr"), None);
    assert_eq!(start.matrix_array("_Arr"), None);
    assert_eq!(snapshot.work.len(), 3, "two ordered values and the blit");
    let expected = [
        PropertyValue::VectorArray("_Arr".into(), vectors(&third)),
        PropertyValue::MatrixArray("_Arr".into(), matrices(&second)),
    ];
    for (work, value) in snapshot.work.iter().zip(expected) {
        assert_eq!(*work, OffscreenWork::SetGlobal(value));
    }
}

#[test]
fn a_large_ordered_array_reaches_the_frame_in_front_of_the_record_that_follows_it() {
    let world = new_world();
    let values = floats_of(1023, 16);
    assert_eq!(call(SETTERS[1].0, world, "_Big", 1023, &values).code, 0);
    assert_eq!(disunity_offscreen_release_temporary(world, 3).code, 0);
    let work = frame::snapshot(world).work;
    assert_eq!(
        work,
        [
            OffscreenWork::SetGlobal(PropertyValue::MatrixArray("_Big".into(), matrices(&values))),
            OffscreenWork::ReleaseTemporary { name_id: 3 },
        ],
        "the ordered array and the release it precedes"
    );
}

#[test]
fn the_most_elements_are_accepted_and_one_more_is_refused_by_shape() {
    for (setter, width) in SETTERS {
        let world = new_world();
        let values = floats_of(1023, width);
        assert_eq!(call(setter, world, "_Max", 1023, &values).code, 0);
        // The buffer holds the floats a count of 1024 would read, and they are not finite, so
        // a call that read them would fail as an error instead of as an invalid argument.
        let poisoned = vec![f32::NAN; 1024 * width];
        for count in [0, 1024, u32::MAX] {
            let result = call(setter, world, "_Max", count, &poisoned);
            assert_eq!(result.code, result_code::INVALID_ARGUMENT, "count {count}");
        }
    }
}

#[test]
fn a_null_or_misaligned_pointer_is_an_invalid_argument() {
    for (setter, width) in SETTERS {
        let world = new_world();
        let (name, length) = text("_A");
        let null = unsafe { setter(world, name, length, std::ptr::null(), 1) };
        assert_eq!(null.code, result_code::INVALID_ARGUMENT);

        let storage = vec![0.0_f32; width + 2];
        let shifted = unsafe { storage.as_ptr().cast::<u8>().add(1) }.cast::<f32>();
        let misaligned = unsafe { setter(world, name, length, shifted, 1) };
        assert_eq!(misaligned.code, result_code::INVALID_ARGUMENT);
    }
}

#[test]
fn a_bad_name_is_refused_and_nothing_is_recorded_or_stored() {
    for (setter, width) in SETTERS {
        let world = new_world();
        let values = floats_of(1, width);
        let (name, _) = text("_A");
        let refused = [
            unsafe { setter(world, std::ptr::null(), 2, values.as_ptr(), 1) },
            unsafe { setter(world, name, 0, values.as_ptr(), 1) },
            unsafe { setter(world, [0xFF_u8, 0xFE].as_ptr(), 2, values.as_ptr(), 1) },
            call(setter, world, &"n".repeat(257), 1, &values),
        ];
        for result in refused {
            assert_eq!(result.code, result_code::ERROR);
            assert_eq!(result.detail, DisunityErrorReason::InvalidArgument as u64);
        }
        let state = snapshot_after_a_blit(world);
        let global = state.global.unwrap().properties;
        assert!(global.vector_arrays.is_empty() && global.matrix_arrays.is_empty());
        assert_eq!(state.work.len(), 1, "only the blit that took the snapshot");
        assert_eq!(call(setter, world, &"n".repeat(256), 1, &values).code, 0);
    }
}

#[test]
fn a_non_finite_component_anywhere_is_refused_and_nothing_changes() {
    for (setter, width) in SETTERS {
        let world = new_world();
        for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            for at in [0, width - 1, width, 2 * width - 1] {
                let mut values = floats_of(2, width);
                values[at] = bad;
                let result = call(setter, world, "_A", 2, &values);
                assert_eq!(result.code, result_code::ERROR, "component {at}");
                assert_eq!(result.detail, DisunityErrorReason::InvalidArgument as u64);
            }
        }
        let state = snapshot_after_a_blit(world);
        let global = state.global.unwrap().properties;
        assert!(global.vector_arrays.is_empty() && global.matrix_arrays.is_empty());
        assert_eq!(state.work.len(), 1);
    }
}

#[test]
fn a_stale_world_is_refused_like_any_other_call() {
    for (setter, width) in SETTERS {
        let world = new_world();
        unsafe { assert_eq!(disunity_world_destroy(world).code, 0) };
        let result = call(setter, world, "_A", 1, &floats_of(1, width));
        let reference = disunity_shader_properties_clear(world, 1, 0);
        assert_ne!(result.code, 0);
        assert_eq!(
            (result.code, result.detail),
            (reference.code, reference.detail)
        );
    }
}

#[test]
fn the_input_is_copied_before_the_call_returns() {
    let world = new_world();
    let mut vector_values = floats_of(3, 4);
    let expected_vectors = vectors(&vector_values);
    assert_eq!(call(SETTERS[0].0, world, "_V", 3, &vector_values).code, 0);
    vector_values.fill(-1.0);
    let mut matrix_values = floats_of(2, 16);
    let expected_matrices = matrices(&matrix_values);
    assert_eq!(call(SETTERS[1].0, world, "_M", 2, &matrix_values).code, 0);
    matrix_values.fill(-1.0);
    drop((vector_values, matrix_values));

    let state = snapshot_after_a_blit(world);
    let global = global_properties(world);
    assert_eq!(global.vector_array("_V"), Some(expected_vectors));
    // Both arrays are ordered ahead of the blit that took the snapshot.
    let held = state
        .work
        .iter()
        .find_map(|work| match work {
            OffscreenWork::SetGlobal(PropertyValue::MatrixArray(_, held)) => Some(held),
            _ => None,
        })
        .unwrap_or_else(|| panic!("expected the ordered matrix array in {:?}", state.work));
    assert_eq!(*held, expected_matrices);
}

#[test]
fn a_replacement_through_the_shader_setter_drops_the_old_tail() {
    let world = new_world();
    assert_eq!(call(SETTERS[0].0, world, "_V", 6, &floats_of(6, 4)).code, 0);
    let short = floats_of(2, 4);
    assert_eq!(call(SETTERS[0].0, world, "_V", 2, &short).code, 0);
    let global = global_properties(world);
    assert_eq!(global.vector_array("_V"), Some(vectors(&short)));
}
