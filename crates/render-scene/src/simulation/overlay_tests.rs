use super::UiOverlay;
use std::collections::BTreeMap;
use worldrunner_world_simulation::graphics::MaterialId;
use worldrunner_world_simulation::ui::UiTexture;
use worldrunner_world_simulation::ui::presentation::{
    UiDraw, UiDrawKind, UiMaterial, UiPresentation,
};

const VIEWPORT: [u32; 2] = [64, 32];

fn draw(material: Option<UiMaterial>, rect: [f32; 4], order: (i32, usize)) -> UiDraw {
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
        texture: material.map(|_| UiTexture::Image(9)),
        material,
        sorting_layer: 3,
        sorting_order: order.0,
        hierarchy_order: order.1,
        clip_rect: [0, 0, VIEWPORT[0], VIEWPORT[1]],
    }
}

fn material(bits: u64) -> UiMaterial {
    UiMaterial {
        id: MaterialId::from_bits(bits),
        generation: 1,
    }
}

#[test]
fn a_run_becomes_one_quad_at_the_sort_key_of_its_first_draw() {
    let (a, b) = (material(1), material(2));
    let plan = UiPresentation {
        draws: vec![
            draw(None, [0., 0., 4., 4.], (1, 0)),
            draw(Some(a), [10., 4., 8., 6.], (2, 5)),
            draw(Some(b), [20., 8., 10., 6.], (2, 6)),
            draw(None, [0., 0., 4., 4.], (3, 0)),
        ],
        ..UiPresentation::default()
    };
    let layers = plan.layers(VIEWPORT, VIEWPORT);
    assert_eq!(layers.runs.len(), 1);
    let overlay = UiOverlay::from_presentation(&plan, &BTreeMap::new(), &layers).unwrap();
    assert_eq!(overlay.quads.len(), 3);
    let keys: Vec<_> = overlay
        .quads
        .iter()
        .map(|q| (q.sorting_layer, q.sorting_order, q.hierarchy_order))
        .collect();
    assert_eq!(keys, [(3, 1, 0), (3, 2, 5), (3, 3, 0)]);
    let run = &overlay.quads[1];
    assert_eq!(run.texture, overlay.layer);
    assert!(overlay.shows_layers());
    let corners = run.corners.unwrap();
    near(&corners[0], &[10., 4., 0.]);
    near(&corners[2], &[30., 14., 0.]);
    // The run sits at the atlas origin; the atlas rows are bottom first, so the top edge of
    // the run reads the larger v.
    near(&run.uv_rect, &[0., 10. / 32., 20. / 64., 0.]);
    near(&run.tint, &[1.; 4]);
    assert_eq!(run.clip_rect, None);
}

fn near(actual: &[f32], expected: &[f32]) {
    assert_eq!(actual.len(), expected.len());
    for (a, e) in actual.iter().zip(expected) {
        assert!((a - e).abs() < 1e-6, "{actual:?} is not {expected:?}");
    }
}

#[test]
fn a_plan_without_runs_has_no_layer_texture() {
    let plan = UiPresentation {
        draws: vec![draw(None, [0., 0., 4., 4.], (0, 0))],
        ..UiPresentation::default()
    };
    let layers = plan.layers(VIEWPORT, VIEWPORT);
    let overlay = UiOverlay::from_presentation(&plan, &BTreeMap::new(), &layers).unwrap();
    assert_eq!(overlay.quads.len(), 1);
    assert!(!overlay.shows_layers());
    assert!(overlay.textures.is_empty());
}

#[test]
fn a_material_draw_outside_every_run_has_no_quad() {
    let plan = UiPresentation {
        draws: vec![
            draw(None, [0., 0., 4., 4.], (0, 0)),
            draw(Some(material(1)), [100., 100., 4., 4.], (1, 0)),
        ],
        ..UiPresentation::default()
    };
    let layers = plan.layers(VIEWPORT, VIEWPORT);
    assert!(layers.runs.is_empty());
    let overlay = UiOverlay::from_presentation(&plan, &BTreeMap::new(), &layers).unwrap();
    assert_eq!(overlay.quads.len(), 1);
}
