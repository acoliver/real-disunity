//! The canvas renderer record: creation, round trip of every field, rejection, materials and the
//! inherited alpha.
use worldrunner_disunity_ffi::{
    abi::*, canvas_group::*, canvas_renderer::*, hierarchy::*, scheduling::*, ui::*,
};

fn renderer_world() -> (u64, u64, u64) {
    let (mut world, mut object, mut renderer) = (0, 0, 0);
    unsafe {
        assert_eq!(disunity_world_create(&mut world).code, 0);
        assert_eq!(disunity_gameobject_create(world, &mut object).code, 0);
        assert_eq!(
            disunity_canvas_renderer_create(world, object, &mut renderer).code,
            0
        );
    }
    (world, object, renderer)
}

fn read(world: u64, renderer: u64) -> DisunityCanvasRenderer {
    let mut value = DisunityCanvasRenderer::default();
    assert_eq!(
        unsafe { disunity_canvas_renderer_read(world, renderer, &mut value) }.code,
        0
    );
    value
}

fn write(world: u64, renderer: u64, value: DisunityCanvasRenderer) -> DisunityResult {
    unsafe { disunity_canvas_renderer_write(world, renderer, &value) }
}

fn bits(value: DisunityCanvasRenderer) -> ([u64; 3], [u32; 17]) {
    let floats = [
        value.r,
        value.g,
        value.b,
        value.a,
        value.alpha,
        value.clip_x,
        value.clip_y,
        value.clip_width,
        value.clip_height,
        value.softness_x,
        value.softness_y,
    ];
    let mut words = [0; 17];
    for (word, float) in words.iter_mut().zip(floats) {
        *word = float.to_bits();
    }
    words[11..].copy_from_slice(&[
        value.cull,
        value.cull_transparent_mesh,
        value.has_pop_instruction,
        value.has_rect_clipping,
        value.material_count,
        value.pop_material_count,
    ]);
    ([value.mesh, value.texture, value.alpha_texture], words)
}

fn distinct() -> DisunityCanvasRenderer {
    DisunityCanvasRenderer {
        mesh: 11,
        texture: 12,
        alpha_texture: 13,
        r: 0.1,
        g: 0.2,
        b: 0.3,
        a: 0.4,
        alpha: 0.5,
        cull: 1,
        cull_transparent_mesh: 1,
        has_pop_instruction: 1,
        has_rect_clipping: 1,
        clip_x: 1.,
        clip_y: 2.,
        clip_width: 3.,
        clip_height: 4.,
        softness_x: 5.,
        softness_y: 6.,
        material_count: 3,
        pop_material_count: 2,
    }
}

fn material(world: u64, renderer: u64, pop: u32, index: u32) -> Option<u64> {
    let mut id = u64::MAX;
    let result =
        unsafe { disunity_canvas_renderer_material_read(world, renderer, pop, index, &mut id) };
    (result.code == 0).then_some(id)
}

#[test]
fn a_new_renderer_has_the_engine_defaults() {
    let (world, _, renderer) = renderer_world();
    let value = read(world, renderer);
    assert_eq!(bits(value), bits(DisunityCanvasRenderer::default()));
    assert_eq!([value.r, value.g, value.b, value.a, value.alpha], [1.; 5]);
    assert_eq!(
        (
            value.cull,
            value.cull_transparent_mesh,
            value.has_rect_clipping
        ),
        (0, 0, 0)
    );
    assert_eq!((value.material_count, value.mesh), (0, 0));
}

#[test]
fn every_field_round_trips() {
    let (world, _, renderer) = renderer_world();
    assert_eq!(write(world, renderer, distinct()).code, 0);
    assert_eq!(bits(read(world, renderer)), bits(distinct()));
}

#[test]
fn a_cleared_clip_flag_reads_back_cleared() {
    let (world, _, renderer) = renderer_world();
    assert_eq!(write(world, renderer, distinct()).code, 0);
    let off = DisunityCanvasRenderer {
        has_rect_clipping: 0,
        ..distinct()
    };
    assert_eq!(write(world, renderer, off).code, 0);
    assert_eq!(read(world, renderer).has_rect_clipping, 0);
}

#[test]
fn nan_alpha_booleans_above_one_and_big_counts_are_rejected_and_change_nothing() {
    let (world, _, renderer) = renderer_world();
    assert_eq!(write(world, renderer, distinct()).code, 0);
    let bad = [
        DisunityCanvasRenderer {
            alpha: f32::NAN,
            ..distinct()
        },
        DisunityCanvasRenderer {
            cull: 2,
            ..distinct()
        },
        DisunityCanvasRenderer {
            cull_transparent_mesh: 2,
            ..distinct()
        },
        DisunityCanvasRenderer {
            has_pop_instruction: 2,
            ..distinct()
        },
        DisunityCanvasRenderer {
            has_rect_clipping: 2,
            ..distinct()
        },
        DisunityCanvasRenderer {
            material_count: 9,
            ..distinct()
        },
        DisunityCanvasRenderer {
            pop_material_count: 9,
            ..distinct()
        },
    ];
    for (index, value) in bad.into_iter().enumerate() {
        assert_ne!(write(world, renderer, value).code, 0, "case {index}");
        assert_eq!(
            bits(read(world, renderer)),
            bits(distinct()),
            "case {index}"
        );
    }
    let eight = DisunityCanvasRenderer {
        material_count: 8,
        pop_material_count: 8,
        ..distinct()
    };
    assert_eq!(write(world, renderer, eight).code, 0);
}

