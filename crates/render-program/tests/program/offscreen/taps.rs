//! Reads of a record's target after the record, in the one submission that draws the frame.

use std::sync::Arc;

use worldrunner_render_program::unity::offscreen::PendingImage;
use worldrunner_render_program::unity::offscreen::frame::{GlobalState, OffscreenFrame, Record};

use super::camera::{TARGET, fixture};
use super::depth_only::depth_frame;
use super::lists::{item, list, whole};
use super::rig::{BLUE, CLEAR, RED, Rig, layer, pixel};

const LEFT_ONLY: u32 = 1 << 1;

/// The images of one tap, or the reasons they were refused.
struct Tapped {
    record: usize,
    colour: Result<Vec<u8>, String>,
    depth: Result<Vec<u8>, String>,
}

/// The target's image at the end of the frame, and each tap of `taps`.
struct Run {
    last: Vec<u8>,
    taps: Vec<Tapped>,
}

fn run_tapped(rig: &mut Rig, frame: &OffscreenFrame, taps: &[usize]) -> Run {
    let frame = OffscreenFrame {
        taps: taps.to_vec(),
        ..frame.clone()
    };
    let mut operations = rig.build(&frame).unwrap();
    let pending = operations.take_taps();
    let last = operations.read_back(TARGET, 0, 0).unwrap();
    let authority = rig.gpu.authority.clone();
    let mut encoder = authority.begin(&rig.gpu.device).unwrap();
    let admitted = operations.admit(&encoder).unwrap();
    admitted.encode(None, &mut encoder).unwrap();
    encoder.finish().submit().unwrap();
    let device = rig.gpu.device.clone();
    let bytes = |image: Result<PendingImage, String>| image.map(|image| image.bytes(&device));
    Run {
        taps: pending
            .into_iter()
            .map(|tap| Tapped {
                record: tap.record,
                colour: bytes(tap.colour),
                depth: bytes(tap.depth),
            })
            .collect(),
        last: last.bytes(&device),
    }
}

/// A clear to yellow, then the red renderer over the left half and the blue one over the right.
fn three_records(rig: &mut Rig) -> OffscreenFrame {
    let mut frame = fixture(rig, 0);
    frame.records = vec![
        Record::Clear {
            target: whole(TARGET),
            clear: worldrunner_render_program::unity::offscreen::frame::Clear {
                color: Some([1.0, 1.0, 0.0, 1.0]),
                depth: None,
                stencil: None,
            },
        },
        list(whole(TARGET), vec![item(1, None)]),
        list(whole(TARGET), vec![item(2, None)]),
    ];
    frame
}

fn depths(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks(4)
        .map(|float| f32::from_le_bytes(float.try_into().unwrap()))
        .collect()
}

#[test]
fn a_tap_reads_the_target_as_it_stands_after_its_record() {
    let mut rig = Rig::new();
    let frame = three_records(&mut rig);
    let run = run_tapped(&mut rig, &frame, &[1, 2]);
    assert_eq!(run.taps.len(), 2);
    let after_red = run.taps[0].colour.as_ref().unwrap();
    assert_eq!(run.taps[0].record, 1);
    assert_eq!(pixel(after_red, 8, (1, 3)), RED);
    assert_eq!(pixel(after_red, 8, (6, 3)), CLEAR, "blue is not drawn yet");
    let after_blue = run.taps[1].colour.as_ref().unwrap();
    assert_eq!(run.taps[1].record, 2);
    assert_eq!(pixel(after_blue, 8, (1, 3)), RED);
    assert_eq!(pixel(after_blue, 8, (6, 3)), BLUE);
}

#[test]
fn taps_leave_the_final_image_as_it_is_without_them() {
    let mut rig = Rig::new();
    let frame = three_records(&mut rig);
    let plain = run_tapped(&mut rig, &frame, &[]);
    let tapped = run_tapped(&mut rig, &frame, &[0, 1, 2]);
    assert_eq!(tapped.taps.len(), 3);
    assert_eq!(plain.last, tapped.last);
    assert_eq!(pixel(&tapped.last, 8, (6, 3)), BLUE);
}

#[test]
fn a_tap_after_a_record_that_draws_nothing_is_a_named_refusal() {
    let mut rig = Rig::new();
    let mut frame = three_records(&mut rig);
    frame.records.insert(
        1,
        Record::Globals(Arc::new(GlobalState {
            values: layer(&[]),
            keywords: Vec::new(),
        })),
    );
    let run = run_tapped(&mut rig, &frame, &[1]);
    let refusal = run.taps[0].colour.as_ref().unwrap_err();
    assert!(
        refusal.contains("record 1") && refusal.contains("Globals"),
        "{refusal}"
    );
    assert!(run.taps[0].depth.is_err());
    assert_eq!(
        pixel(&run.last, 8, (6, 3)),
        BLUE,
        "every draw still happens"
    );
}

#[test]
fn a_tap_past_the_last_record_is_a_named_refusal() {
    let mut rig = Rig::new();
    let frame = three_records(&mut rig);
    let run = run_tapped(&mut rig, &frame, &[9]);
    assert_eq!(run.taps.len(), 1);
    assert_eq!(run.taps[0].record, 9);
    let refusal = run.taps[0].colour.as_ref().unwrap_err();
    assert!(
        refusal.contains("record 9") && refusal.contains("3 records"),
        "{refusal}"
    );
    assert_eq!(pixel(&run.last, 8, (6, 3)), BLUE);
}

#[test]
fn a_depth_tap_reads_the_stored_depth_of_a_depth_only_target() {
    let mut rig = Rig::new();
    let frame = depth_frame(&mut rig, LEFT_ONLY, 32);
    let run = run_tapped(&mut rig, &frame, &[0]);
    let depth = depths(run.taps[0].depth.as_ref().unwrap());
    assert_eq!(depth.len(), 64);
    assert_eq!(depth[3].to_bits(), 0.5_f32.to_bits());
    assert_eq!(depth[4].to_bits(), 0.0_f32.to_bits());
    let refusal = run.taps[0].colour.as_ref().unwrap_err();
    assert!(refusal.contains("no colour"), "{refusal}");
}

#[test]
fn a_depth_tap_reads_the_depth_under_a_colour_target() {
    let mut rig = Rig::new();
    let mut frame = fixture(&mut rig, LEFT_ONLY);
    frame.render_textures[0].depth_bits = 32;
    let run = run_tapped(&mut rig, &frame, &[0]);
    assert_eq!(pixel(run.taps[0].colour.as_ref().unwrap(), 8, (1, 3)), RED);
    let depth = depths(run.taps[0].depth.as_ref().unwrap());
    assert_eq!(depth[3].to_bits(), 0.5_f32.to_bits());
    assert_eq!(depth[4].to_bits(), 0.0_f32.to_bits());
}

#[test]
fn a_depth_that_cannot_be_copied_is_refused_by_name_and_the_colour_is_still_read() {
    let mut rig = Rig::new();
    let mut frame = fixture(&mut rig, LEFT_ONLY);
    frame.render_textures[0].depth_bits = 24;
    let run = run_tapped(&mut rig, &frame, &[0]);
    assert_eq!(pixel(run.taps[0].colour.as_ref().unwrap(), 8, (1, 3)), RED);
    let refusal = run.taps[0].depth.as_ref().unwrap_err();
    assert!(refusal.contains("Depth24Plus"), "{refusal}");
}
