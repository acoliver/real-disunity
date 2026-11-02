#![cfg(feature = "ui")]

mod ui_font;
use worldrunner_world_simulation::ui::{
    FaceBook, FaceLimits, GlyphJob, GlyphRect, RenderMode, UiError,
};

/// A `kern` table of one horizontal format 0 subtable holding `pairs` (left, right, units).
pub fn kern(pairs: &[(u16, u16, i16)]) -> Vec<u8> {
    let count = u16::try_from(pairs.len()).unwrap();
    let mut table = Vec::new();
    for n in [0_u16, 1, 0, 14 + count * 6, 1, count, 0, 0, 0] {
        table.extend(n.to_be_bytes());
    }
    for &(left, right, value) in pairs {
        table.extend(left.to_be_bytes());
        table.extend(right.to_be_bytes());
        table.extend(value.to_be_bytes());
    }
    table
}

fn loaded(size: f32) -> (FaceBook, worldrunner_world_simulation::ui::FaceId) {
    let mut book = FaceBook::default();
    let id = book.load(ui_font::font(), 0, size).unwrap();
    (book, id)
}

/// The test font's glyph 1 is a right triangle with its right angle at the bottom left corner,
/// 500 wide and 700 tall in 1000 units per em.
fn render(mode: RenderMode, size: f32, padding: u32) -> (Vec<u8>, GlyphRect, u32) {
    let (book, id) = loaded(size);
    // At 100 points the triangle is 50 by 70 pixels.
    let (w, h) = (50 + 2 * padding, 70 + 2 * padding);
    let (atlas_w, atlas_h) = (w + 6, h + 6);
    let rect = GlyphRect {
        x: 3,
        y: 3,
        width: w,
        height: h,
    };
    let mut atlas = vec![7_u8; (atlas_w * atlas_h) as usize];
    let job = GlyphJob {
        glyph: 1,
        mode,
        padding,
        rect,
    };
    book.render_glyph(id, job, &mut atlas, atlas_w, atlas_h)
        .unwrap();
    (atlas, rect, atlas_w)
}

fn at(atlas: &[u8], stride: u32, x: u32, y: u32) -> u8 {
    atlas[(y * stride + x) as usize]
}

#[test]
fn a_face_loads_from_bytes_and_reports_its_measurements() {
    let (book, id) = loaded(100.);
    let info = book.face_info(id).unwrap();
    assert_eq!(info.units_per_em, 1000);
    assert!((info.point_size - 100.).abs() < 1e-3);
    assert!((info.ascent_line - 80.).abs() < 1e-3);
    assert!((info.descent_line + 20.).abs() < 1e-3);
    assert!((info.line_height - 100.).abs() < 1e-3);
    assert!(info.baseline.abs() < 1e-6);
    assert!(info.cap_line > 0. && info.mean_line > 0.);
    assert!(info.underline_thickness > 0. && info.strikethrough_thickness > 0.);
    assert!(info.superscript_size > 0. && info.subscript_size > 0.);
    assert!(info.tab_width > 0.);
}

#[test]
fn garbage_bytes_and_bad_sizes_are_rejected() {
    let mut book = FaceBook::default();
    assert_eq!(book.load(vec![1, 2, 3], 0, 12.), Err(UiError::InvalidFont));
    assert_eq!(book.load(ui_font::font(), 0, 0.), Err(UiError::InvalidFont));
    assert_eq!(
        book.load(ui_font::font(), 0, f32::NAN),
        Err(UiError::InvalidFont)
    );
    assert_eq!(
        book.load(ui_font::font(), 3, 12.),
        Err(UiError::InvalidFont)
    );
}

#[test]
fn glyph_index_maps_code_points_and_zero_means_unmapped() {
    let (book, id) = loaded(50.);
    assert_eq!(book.glyph_index(id, u32::from('A')), Ok(1));
    assert_eq!(book.glyph_index(id, 0x4E00), Ok(0));
    assert_eq!(book.glyph_index(id, 0xD800), Ok(0));
}

