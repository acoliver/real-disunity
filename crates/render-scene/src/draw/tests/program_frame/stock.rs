//! What the compositor does with and without admitted programs, observed on the frame.

use super::super::*;
use super::rig::{BLACK, Fixture, GREEN, Image, RED, engine_scene, frame};
use super::{COLOR_FORMAT, settings};
use crate::draw::DrawCount;

fn fixture() -> Fixture {
    Fixture::new(settings(), |device| {
        ScenePipelines::new(device, COLOR_FORMAT)
    })
}

/// Records, submits and reads one frame, with the fixture's patches admitted when `programs`.
fn submitted(fixture: &mut Fixture, programs: bool) -> Image {
    let authority = fixture.context.submission().clone();
    let mut recording = authority.begin(fixture.context.device()).unwrap();
    if programs {
        let signature = fixture.signature();
        let admitted = fixture.admit(&recording, signature).unwrap();
        fixture.gpu.admit_programs(admitted);
    }
    fixture.encode(&mut recording).unwrap();
    let readback = fixture.output_copy(&mut recording);
    recording.finish().submit().unwrap();
    fixture.read(&readback)
}

#[test]
fn without_admitted_programs_the_frame_is_byte_identical_to_the_stock_scene_draw() {
    let mut fixture = fixture();
    let composed = submitted(&mut fixture, false);
    let direct = render_frame(&fixture.context, &engine_scene(), &frame());
    assert_eq!(composed.bytes(), direct.as_slice());
    assert_eq!(composed.pixel(30, 20), RED, "the stock scene did draw");
}

#[test]
fn a_cancelled_program_frame_leaves_the_target_and_the_published_counters_unchanged() {
    let mut fixture = fixture();
    let authority = fixture.context.submission().clone();
    let baseline = submitted(&mut fixture, true);
    let published = fixture.gpu.statistics().meshes;
    assert_eq!(
        published,
        DrawCount {
            calls: 1,
            instances: 1
        }
    );

    // With the card hidden, a frame that is submitted draws the patches alone.
    let card = fixture.gpu.instance_handles().next().unwrap();
    fixture.gpu.set_instance_visible(card, false).unwrap();
    let serial = authority.submitted_serial();
    let mut recording = authority.begin(fixture.context.device()).unwrap();
    let signature = fixture.signature();
    let admitted = fixture.admit(&recording, signature).unwrap();
    fixture.gpu.admit_programs(admitted);
    fixture.encode(&mut recording).unwrap();
    fixture.output_copy(&mut recording);
    recording.finish().cancel();

    assert_eq!(
        authority.submitted_serial(),
        serial,
        "cancelling submits nothing"
    );
    assert_eq!(fixture.gpu.statistics().meshes, published);
    let mut inspect = authority.begin(fixture.context.device()).unwrap();
    let readback = fixture.copy_output(&mut inspect);
    inspect.finish().submit().unwrap();
    assert_eq!(fixture.read(&readback).bytes(), baseline.bytes());

    // The control: the same frame submitted does change both.
    let drawn = submitted(&mut fixture, true);
    assert_eq!(drawn.pixel(30, 20), BLACK, "the card is hidden");
    assert_eq!(drawn.pixel(54, 32), GREEN, "the patches still draw");
    assert_eq!(fixture.gpu.statistics().meshes, DrawCount::default());
}
