//! A player shader program and the resident engine scene drawn into one frame through the
//! configured compositor.
//!
//! The camera looks at a red engine card 3 m away that covers the middle of a 64 by 64 frame.
//! A green patch of the player program sits in front of the card's right edge and a blue patch
//! sits behind its lower left corner. Every expected pixel is derived from those placements and
//! clip depths, not from what the shaders output.

use super::*;
use crate::compositor::RendererConfiguration;

mod admission;
mod offscreen;
mod rig;
mod stock;
use rig::{BLACK, BLUE, CARD_EDGE, FAR_DEPTH, Fixture, GREEN, NEAR_DEPTH, RED, engine_depth};

const DEPTH_FORMAT: TextureFormat = TextureFormat::Depth32Float;

fn settings() -> RendererConfiguration {
    RendererConfiguration::legacy([TARGET; 2], COLOR_FORMAT)
}

#[test]
fn player_program_and_scene_gpu_draw_composite_in_one_submitted_frame() {
    // The engine's card sits between the patches in depth, which is what decides the overlaps.
    assert!(NEAR_DEPTH < engine_depth() && engine_depth() < FAR_DEPTH);
    // The card covers pixels 32 +- 18.5 on both axes; the patches' clip rectangles cover
    // columns 38.4..57.6 and rows 25.6..38.4 (near) and columns 6.4..25.6 and rows 44.8..57.6
    // (far).
    assert!((f64::from(TARGET) / 2.0 * CARD_EDGE - 18.47).abs() < 0.01);

    let mut fixture = Fixture::new(settings(), |device| {
        ScenePipelines::new(device, COLOR_FORMAT)
    });
    let authority = fixture.context.submission().clone();
    let before = authority.submitted_serial();

    let mut frame = authority.begin(fixture.context.device()).unwrap();
    let signature = fixture.signature();
    let admitted = fixture.admit(&frame, signature).unwrap();
    fixture.gpu.admit_programs(admitted);
    fixture.encode(&mut frame).unwrap();
    let readback = fixture.output_copy(&mut frame);
    assert_eq!(
        authority.submitted_serial(),
        before,
        "recording must not submit"
    );

    frame.finish().submit().unwrap();
    assert_eq!(
        authority.submitted_serial(),
        before + 1,
        "one frame, one submission"
    );
    let image = fixture.read(&readback);
    assert_eq!(authority.submitted_serial(), before + 1);

    // Engine only, away from both patches and the card's edges.
    assert_eq!(image.pixel(30, 20), RED);
    assert_eq!(image.pixel(20, 30), RED);
    assert_eq!(image.pixel(44, 20), RED);
    // Player only: green beyond the card's right edge, blue beyond its left edge.
    assert_eq!(image.pixel(54, 32), GREEN);
    assert_eq!(image.pixel(10, 54), BLUE);
    // The near patch is in front of the card and wins; the far patch is behind it and loses.
    assert_eq!(image.pixel(44, 32), GREEN);
    assert_eq!(image.pixel(20, 48), RED);
    // Neither drew where nothing was.
    assert_eq!(image.pixel(3, 3), BLACK);
    assert_eq!(image.pixel(60, 60), BLACK);
}