#[test]
fn glyph_metrics_scale_with_point_size() {
    let (mut book, id) = loaded(100.);
    let big = book.glyph_metrics(id, 1).unwrap();
    assert!((big.width - 50.).abs() < 1e-3);
    assert!((big.height - 70.).abs() < 1e-3);
    assert!(big.bearing_x.abs() < 1e-3);
    assert!((big.bearing_y - 70.).abs() < 1e-3);
    assert!((big.advance - 60.).abs() < 1e-3);
    book.set_size(id, 50.).unwrap();
    let small = book.glyph_metrics(id, 1).unwrap();
    assert!((small.width * 2. - big.width).abs() < 1e-3);
    assert!((small.advance * 2. - big.advance).abs() < 1e-3);
    assert!((book.face_info(id).unwrap().point_size - 50.).abs() < 1e-3);
    assert_eq!(book.glyph_metrics(id, 9), Err(UiError::InvalidGlyph));
    let empty = book.glyph_metrics(id, 0).unwrap();
    assert!(empty.width.abs() < 1e-6 && empty.height.abs() < 1e-6);
}

#[test]
fn an_unknown_face_is_an_error_and_an_unloaded_face_is_gone() {
    let (mut book, id) = loaded(20.);
    book.unload(id).unwrap();
    assert_eq!(book.glyph_index(id, 65), Err(UiError::MissingFace));
    assert_eq!(book.set_size(id, 10.), Err(UiError::MissingFace));
    assert_eq!(book.unload(id), Err(UiError::MissingFace));
}

#[test]
fn coverage_is_high_inside_the_outline_and_zero_outside_and_in_the_padding() {
    let (atlas, rect, stride) = render(RenderMode::Coverage, 100., 2);
    // Atlas rows run bottom to top: row `rect.y + padding` is the glyph's lowest pixel row.
    let (left, bottom) = (rect.x + 2, rect.y + 2);
    assert!(at(&atlas, stride, left + 3, bottom + 3) > 240);
    assert_eq!(at(&atlas, stride, left + 45, bottom + 65), 0);
    assert_eq!(at(&atlas, stride, rect.x, rect.y), 0);
    assert_eq!(at(&atlas, stride, rect.x + rect.width - 1, rect.y + 1), 0);
    // Pixels outside the rectangle are untouched.
    assert_eq!(at(&atlas, stride, 0, 0), 7);
    assert_eq!(at(&atlas, stride, rect.x + rect.width, rect.y), 7);
    assert_eq!(at(&atlas, stride, rect.x, rect.y + rect.height), 7);
}

#[test]
fn the_glyph_top_lands_on_the_highest_rows() {
    let (atlas, rect, stride) = render(RenderMode::Coverage, 100., 2);
    // The triangle is widest at its bottom, so the lower rows hold more coverage than the upper.
    let row_sum = |y: u32| -> u32 {
        (rect.x..rect.x + rect.width)
            .map(|x| u32::from(at(&atlas, stride, x, y)))
            .sum()
    };
    assert!(row_sum(rect.y + 4) > row_sum(rect.y + rect.height - 6) * 3);
}

#[test]
fn a_signed_distance_field_is_half_at_the_edge_and_high_inside() {
    let (atlas, rect, stride) = render(RenderMode::Sdf, 100., 4);
    let (left, bottom) = (rect.x + 4, rect.y + 4);
    let inside = at(&atlas, stride, left + 4, bottom + 4);
    let edge_in = at(&atlas, stride, left, bottom + 20);
    let edge_out = at(&atlas, stride, left - 1, bottom + 20);
    let far_out = at(&atlas, stride, left + 45, bottom + 65);
    assert!(inside > 200, "inside {inside}");
    assert!((145..=150).contains(&edge_in), "edge inside {edge_in}");
    assert!((114..=118).contains(&edge_out), "edge outside {edge_out}");
    assert!(edge_in > edge_out);
    assert!(far_out < 40, "far outside {far_out}");
    assert_eq!(at(&atlas, stride, rect.x, rect.y), 0);
}

#[test]
fn the_distance_grows_with_depth_up_to_the_spread() {
    let (atlas, rect, stride) = render(RenderMode::Sdf, 100., 4);
    let (left, bottom) = (rect.x + 4, rect.y + 4);
    let shallow = at(&atlas, stride, left + 1, bottom + 20);
    let deeper = at(&atlas, stride, left + 3, bottom + 20);
    assert!(deeper > shallow);
    assert_eq!(at(&atlas, stride, left + 8, bottom + 20), 255);
}

