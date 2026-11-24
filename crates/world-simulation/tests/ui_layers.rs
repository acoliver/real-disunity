#![cfg(feature = "ui")]
//! The runs of UI draws that a game's shader draws, and where each run sits in the layer atlas.
use worldrunner_world_simulation::graphics::MaterialId;
use worldrunner_world_simulation::ui::UiTexture;
use worldrunner_world_simulation::ui::presentation::{
    UiDraw, UiDrawKind, UiLayers, UiMaterial, UiPresentation,
};

const VIEWPORT: [u32; 2] = [200, 100];
/// A device limit far above what the plans of most tests need.
const LIMIT: u32 = 4096;

fn material(bits: u64) -> UiMaterial {
    UiMaterial {
        id: MaterialId::from_bits(bits),
        generation: 1,
    }
}

/// A draw of the rectangle `x, y, width, height` in lower-left viewport pixels.
fn draw(material: Option<UiMaterial>, rect: [f32; 4]) -> UiDraw {
    let [x, y, width, height] = rect;
    UiDraw {
        kind: UiDrawKind::Graphic,
        depth_test: false,
        triangle_uv: None,
        corners: [
            [x, y, 0.],
            [x + width, y, 0.],
            [x + width, y + height, 0.],
            [x, y + height, 0.],
        ],
        uv_rect: [0., 0., 1., 1.],
        color: [1.; 4],
        distance_field: None,
        corner_colors: None,
        texture: Some(UiTexture::Image(1)),
        material,
        sorting_layer: 0,
        sorting_order: 0,
        hierarchy_order: 0,
        clip_rect: [0, 0, VIEWPORT[0], VIEWPORT[1]],
    }
}

fn plan(draws: Vec<UiDraw>) -> UiPresentation {
    UiPresentation {
        draws,
        ..UiPresentation::default()
    }
}

fn layers(draws: Vec<UiDraw>) -> UiLayers {
    plan(draws).layers(VIEWPORT, LIMIT)
}

#[test]
fn consecutive_material_draws_form_one_run() {
    let (a, b) = (material(1), material(2));
    let layers = layers(vec![
        draw(Some(a), [10., 10., 20., 20.]),
        draw(Some(a), [40., 10., 20., 20.]),
        draw(Some(b), [10., 40., 30., 10.]),
    ]);
    assert_eq!(layers.runs.len(), 1);
    let run = &layers.runs[0];
    assert_eq!(run.draws, 0..3);
    assert_eq!(run.bounds, [10, 10, 50, 40]);
    let spans: Vec<_> = run.spans.iter().map(|s| (s.material, &s.draws)).collect();
    assert_eq!(spans, [(a, &(0..2)), (b, &(2..3))]);
}

#[test]
fn a_draw_of_another_image_starts_a_new_span() {
    let a = material(1);
    let mut other = draw(Some(a), [40., 10., 20., 20.]);
    other.texture = Some(UiTexture::Image(2));
    let layers = layers(vec![draw(Some(a), [10., 10., 20., 20.]), other]);
    assert_eq!(layers.runs.len(), 1);
    let spans: Vec<_> = layers.runs[0]
        .spans
        .iter()
        .map(|s| (s.texture, s.draws.clone()))
        .collect();
    assert_eq!(
        spans,
        [
            (Some(UiTexture::Image(1)), 0..1),
            (Some(UiTexture::Image(2)), 1..2)
        ]
    );
}

#[test]
fn a_default_draw_between_two_material_draws_splits_them() {
    let a = material(1);
    let layers = layers(vec![
        draw(Some(a), [0., 0., 10., 10.]),
        draw(None, [0., 0., 50., 50.]),
        draw(Some(a), [20., 0., 10., 10.]),
    ]);
    let runs: Vec<_> = layers.runs.iter().map(|r| r.draws.clone()).collect();
    assert_eq!(runs, [0..1, 2..3]);
}

#[test]
fn a_text_draw_splits_a_run_too() {
    let a = material(1);
    let mut text = draw(None, [0., 0., 10., 10.]);
    text.kind = UiDrawKind::Text;
    let layers = layers(vec![
        draw(Some(a), [0., 0., 10., 10.]),
        text,
        draw(Some(a), [20., 0., 10., 10.]),
    ]);
    assert_eq!(layers.runs.len(), 2);
}

#[test]
fn a_different_clip_splits_a_run() {
    let a = material(1);
    let mut clipped = draw(Some(a), [20., 0., 10., 10.]);
    clipped.clip_rect = [0, 0, 100, 50];
    let layers = layers(vec![draw(Some(a), [0., 0., 10., 10.]), clipped]);
    let clips: Vec<_> = layers.runs.iter().map(|r| r.clip).collect();
    assert_eq!(clips, [[0, 0, 200, 100], [0, 0, 100, 50]]);
}

#[test]
fn a_run_is_clipped_to_the_viewport_and_one_past_it_is_left_out() {
    let a = material(1);
    let layers = layers(vec![
        draw(Some(a), [180., 90., 40., 40.]),
        draw(None, [0., 0., 5., 5.]),
        draw(Some(a), [200., 0., 30., 30.]),
        draw(None, [0., 0., 5., 5.]),
        draw(Some(a), [-30., -30., 10., 10.]),
    ]);
    assert_eq!(layers.runs.len(), 1);
    assert_eq!(layers.runs[0].bounds, [180, 90, 20, 10]);
    assert!(layers.too_large.is_empty());
}

#[test]
fn a_run_is_clipped_to_the_clip_of_its_draws() {
    let mut clipped = draw(Some(material(1)), [0., 0., 100., 100.]);
    clipped.clip_rect = [30, 40, 20, 10];
    let layers = layers(vec![clipped]);
    assert_eq!(layers.runs[0].bounds, [30, 40, 20, 10]);
}