#[test]
fn materials_are_stored_by_slot_and_cleared_past_a_lowered_count() {
    let (world, _, renderer) = renderer_world();
    assert_eq!(write(world, renderer, distinct()).code, 0);
    for index in 0..3 {
        let result = disunity_canvas_renderer_material_write(
            world,
            renderer,
            0,
            index,
            100 + u64::from(index),
        );
        assert_eq!(result.code, 0);
    }
    assert_eq!(
        disunity_canvas_renderer_material_write(world, renderer, 1, 1, 201).code,
        0
    );
    assert_eq!(material(world, renderer, 0, 2), Some(102));
    assert_eq!(material(world, renderer, 1, 1), Some(201));
    assert_eq!(
        material(world, renderer, 1, 0),
        Some(0),
        "pop materials are separate"
    );
    assert_eq!(material(world, renderer, 0, 3), None, "past the count");
    assert_ne!(
        disunity_canvas_renderer_material_write(world, renderer, 0, 3, 5).code,
        0
    );
    assert_ne!(
        disunity_canvas_renderer_material_write(world, renderer, 2, 0, 5).code,
        0
    );
    let fewer = DisunityCanvasRenderer {
        material_count: 1,
        ..distinct()
    };
    assert_eq!(write(world, renderer, fewer).code, 0);
    assert_eq!(material(world, renderer, 0, 0), Some(100));
    assert_eq!(write(world, renderer, distinct()).code, 0);
    assert_eq!(
        material(world, renderer, 0, 2),
        Some(0),
        "the dropped slot was cleared"
    );
    assert_eq!(material(world, renderer, 1, 1), Some(201));
}

fn parent(world: u64, child: u64, parent: u64) {
    let mut work = DisunityAdvance::default();
    assert_eq!(
        unsafe { disunity_transform_parent(world, child, parent, 0, &mut work) }.code,
        0
    );
}

#[test]
fn the_inherited_alpha_includes_the_groups_above() {
    let (world, object, renderer) = renderer_world();
    let (mut top, mut group) = (0, 0);
    unsafe {
        assert_eq!(disunity_gameobject_create(world, &mut top).code, 0);
        assert_eq!(disunity_canvas_group_create(world, top, &mut group).code, 0);
        let half = DisunityCanvasGroup {
            alpha: 0.5,
            ..DisunityCanvasGroup::default()
        };
        assert_eq!(disunity_canvas_group_write(world, group, &half).code, 0);
    }
    parent(world, object, top);
    let value = DisunityCanvasRenderer {
        alpha: 0.5,
        ..DisunityCanvasRenderer::default()
    };
    assert_eq!(write(world, renderer, value).code, 0);
    let mut alpha = 0f32;
    assert_eq!(
        unsafe { disunity_canvas_renderer_inherited_alpha(world, renderer, &mut alpha) }.code,
        0
    );
    assert!((alpha - 0.25).abs() < 1e-6, "alpha {alpha}");
}

#[test]
fn null_pointers_and_other_components_are_rejected() {
    let (world, object, renderer) = renderer_world();
    unsafe {
        assert_ne!(
            disunity_canvas_renderer_create(world, object, std::ptr::null_mut()).code,
            0
        );
        assert_ne!(
            disunity_canvas_renderer_read(world, renderer, std::ptr::null_mut()).code,
            0
        );
        assert_ne!(
            disunity_canvas_renderer_write(world, renderer, std::ptr::null()).code,
            0
        );
        assert_ne!(
            disunity_canvas_renderer_material_read(world, renderer, 0, 0, std::ptr::null_mut())
                .code,
            0
        );
        assert_ne!(
            disunity_canvas_renderer_inherited_alpha(world, renderer, std::ptr::null_mut()).code,
            0
        );
    }
    let mut again = 0;
    assert_ne!(
        unsafe { disunity_canvas_renderer_create(world, object, &mut again) }.code,
        0,
        "one renderer per object"
    );
    let mut canvas = 0;
    assert_eq!(
        unsafe { disunity_ui_create(world, object, 1, 0, &mut canvas) }.code,
        0
    );
    let mut value = DisunityCanvasRenderer::default();
    assert_ne!(
        unsafe { disunity_canvas_renderer_read(world, canvas, &mut value) }.code,
        0
    );
}
