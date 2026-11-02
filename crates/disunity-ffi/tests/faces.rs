//! Font face calls: loading from bytes, measurements, glyph drawing into an atlas, and the
//! validation of every pointer, length, identity and rectangle.
#[path = "../../world-simulation/tests/ui_font/mod.rs"]
mod test_font;

use worldrunner_disunity_ffi::{abi::*, faces::*};

const OK: i32 = result_code::OK;
const INVALID: i32 = result_code::INVALID_ARGUMENT;

fn world() -> u64 {
    let mut world = 0;
    assert_eq!(unsafe { disunity_world_create(&mut world) }.code, OK);
    world
}

fn load(world: u64, size: f32) -> u64 {
    let bytes = test_font::font();
    let mut face = u64::MAX;
    let length = u32::try_from(bytes.len()).unwrap();
    let result = unsafe { disunity_face_load(world, bytes.as_ptr(), length, 0, size, &mut face) };
    assert_eq!(result.code, OK);
    face
}

fn render_call(world: u64, face: u64, render: &DisunityGlyphRender, atlas: &mut [u8]) -> i32 {
    let length = u32::try_from(atlas.len()).unwrap();
    unsafe { disunity_face_glyph_render(world, face, render, atlas.as_mut_ptr(), length) }.code
}

fn job(mode: u32) -> DisunityGlyphRender {
    DisunityGlyphRender {
        glyph: 1,
        mode,
        padding: 2,
        x: 1,
        y: 1,
        width: 54,
        height: 74,
        atlas_width: 64,
        atlas_height: 80,
    }
}

#[test]
fn a_loaded_face_reports_its_info_and_names() {
    let world = world();
    let face = load(world, 100.);
    let mut info = DisunityFaceInfo::default();
    assert_eq!(
        unsafe { disunity_face_info(world, face, &mut info) }.code,
        OK
    );
    assert_eq!(info.units_per_em, 1000);
    assert!((info.point_size - 100.).abs() < 1e-3);
    assert!((info.ascent_line - 80.).abs() < 1e-3);
    let mut length = 99;
    assert_eq!(
        unsafe { disunity_face_name(world, face, 0, std::ptr::null_mut(), 0, &mut length) }.code,
        OK
    );
    assert_eq!(length, 0, "the test font has no name table");
}

#[test]
fn a_loaded_face_maps_characters_and_scales_metrics() {
    let world = world();
    let face = load(world, 100.);
    let mut glyph = 77;
    assert_eq!(
        unsafe { disunity_face_glyph_index(world, face, 65, &mut glyph) }.code,
        OK
    );
    assert_eq!(glyph, 1);
    assert_eq!(
        unsafe { disunity_face_glyph_index(world, face, 0x4E00, &mut glyph) }.code,
        OK
    );
    assert_eq!(glyph, 0);
    let mut metrics = DisunityGlyphMetrics::default();
    assert_eq!(
        unsafe { disunity_face_glyph_metrics(world, face, 1, &mut metrics) }.code,
        OK
    );
    assert!((metrics.width - 50.).abs() < 1e-3 && (metrics.advance - 60.).abs() < 1e-3);
    assert_eq!(disunity_face_set_size(world, face, 50.).code, OK);
    assert_eq!(
        unsafe { disunity_face_glyph_metrics(world, face, 1, &mut metrics) }.code,
        OK
    );
    assert!((metrics.width - 25.).abs() < 1e-3);
}

#[test]
fn an_unloaded_face_answers_nothing() {
    let world = world();
    let face = load(world, 100.);
    let mut glyph = 77;
    assert_eq!(disunity_face_unload(world, face).code, OK);
    assert_ne!(disunity_face_unload(world, face).code, OK);
    assert_ne!(
        unsafe { disunity_face_glyph_index(world, face, 65, &mut glyph) }.code,
        OK
    );
}