#[test]
fn rectangles_outside_the_atlas_or_with_too_much_padding_are_rejected() {
    let (book, id) = loaded(100.);
    let mut atlas = vec![0_u8; 64 * 64];
    let job = |x, y, width, height, padding| GlyphJob {
        glyph: 1,
        mode: RenderMode::Coverage,
        padding,
        rect: GlyphRect {
            x,
            y,
            width,
            height,
        },
    };
    for bad in [
        job(60, 0, 8, 8, 0),
        job(0, 60, 8, 8, 0),
        job(0, 0, 0, 8, 0),
        job(0, 0, 8, 8, 4),
        job(u32::MAX, 0, 8, 8, 0),
    ] {
        assert_eq!(
            book.render_glyph(id, bad, &mut atlas, 64, 64),
            Err(UiError::AtlasLimit),
            "{bad:?}"
        );
    }
    assert_eq!(
        book.render_glyph(id, job(0, 0, 8, 8, 0), &mut atlas[..2048], 64, 64),
        Err(UiError::AtlasLimit),
        "buffer shorter than the atlas it claims"
    );
    assert_eq!(
        book.render_glyph(
            id,
            GlyphJob {
                glyph: 99,
                ..job(0, 0, 8, 8, 0)
            },
            &mut atlas,
            64,
            64
        ),
        Err(UiError::InvalidGlyph)
    );
}

#[test]
fn a_font_without_pair_tables_has_no_adjustments() {
    let (book, id) = loaded(100.);
    assert!(book.pair_adjustments(id, &[0, 1]).unwrap().is_empty());
}

#[test]
fn kern_pairs_become_scaled_advance_adjustments_in_both_orders_asked() {
    let glyphs = [Vec::new(), ui_font::glyph(1), ui_font::glyph(1)];
    let bytes = ui_font::assemble_with(
        &glyphs,
        800,
        -200,
        vec![(*b"kern", kern(&[(1, 2, -80), (2, 1, 40)]))],
    );
    let mut book = FaceBook::default();
    let id = book.load(bytes, 0, 50.).unwrap();
    let pairs = book.pair_adjustments(id, &[2, 1, 0, 1, 2]).unwrap();
    assert_eq!(pairs.len(), 2);
    let find = |first, second| {
        pairs
            .iter()
            .find(|p| p.first == first && p.second == second)
            .map(|p| p.x_advance)
    };
    assert!((find(1, 2).unwrap() + 4.).abs() < 1e-3);
    assert!((find(2, 1).unwrap() - 2.).abs() < 1e-3);
    assert_eq!(find(1, 1), None);
    assert!(book.pair_adjustments(id, &[1]).unwrap().is_empty());
    assert_eq!(
        book.pair_adjustments(worldrunner_world_simulation::ui::FaceId(99), &[1]),
        Err(UiError::MissingFace)
    );
}

#[test]
fn unloading_a_face_frees_its_share_of_the_limits() {
    let bytes = ui_font::font();
    let limits = FaceLimits {
        max_faces: 1,
        max_face_bytes: bytes.len(),
        max_total_bytes: bytes.len() + 10,
    };
    let mut book = FaceBook::new(limits);
    let id = book.load(bytes.clone(), 0, 12.).unwrap();
    assert_eq!(book.load(bytes.clone(), 0, 12.), Err(UiError::FontLimit));
    book.unload(id).unwrap();
    let again = book.load(bytes.clone(), 0, 12.).unwrap();
    assert_ne!(again, id, "ids are not reused");
    let mut bytes_limit = FaceBook::new(FaceLimits {
        max_faces: 4,
        ..limits
    });
    let first = bytes_limit.load(bytes.clone(), 0, 12.).unwrap();
    assert_eq!(
        bytes_limit.load(bytes.clone(), 0, 12.),
        Err(UiError::FontLimit)
    );
    bytes_limit.unload(first).unwrap();
    bytes_limit.load(bytes.clone(), 0, 12.).unwrap();
    let mut small = FaceBook::new(FaceLimits {
        max_face_bytes: bytes.len() - 1,
        ..limits
    });
    assert_eq!(small.load(bytes, 0, 12.), Err(UiError::InvalidFont));
}

#[test]
fn the_padding_is_the_spread_of_the_distance_field() {
    let (atlas, rect, stride) = render(RenderMode::Sdf, 100., 2);
    // One pixel and a bit inside the vertical edge: 1.625 pixels from it at the sampled point,
    // 0.906 of the way across a spread of 2.
    let value = at(&atlas, stride, rect.x + 2 + 1, rect.y + 2 + 20);
    assert!((228..=234).contains(&value), "value {value}");
}