#[test]
fn runs_pack_without_overlap_and_the_same_input_gives_the_same_plan() {
    let first_material = material(1);
    let draws: Vec<_> = (0..12_u8)
        .flat_map(|i| {
            let size = 10. + f32::from(i % 4) * 10.;
            [
                draw(Some(first_material), [f32::from(i) * 3., 0., size, size]),
                draw(None, [0., 0., 1., 1.]),
            ]
        })
        .collect();
    let first = layers(draws.clone());
    assert_eq!(first, layers(draws));
    assert_eq!(first.runs.len(), 12);
    assert!(first.too_large.is_empty());
    for (number, run) in first.runs.iter().enumerate() {
        assert_eq!(run.atlas[2..], run.bounds[2..]);
        assert!(inside(run.atlas, first.atlas_size), "run {number}");
        for other in &first.runs[..number] {
            assert!(
                apart(run.atlas, other.atlas),
                "run {number} overlaps {:?}",
                other.atlas
            );
        }
    }
}

/// Whether the rectangle (x, y, width, height) lies inside an atlas of `size`.
fn inside([left, bottom, width, height]: [u32; 4], size: [u32; 2]) -> bool {
    left + width <= size[0] && bottom + height <= size[1]
}

/// Whether two rectangles share no pixel.
fn apart([ax, ay, aw, ah]: [u32; 4], [bx, by, bw, bh]: [u32; 4]) -> bool {
    ax + aw <= bx || bx + bw <= ax || ay + ah <= by || by + bh <= ay
}

/// `count` runs that each cover the whole viewport, told apart by a default draw between them.
fn full_screen_runs(count: usize) -> UiPresentation {
    let a = material(1);
    let draws = (0..count)
        .flat_map(|_| {
            [
                draw(Some(a), [0., 0., 200., 100.]),
                draw(None, [0., 0., 1., 1.]),
            ]
        })
        .collect();
    plan(draws)
}

/// Asserts that every run lies inside the atlas and no two runs share a pixel.
fn assert_packed(layers: &UiLayers) {
    for (number, run) in layers.runs.iter().enumerate() {
        assert_eq!(run.atlas[2..], run.bounds[2..]);
        assert!(inside(run.atlas, layers.atlas_size), "run {number}");
        for other in &layers.runs[..number] {
            assert!(apart(run.atlas, other.atlas), "run {number} overlaps");
        }
    }
}

#[test]
fn a_run_that_covers_the_whole_viewport_fits_the_atlas() {
    let layers = full_screen_runs(1).layers(VIEWPORT, LIMIT);
    assert_eq!(layers.runs.len(), 1);
    assert_eq!(layers.runs[0].atlas, [0, 0, 200, 100]);
    assert_eq!(layers.atlas_size, VIEWPORT);
    assert!(layers.too_large.is_empty());
}

#[test]
fn more_runs_than_one_page_holds_grow_the_atlas_and_every_run_gets_a_place() {
    let layers = full_screen_runs(5).layers(VIEWPORT, LIMIT);
    assert_eq!(layers.runs.len(), 5);
    assert!(layers.too_large.is_empty(), "{:?}", layers.too_large);
    assert!(layers.atlas_size[0] * layers.atlas_size[1] >= 5 * 200 * 100);
    assert!(layers.atlas_size.iter().all(|side| *side <= LIMIT));
    assert_packed(&layers);
}

#[test]
fn the_atlas_stays_the_size_of_the_viewport_while_the_runs_fit_it() {
    let layers = full_screen_runs(1).layers(VIEWPORT, LIMIT);
    assert_eq!(layers.atlas_size, VIEWPORT);
    let two = full_screen_runs(2).layers(VIEWPORT, LIMIT);
    assert_eq!(two.runs.len(), 2);
    assert_ne!(two.atlas_size, VIEWPORT, "two screens do not fit one page");
}

#[test]
fn the_atlas_grows_to_the_limit_before_it_refuses_a_run() {
    let layers = full_screen_runs(40).layers(VIEWPORT, 1000);
    assert_eq!(layers.runs.len(), 40);
    assert!(layers.too_large.is_empty());
    assert_packed(&layers);
}

#[test]
fn a_run_that_does_not_fit_the_atlas_at_its_limit_is_refused_by_its_size() {
    let layers = full_screen_runs(3).layers(VIEWPORT, 200);
    assert_eq!(layers.atlas_size, [200, 200]);
    assert_eq!(layers.runs.len(), 2);
    assert_eq!(layers.too_large.len(), 1);
    assert_eq!(layers.too_large[0].draws, 4..5);
    assert_eq!(layers.too_large[0].size, [200, 100]);
    assert_eq!(layers.max_side, 200);
    assert_packed(&layers);
}

#[test]
fn a_run_wider_than_the_limit_is_refused_by_its_size_and_the_others_are_placed() {
    let a = material(1);
    let layers = plan(vec![
        draw(Some(a), [0., 0., 200., 80.]),
        draw(None, [0., 0., 1., 1.]),
        draw(Some(a), [0., 0., 100., 50.]),
    ])
    .layers(VIEWPORT, 150);
    assert_eq!(layers.too_large.len(), 1);
    assert_eq!(layers.too_large[0].draws, 0..1);
    assert_eq!(layers.too_large[0].size, [200, 80]);
    assert_eq!(layers.runs.len(), 1);
    assert_eq!(layers.runs[0].draws, 2..3);
    assert_packed(&layers);
}

#[test]
fn a_plan_that_needed_growth_packs_the_same_way_every_time() {
    let plan = full_screen_runs(7);
    assert_eq!(plan.layers(VIEWPORT, LIMIT), plan.layers(VIEWPORT, LIMIT));
}