#[test]
fn loading_validates_pointers_lengths_and_font_bytes() {
    let world = world();
    let bytes = test_font::font();
    let length = u32::try_from(bytes.len()).unwrap();
    let mut face = 0;
    let load = |bytes: *const u8, length, index, size, out: *mut u64| unsafe {
        disunity_face_load(world, bytes, length, index, size, out).code
    };
    assert_eq!(load(std::ptr::null(), length, 0, 10., &mut face), INVALID);
    assert_eq!(
        load(bytes.as_ptr(), length, 0, 10., std::ptr::null_mut()),
        INVALID
    );
    assert_eq!(load(bytes.as_ptr(), 0, 0, 10., &mut face), INVALID);
    assert_eq!(load(bytes.as_ptr(), u32::MAX, 0, 10., &mut face), INVALID);
    assert_ne!(load(bytes.as_ptr(), 3, 0, 10., &mut face), OK, "not a font");
    assert_ne!(
        load(bytes.as_ptr(), length, 0, 0., &mut face),
        OK,
        "zero size"
    );
    assert_ne!(
        load(bytes.as_ptr(), length, 0, f32::NAN, &mut face),
        OK,
        "NaN size"
    );
    assert_ne!(
        load(bytes.as_ptr(), length, 5, 10., &mut face),
        OK,
        "no such face"
    );
    assert_ne!(
        unsafe { disunity_face_load(world + 99, bytes.as_ptr(), length, 0, 10., &mut face) }.code,
        OK
    );
    assert_ne!(
        disunity_face_set_size(world, 4_000_000_000_000, 10.).code,
        OK
    );
}

#[test]
fn a_face_id_with_high_bits_is_not_a_face_of_this_world() {
    let world = world();
    let face = load(world, 20.);
    let high = face | (1 << 32);
    let mut info = DisunityFaceInfo::default();
    assert_ne!(
        unsafe { disunity_face_info(world, high, &mut info) }.code,
        OK
    );
    assert_ne!(disunity_face_set_size(world, high, 10.).code, OK);
    assert_ne!(disunity_face_unload(world, high).code, OK);
    assert_eq!(disunity_face_unload(world, face).code, OK);
}

#[test]
fn queries_reject_null_outputs_and_unknown_faces() {
    let world = world();
    let face = load(world, 20.);
    let mut length = 0_u32;
    unsafe {
        assert_eq!(
            disunity_face_info(world, face, std::ptr::null_mut()).code,
            INVALID
        );
        assert_eq!(
            disunity_face_glyph_index(world, face, 65, std::ptr::null_mut()).code,
            INVALID
        );
        assert_eq!(
            disunity_face_glyph_metrics(world, face, 1, std::ptr::null_mut()).code,
            INVALID
        );
        assert_eq!(
            disunity_face_name(
                world,
                face,
                0,
                std::ptr::null_mut(),
                0,
                std::ptr::null_mut()
            )
            .code,
            INVALID
        );
        assert_eq!(
            disunity_face_name(world, face, 2, std::ptr::null_mut(), 0, &mut length).code,
            INVALID
        );
        assert_eq!(
            disunity_face_name(world, face, 0, std::ptr::null_mut(), 4, &mut length).code,
            INVALID
        );
        let mut metrics = DisunityGlyphMetrics::default();
        assert_ne!(
            disunity_face_glyph_metrics(world, face, 77, &mut metrics).code,
            OK
        );
        assert_ne!(
            disunity_face_glyph_metrics(world, face + 1, 1, &mut metrics).code,
            OK
        );
    }
}

