//! Pins what the per-view selection and compaction path produces for a synthetic scene: the
//! selected ranges, the selection and compaction keys, the committed groups and which frames
//! rebuild or restage. The camera moves, stands still, changes one placement, returns to an
//! earlier pose, and a cascade changes its matrix.
use super::compaction::{Rig, cards, pose_at, unlit};
use super::*;
use glam::{Mat4, Vec3};

/// Numbers distinct texts in order of first appearance. A key's text holds matrix bits that
/// libm differences can change between platforms, so the pins record which keys are equal.
#[derive(Default)]
struct Classes(Vec<String>);

impl Classes {
    fn of(&mut self, text: String) -> usize {
        self.0
            .iter()
            .position(|seen| *seen == text)
            .unwrap_or_else(|| {
                self.0.push(text);
                self.0.len() - 1
            })
    }
}

fn fnv(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

/// A cascade whose map looks down at the cards, shifted sideways by `shift` metres.
fn cascade(index: u32, shift: f32) -> (u32, Mat4) {
    let projection = glam::camera::rh::proj::directx::orthographic(
        -4.0 + shift,
        4.0 + shift,
        -4.0,
        4.0,
        0.0,
        60.0,
    );
    let view = glam::camera::rh::view::look_to_mat4(
        Vec3::new(shift, -4.0, 25.0),
        Vec3::new(0.2, 0.3, -1.0).normalize(),
        Vec3::Z,
    );
    (index, projection * view)
}

fn rebuilds(gpu: &SceneGpu) -> usize {
    gpu.views.iter().map(|view| view.rebuilds.get()).sum()
}

/// One frame: draws the camera, then selects the camera and two cascades together as the
/// shadow feature does, and describes everything it produced.
fn frame(rig: &mut Rig, classes: &mut Classes, x: i64, cascades: [f32; 2]) -> String {
    let uniform = unlit(pose_at(x));
    let staged = rig.gpu.views[0].compaction.observed.staged.get();
    let before = rebuilds(&rig.gpu);
    rig.render(&uniform);
    let drawn = rebuilds(&rig.gpu) - before;
    let staged = rig.gpu.views[0].compaction.observed.staged.get() - staged;
    let views = [
        None,
        Some(cascade(0, cascades[0])),
        Some(cascade(2, cascades[1])),
    ];
    let before = rebuilds(&rig.gpu);
    rig.gpu.select_views(&views, &uniform);
    let shadows = rebuilds(&rig.gpu) - before;
    let mut selected = Vec::new();
    let mut keys = Vec::new();
    for (index, view) in views.into_iter().enumerate() {
        let selection = rig.gpu.select_objects(view, &uniform);
        selected.push(format!("{:?}{:?}", selection.ranges, selection.stats));
        let key = format!("{index}{:?}", rig.gpu.selection_key(view, &uniform));
        keys.push(classes.of(key));
    }
    let groups = format!(
        "{:?}",
        rig.gpu.views[0].compaction.committed.borrow().groups
    );
    let key = classes.of(format!("{:?}", rig.gpu.committed_compaction_key(0)));
    format!(
        "drawn {drawn} staged {staged} shadows {shadows} selected {:016x} keys {keys:?} groups {:016x} key {key}",
        fnv(&selected.concat()),
        fnv(&groups),
    )
}

fn rig() -> Rig {
    let mut rig = Rig::over(&cards(), false);
    let handles = rig.handles();
    for hidden in [4, 3] {
        rig.gpu
            .set_instance_visible(handles[hidden], false)
            .unwrap();
    }
    rig
}

#[test]
fn gpu_moving_still_edited_and_returning_views_select_and_compact_as_pinned() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let mut rig = rig();
    let classes = &mut Classes::default();
    let mut lines = vec![
        frame(&mut rig, classes, 0, [0.0, 3.0]),
        frame(&mut rig, classes, 0, [0.0, 3.0]),
        frame(&mut rig, classes, 6_000, [0.0, 3.0]),
    ];
    let handle = rig.handles()[0];
    let away = SceneTransform {
        position_millimeters: [-7_000, -12_000, 0],
        rotation: SceneRotation::IDENTITY,
        scale_millionths: 1_000_000,
    };
    rig.gpu.set_instance_transform(handle, away).unwrap();
    lines.push(frame(&mut rig, classes, 6_000, [0.0, 3.0]));
    lines.push(frame(&mut rig, classes, 6_000, [0.0, 3.0]));
    lines.push(frame(&mut rig, classes, 6_000, [0.0, 7.0]));
    lines.push(frame(&mut rig, classes, 0, [0.0, 3.0]));
    lines.push(frame(&mut rig, classes, 40_000, [-9.0, 3.0]));
    assert_eq!(lines, PINNED);
}

/// Per frame: views the camera's draw rebuilt, ranges it staged for the gather, views the
/// shadow selection rebuilt, a digest of every selected range and statistic, which views
/// share a key with an earlier frame (equal numbers), a digest of the committed groups and
/// the committed compaction key.
const PINNED: [&str; 8] = [
    "drawn 1 staged 6 shadows 2 selected 485171f387d80cd1 keys [0, 1, 2] groups 0cc10939f1a46287 key 3",
    "drawn 0 staged 0 shadows 0 selected 485171f387d80cd1 keys [0, 1, 2] groups 0cc10939f1a46287 key 3",
    "drawn 1 staged 5 shadows 2 selected 6849d66fbdc9a2a1 keys [4, 5, 6] groups c06fbce95648570d key 7",
    "drawn 1 staged 5 shadows 2 selected 6849d66fbdc9a2a1 keys [8, 9, 10] groups c06fbce95648570d key 11",
    "drawn 0 staged 0 shadows 0 selected 6849d66fbdc9a2a1 keys [8, 9, 10] groups c06fbce95648570d key 11",
    "drawn 0 staged 0 shadows 1 selected ae6013dc1171c07c keys [8, 9, 12] groups c06fbce95648570d key 11",
    "drawn 1 staged 6 shadows 2 selected 485171f387d80cd1 keys [13, 14, 15] groups 0cc10939f1a46287 key 16",
    "drawn 1 staged 0 shadows 2 selected 7d8457552ec98ee8 keys [17, 18, 19] groups 09612b07b5ecb5a5 key 20",
];
