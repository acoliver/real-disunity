//! A frame abandoned after it admitted and encoded offscreen work leaves the texture bytes an
//! earlier submission wrote.
//!
//! A first frame fills element 2 of the texture array with cyan, blue and yellow. A second frame
//! with the same render textures would overwrite that element with magenta: its camera clears to
//! green, sees no renderer, and the blit inverts the green. It is admitted and encoded, then
//! abandoned, so the GPU never runs it. A third frame declares the same textures, has no records
//! and only reads element 2 back. Every expected pixel is derived from the colors and the
//! inversion, and the third frame never writes the array, so it can only report what is there.

use super::super::rig::Fixture;
use super::{ARRAY, BLUE, CYAN, Player, YELLOW, frame, pixel, player, timed};
use worldrunner_render_gpu::submission::RecordingFrame;
use worldrunner_render_program::ids::CameraId;
use worldrunner_render_program::unity::offscreen::frame::OffscreenFrame;

const MAGENTA: [u8; 4] = [255, 0, 255, 255];
const SENTINELS: [((usize, usize), [u8; 4]); 3] =
    [((1, 0), CYAN), ((4, 4), BLUE), ((7, 7), YELLOW)];

/// The ways a recorded frame is given up without being submitted.
#[derive(Clone, Copy)]
enum Abandon {
    Cancel,
    DropPrepared,
    DropRecording,
}

impl Abandon {
    fn apply(self, recording: RecordingFrame) {
        match self {
            Self::Cancel => recording.finish().cancel(),
            Self::DropPrepared => drop(recording.finish()),
            Self::DropRecording => drop(recording),
        }
    }
}

/// The frame whose submission would put magenta in element 2: the same render textures and
/// records, with a camera that clears to green and culls every renderer.
fn poison_frame() -> OffscreenFrame {
    let mut poison = frame();
    let camera = poison
        .cameras
        .get_mut(&CameraId(1))
        .expect("the frame's camera");
    camera.clear_color = Some([0.0, 1.0, 0.0, 1.0]);
    camera.culling_mask = 0;
    poison
}

/// The same render texture declarations with no records and no later reads: nothing in this frame
/// writes a texture.
fn readbacks_only() -> OffscreenFrame {
    let mut frame = frame();
    frame.records.clear();
    frame.later_reads.clear();
    frame
}

/// Builds `frame`, appends a readback of element 2 of the array, submits it as one frame and
/// returns the bytes the readback saw.
fn submit_and_read_element(
    fixture: &mut Fixture,
    player: &mut Player,
    frame: &OffscreenFrame,
) -> Vec<u8> {
    let authority = fixture.context.submission().clone();
    let device = fixture.context.device().clone();
    let mut operations = player
        .state
        .build(&mut player.runtime, &player.arena, frame)
        .unwrap();
    let element = operations.read_back(ARRAY, 0, 2).unwrap();
    let mut recording = authority.begin(&device).unwrap();
    fixture
        .gpu
        .admit_offscreen(operations.admit(&recording).unwrap());
    fixture.encode(&mut recording).unwrap();
    recording.finish().submit().unwrap();
    element.bytes(&device)
}

/// Admits and encodes the poison frame and hands back the unfinished recording.
fn encode_poison(fixture: &mut Fixture, player: &mut Player) -> RecordingFrame {
    let authority = fixture.context.submission().clone();
    let operations = player
        .state
        .build(&mut player.runtime, &player.arena, &poison_frame())
        .unwrap();
    let mut recording = authority.begin(fixture.context.device()).unwrap();
    fixture
        .gpu
        .admit_offscreen(operations.admit(&recording).unwrap());
    fixture.encode(&mut recording).unwrap();
    recording
}

fn check_sentinels(bytes: &[u8]) {
    for (place, color) in SENTINELS {
        assert_eq!(pixel(bytes, place), color, "element 2 at {place:?}");
    }
}

fn abandoning_leaves_element_bytes_as_submitted(how: Abandon) {
    let mut fixture = timed();
    let mut player = player(&mut fixture);
    let authority = fixture.context.submission().clone();
    let start = authority.submitted_serial();

    let baseline = submit_and_read_element(&mut fixture, &mut player, &frame());
    check_sentinels(&baseline);
    let after_baseline = authority.submitted_serial();

    let recording = encode_poison(&mut fixture, &mut player);
    let admission_taken = !fixture.gpu.has_offscreen();
    let while_recording = authority.submitted_serial();
    how.apply(recording);
    let after_abandon = authority.submitted_serial();

    let read = submit_and_read_element(&mut fixture, &mut player, &readbacks_only());
    // The bytes come first: a poison that reached the queue must fail here, not only in the
    // serial accounting below.
    assert_eq!(
        read, baseline,
        "the abandoned frame must not change element 2"
    );
    assert!(admission_taken, "encoding took the poison's admission");
    assert!(!fixture.gpu.has_offscreen(), "the readbacks were consumed");
    assert_eq!(after_baseline, start + 1, "the baseline is one submission");
    assert_eq!(while_recording, after_baseline, "recording must not submit");
    assert_eq!(after_abandon, after_baseline, "abandoning must not submit");
    assert_eq!(
        authority.submitted_serial(),
        after_baseline + 1,
        "only the readback frame submitted after the baseline"
    );
    player
        .arena
        .retire_settled(
            fixture.context.device(),
            std::time::Instant::now() + std::time::Duration::from_secs(30),
        )
        .expect("the abandoned frame released the arena");
}

#[test]
fn a_cancelled_frame_leaves_the_array_element_an_earlier_submission_wrote() {
    abandoning_leaves_element_bytes_as_submitted(Abandon::Cancel);
}

#[test]
fn a_dropped_prepared_frame_leaves_the_array_element_an_earlier_submission_wrote() {
    abandoning_leaves_element_bytes_as_submitted(Abandon::DropPrepared);
}

#[test]
fn a_dropped_recording_leaves_the_array_element_an_earlier_submission_wrote() {
    abandoning_leaves_element_bytes_as_submitted(Abandon::DropRecording);
}

#[test]
fn the_poison_frame_writes_magenta_into_the_array_element_when_it_is_submitted() {
    let mut fixture = timed();
    let mut player = player(&mut fixture);
    let baseline = submit_and_read_element(&mut fixture, &mut player, &frame());
    check_sentinels(&baseline);
    let poisoned = submit_and_read_element(&mut fixture, &mut player, &poison_frame());
    assert!(poisoned.chunks(4).all(|texel| texel == MAGENTA));
    let read = submit_and_read_element(&mut fixture, &mut player, &readbacks_only());
    assert_eq!(
        read, poisoned,
        "a submitted write persists for later frames"
    );
}