#[test]
fn a_glyph_renders_into_the_caller_atlas_and_leaves_the_rest() {
    let world = world();
    let face = load(world, 100.);
    let mut atlas = vec![9_u8; 64 * 80];
    let render = DisunityGlyphRender {
        padding: 2,
        ..job(0)
    };
    // The rectangle is 54 by 74 at (1, 1): a 50 by 70 glyph and 2 pixels of padding.
    assert_eq!(render_call(world, face, &render, &mut atlas), OK);
    let at = |x: usize, y: usize| atlas[y * 64 + x];
    assert!(at(1 + 2 + 3, 1 + 2 + 3) > 240, "inside the triangle");
    assert_eq!(at(1 + 2 + 45, 1 + 2 + 65), 0, "outside the triangle");
    assert_eq!(at(1, 1), 0, "padding");
    assert_eq!(at(0, 0), 9, "outside the rectangle");
    assert_eq!(at(60, 70), 9, "outside the rectangle");
    let sdf = DisunityGlyphRender {
        padding: 4,
        width: 58,
        height: 78,
        ..job(1)
    };
    let mut field = vec![9_u8; 64 * 80];
    assert_eq!(render_call(world, face, &sdf, &mut field), OK);
    let edge = field[(1 + 4 + 20) * 64 + 1 + 4];
    assert!((100..=160).contains(&edge), "edge {edge}");
}

#[test]
fn rendering_rejects_bad_modes_pointers_rectangles_and_lengths() {
    let world = world();
    let face = load(world, 100.);
    let mut atlas = vec![0_u8; 64 * 80];
    assert_eq!(render_call(world, face, &job(2), &mut atlas), INVALID);
    let length = u32::try_from(atlas.len()).unwrap();
    unsafe {
        let render = job(0);
        assert_eq!(
            disunity_face_glyph_render(world, face, std::ptr::null(), atlas.as_mut_ptr(), length)
                .code,
            INVALID
        );
        assert_eq!(
            disunity_face_glyph_render(world, face, &render, std::ptr::null_mut(), length).code,
            INVALID
        );
        assert_eq!(
            disunity_face_glyph_render(world, face, &render, atlas.as_mut_ptr(), u32::MAX).code,
            INVALID
        );
    }
    let outside = DisunityGlyphRender { x: 20, ..job(0) };
    assert_ne!(render_call(world, face, &outside, &mut atlas), OK);
    let short = DisunityGlyphRender {
        atlas_height: 90,
        ..job(0)
    };
    assert_ne!(
        render_call(world, face, &short, &mut atlas),
        OK,
        "claimed atlas longer than the buffer"
    );
    let bad_glyph = DisunityGlyphRender {
        glyph: 50,
        ..job(0)
    };
    assert_ne!(render_call(world, face, &bad_glyph, &mut atlas), OK);
    assert_ne!(
        render_call(world, face + 1, &job(0), &mut atlas),
        OK,
        "unknown face"
    );
    assert_eq!(
        atlas.iter().filter(|&&b| b != 0).count(),
        0,
        "failed calls write nothing"
    );
}

#[test]
fn pairs_validate_pointers_and_report_counts() {
    let world = world();
    let face = load(world, 100.);
    let glyphs = [0_u32, 1];
    let mut count = 0_u32;
    unsafe {
        let call = |glyphs: *const u32, n, records: *mut DisunityGlyphPair, count: *mut u32| {
            disunity_face_pairs(world, face, glyphs, n, records, count).code
        };
        assert_eq!(
            call(glyphs.as_ptr(), 2, std::ptr::null_mut(), &mut count),
            OK
        );
        assert_eq!(count, 0, "the test font has no kern table");
        assert_eq!(
            call(
                glyphs.as_ptr(),
                2,
                std::ptr::null_mut(),
                std::ptr::null_mut()
            ),
            INVALID
        );
        assert_eq!(
            call(std::ptr::null(), 2, std::ptr::null_mut(), &mut count),
            INVALID
        );
        count = 4;
        assert_eq!(
            call(glyphs.as_ptr(), 2, std::ptr::null_mut(), &mut count),
            INVALID
        );
        assert_ne!(
            disunity_face_pairs(
                world,
                face + 1,
                glyphs.as_ptr(),
                2,
                std::ptr::null_mut(),
                &mut 0
            )
            .code,
            OK
        );
    }
}
