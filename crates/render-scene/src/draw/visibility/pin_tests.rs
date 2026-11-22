//! Pins the exact selections of a synthetic world for the camera and four cascades, over a
//! walking camera, a camera that stands still and a camera that returns to an earlier pose.
//!
//! The equality tests compare the tree walk with a full scan. These tests pin the selected
//! ranges, their order and the statistics themselves, so a change that alters any of them
//! fails even if it altered the scan reference the same way.
use super::large_scene::{FrameViews, slice_hull, views as view_from, walk, world};
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

/// Selects every view of the frame together, as the renderer does, on up to `threads` threads.
fn select_frame(
    instances: &Instances,
    meshes: &[MeshSelection],
    views: &FrameViews,
    threads: usize,
) -> Vec<Selection> {
    let mut selections = frame_selections(instances, meshes, views);
    with_placements(
        instances,
        meshes,
        (views.eye_millimeters, views.fade_eye),
        &std::cell::RefCell::default(),
        |placements| {
            placements.select_each(
                &mut selections
                    .iter_mut()
                    .map(|(matrix, selection)| (*matrix, selection))
                    .collect::<Vec<_>>(),
                threads,
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
        let selections = select_frame(&instances, &meshes, &walk(frame), 4);
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

fn mesh_selections(source: &worldrunner_render_plan::RenderScene) -> Vec<MeshSelection> {
    source
        .geometry()
        .meshes()
        .iter()
        .map(MeshSelection::new)
        .collect()
}

/// The frame's selections on each count of threads, which must all equal those on one.
fn assert_threads_agree(instances: &Instances, meshes: &[MeshSelection], views: &FrameViews) {
    let alone = select_frame(instances, meshes, views, 1);
    for threads in [2, 3, 4, 8] {
        let together = select_frame(instances, meshes, views, threads);
        assert_eq!(
            digest_of(&together),
            digest_of(&alone),
            "{threads} threads select what one does"
        );
        for (together, alone) in together.iter().zip(&alone) {
            assert_eq!(together.stats, alone.stats, "{threads} threads");
            let ranges = |selection: &Selection| {
                selection
                    .ranges
                    .iter()
                    .map(|range| (range.key, range.lod, range.slots.clone()))
                    .collect::<Vec<_>>()
            };
            assert_eq!(ranges(together), ranges(alone), "{threads} threads");
        }
    }
}

#[test]
fn every_count_of_threads_selects_the_same_over_moving_eyes_and_edits() {
    let source = world(8_000, 12, 5);
    let mut instances = Instances::new(&source);
    let meshes = mesh_selections(&source);
    assert!(
        instances.edits() >= 6_000,
        "enough to give eight threads chunks"
    );
    let handles: Vec<_> = instances.handles().collect();
    for frame in [0, 1, 2, 9, 40] {
        assert_threads_agree(&instances, &meshes, &walk(frame));
    }
    for (index, &handle) in handles.iter().enumerate() {
        let (object, _) = instances.get(handle).unwrap();
        match index % 50 {
            0..=3 => {
                let mut moved = *object.transform();
                moved.position_millimeters[1] -= 25_000;
                instances.set_transform(handle, moved).unwrap();
            }
            10 => {
                instances.remove(handle).unwrap();
                instances.add(object, true).unwrap();
            }
            20 | 21 => instances.remove(handle).unwrap(),
            _ => {}
        }
    }
    for frame in [0, 3, 3, 41] {
        assert_threads_agree(&instances, &meshes, &walk(frame));
    }
}

#[test]
fn views_that_admit_nothing_or_scenes_without_placements_select_nothing_on_any_threads() {
    let source = world(6_000, 8, 11);
    let mut instances = Instances::new(&source);
    let meshes = mesh_selections(&source);
    let frame = walk(4);
    // Views far from every placement beside one that sees many, and no view at all.
    let away = view_from(Vec3::new(20_000.0, 20_000.0, 2.0), 0.4, -0.05).camera;
    for threads in [1, 3, 8] {
        let mut selections = [
            (frame.camera, Selection::default()),
            (away, Selection::default()),
            (
                away,
                Selection {
                    casters_only: true,
                    ..Selection::default()
                },
            ),
        ];
        with_placements(
            &instances,
            &meshes,
            (frame.eye_millimeters, frame.fade_eye),
            &std::cell::RefCell::default(),
            |placements| {
                placements.select_each(&mut [], threads);
                placements.select_each(
                    &mut selections
                        .iter_mut()
                        .map(|(matrix, selection)| (*matrix, selection))
                        .collect::<Vec<_>>(),
                    threads,
                );
            },
        );
        assert!(!selections[0].1.ranges.is_empty());
        for (_, selection) in &selections[1..] {
            assert!(selection.ranges.is_empty(), "{threads} threads");
            assert_eq!(selection.stats.considered, selections[0].1.stats.considered);
            assert_eq!(selection.stats.culled, selection.stats.considered);
            assert!(
                selection
                    .stats
                    .drawn_per_lod
                    .iter()
                    .all(|&drawn| drawn == 0)
            );
        }
    }
    let handles: Vec<_> = instances.handles().collect();
    for handle in handles {
        instances.remove(handle).unwrap();
    }
    for threads in [1, 8] {
        let emptied = select_frame(&instances, &meshes, &frame, threads);
        assert_eq!(emptied.len(), 6);
        for selection in &emptied {
            assert!(selection.ranges.is_empty());
            assert_eq!(selection.stats.considered, 0);
            assert_eq!(selection.stats.culled, 0);
        }
    }
}
