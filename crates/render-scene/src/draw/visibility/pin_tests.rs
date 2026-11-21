//! Pins the exact selections of a synthetic world for the camera and four cascades, over a
//! walking camera, a camera that stands still and a camera that returns to an earlier pose.
//!
//! The equality tests compare the tree walk with a full scan. These tests pin the selected
//! ranges, their order and the statistics themselves, so a change that alters any of them
//! fails even if it altered the scan reference the same way.
use super::large_scene::{FrameViews, slice_hull, walk, world};
use super::*;

/// FNV-1a over 64-bit words, so a digest stays the same on every platform and toolchain.
struct Digest(u64);

impl Digest {
    fn new() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }

    fn word(&mut self, word: u64) {
        for byte in word.to_le_bytes() {
            self.0 = (self.0 ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3);
        }
    }

    fn count(&mut self, count: usize) {
        self.word(u64::try_from(count).expect("small count"));
    }

    fn selection(&mut self, selection: &Selection) {
        self.count(selection.ranges.len());
        for range in &selection.ranges {
            let (pipeline, mesh, material) = range.key;
            for part in [pipeline, mesh, material, range.lod] {
                self.count(part);
            }
            self.word(u64::from(range.slots.start));
            self.word(u64::from(range.slots.end));
        }
        self.count(selection.stats.considered);
        self.count(selection.stats.culled);
        for &drawn in &selection.stats.drawn_per_lod {
            self.count(drawn);
        }
    }
}

/// The camera, the camera filtered to casters, and the four caster-fitted cascades with their
/// slices, each with its matrix.
fn frame_selections(
    instances: &Instances,
    meshes: &[MeshSelection],
    views: &FrameViews,
) -> Vec<(Mat4, Selection)> {
    let mut selections = vec![
        (views.camera, Selection::default()),
        (
            views.camera,
            Selection {
                casters_only: true,
                ..Selection::default()
            },
        ),
    ];
    for (index, &receiver) in views.receivers.iter().enumerate() {
        let matrix = caster_matrix(instances, meshes, None, receiver);
        selections.push((
            matrix,
            Selection {
                casters_only: true,
                slice: Some(slice_hull(views, index, matrix)),
                ..Selection::default()
            },
        ));
    }
    selections
}

/// Selects every view of the frame together, as the renderer does.
fn select_frame(
    instances: &Instances,
    meshes: &[MeshSelection],
    views: &FrameViews,
) -> Vec<Selection> {
    let mut selections = frame_selections(instances, meshes, views);
    with_placements(
        instances,
        meshes,
        (views.eye_millimeters, views.fade_eye),
        |placements| {
            placements.select_each(
                &mut selections
                    .iter_mut()
                    .map(|(matrix, selection)| (*matrix, selection))
                    .collect::<Vec<_>>(),
            );
        },
    );
    selections
        .into_iter()
        .map(|(_, selection)| selection)
        .collect()
}

fn digest_of(selections: &[Selection]) -> u64 {
    let mut digest = Digest::new();
    for selection in selections {
        digest.selection(selection);
    }
    digest.0
}

/// Walk positions, with repeats for the frames in which nothing moved and a return to an
/// earlier pose.
const FRAMES: [u16; 8] = [0, 0, 1, 2, 7, 7, 0, 40];

#[test]
fn walking_still_and_returning_cameras_select_the_pinned_ranges() {
    let source = world(3_000, 12, 3);
    let instances = Instances::new(&source);
    let meshes: Vec<_> = source
        .geometry()
        .meshes()
        .iter()
        .map(MeshSelection::new)
        .collect();
    let mut digests = Vec::new();
    let mut counts = Vec::new();
    for frame in FRAMES {
        let selections = select_frame(&instances, &meshes, &walk(frame));
        digests.push(digest_of(&selections));
        counts.push(
            selections
                .iter()
                .map(|selection| selection.ranges.len())
                .collect::<Vec<_>>(),
        );
    }
    assert_eq!(digests, PINNED_DIGESTS);
    assert_eq!(counts, PINNED_RANGE_COUNTS);
    // Equal poses select equally, whether or not the pose was visited between.
    assert_eq!(digests[0], digests[1]);
    assert_eq!(digests[0], digests[6]);
    assert_eq!(digests[4], digests[5]);
    assert_ne!(digests[0], digests[2]);
}

const PINNED_DIGESTS: [u64; 8] = [
    0xde60_6717_7ac7_9ca4,
    0xde60_6717_7ac7_9ca4,
    0xedaf_36c5_dc6d_40e2,
    0x417e_cc7d_fb2e_9cf6,
    0x44ce_498e_ac0d_88f7,
    0x44ce_498e_ac0d_88f7,
    0xde60_6717_7ac7_9ca4,
    0xc07a_ed70_021c_5e13,
];

/// Ranges selected by the camera, the camera's casters and the four cascades.
const PINNED_RANGE_COUNTS: [[usize; 6]; 8] = [
    [670, 623, 2, 5, 52, 361],
    [670, 623, 2, 5, 52, 361],
    [666, 620, 1, 5, 52, 370],
    [665, 619, 1, 5, 54, 365],
    [686, 635, 2, 9, 57, 371],
    [686, 635, 2, 9, 57, 371],
    [670, 623, 2, 5, 52, 361],
    [640, 580, 4, 9, 68, 297],
];

#[test]
fn one_pass_stamp_equals_the_separate_reads_as_placements_change() {
    let check = |instances: &Instances| {
        let stamp = instances.stamp();
        assert_eq!(
            (stamp.edits, stamp.content, stamp.skinned),
            (
                instances.edits(),
                instances.content_generation(),
                instances.has_skinned_batches()
            )
        );
        stamp
    };
    let mut instances = Instances::new(&world(500, 6, 9));
    let first = check(&instances);
    assert!(first.skinned);
    let handle = instances.handles().next().unwrap();
    let (object, _) = instances.get(handle).unwrap();
    let mut moved = *object.transform();
    moved.position_millimeters[0] += 5_000;
    instances.set_transform(handle, moved).unwrap();
    let edited = check(&instances);
    assert!(edited.edits > first.edits);
    instances.add(object, true).unwrap();
    assert!(check(&instances).edits > edited.edits);
    instances.remove(handle).unwrap();
    check(&instances);
    assert!(!check(&Instances::new(&super::tests::scene())).skinned);
}
