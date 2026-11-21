//! A camera that the frame gave a set of renderers draws those renderers, within its mask.

use std::collections::BTreeSet;
use std::sync::Arc;

use worldrunner_render_program::ids::RendererId;

use super::camera::{CAMERA, LEFT, camera, halves, render};
use super::rig::{BLUE, CLEAR, RED, Rig, pixel};

const RIGHT: u32 = 2;

fn restricted(rig: &mut Rig, mask: u32, set: &[u64]) -> Vec<u8> {
    let mut frame = halves(rig, mask);
    let mut view = camera(mask);
    view.renderers = Arc::new(
        set.iter()
            .map(|id| RendererId(*id))
            .collect::<BTreeSet<_>>(),
    );
    frame.cameras.insert(CAMERA, view);
    render(rig, &frame)
}

#[test]
fn a_camera_with_a_set_draws_only_the_renderers_in_it() {
    let mut rig = Rig::new();
    let image = restricted(&mut rig, (1 << LEFT) | (1 << RIGHT), &[2]);
    assert_eq!(
        pixel(&image, 8, (1, 3)),
        CLEAR,
        "renderer 1 is not in the set"
    );
    assert_eq!(pixel(&image, 8, (6, 3)), BLUE);
}

#[test]
fn a_set_does_not_widen_the_mask() {
    let mut rig = Rig::new();
    let image = restricted(&mut rig, 1 << LEFT, &[1, 2]);
    assert_eq!(pixel(&image, 8, (1, 3)), RED);
    assert_eq!(
        pixel(&image, 8, (6, 3)),
        CLEAR,
        "layer 2 is outside the mask"
    );
}

#[test]
fn an_empty_set_draws_only_the_clear_color() {
    let mut rig = Rig::new();
    let image = restricted(&mut rig, (1 << LEFT) | (1 << RIGHT), &[]);
    assert!(image.chunks(4).all(|texel| texel == CLEAR));
}

#[test]
fn the_draw_bound_of_a_camera_with_a_set_counts_only_its_renderers() {
    let mut rig = Rig::new();
    let mut frame = halves(&mut rig, (1 << LEFT) | (1 << RIGHT));
    let record = frame.records[0].clone();
    assert_eq!(frame.planned_draw_bound(&record), 2);
    frame.cameras.get_mut(&CAMERA).unwrap().renderers = Arc::new(BTreeSet::from([RendererId(1)]));
    assert_eq!(frame.planned_draw_bound(&record), 1);
}
