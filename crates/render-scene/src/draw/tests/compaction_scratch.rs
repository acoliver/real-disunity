//! A view reuses the lists of its gather from frame to frame, and the gather it stages is the
//! one plain buffer copies of the selected ranges make, whatever the frames before it selected.
use super::compaction::{Rig, pose_at, unlit};
use super::compaction_gather::{assert_gather_equals_plain_copies, fragmented, hide_every_fifth};

const POSES: [i64; 6] = [0, 700, 1_400, 2_100, 2_800, 3_500];

#[test]
fn a_moving_view_reuses_its_scratch_lists_once_it_has_seen_its_largest_selection() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let mut rig = Rig::over(&fragmented(), false);
    hide_every_fifth(&mut rig.gpu);
    for x in POSES {
        rig.render(&unlit(pose_at(x)));
    }
    let warm = rig.gpu.scratch_buffers(0);
    assert!(
        warm.iter()
            .all(|&(pointer, capacity)| pointer != 0 && capacity > 0),
        "{warm:?}"
    );
    for _ in 0..3 {
        for x in POSES.into_iter().rev() {
            rig.render(&unlit(pose_at(x)));
        }
    }
    assert_eq!(rig.gpu.scratch_buffers(0), warm, "a list grew or moved");
    assert!(rig.gpu.views[0].compaction.observed.staged.get() > 0);
}

#[test]
fn a_big_then_small_then_big_selection_gathers_what_plain_copies_gather() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let mut rig = Rig::over(&fragmented(), false);
    hide_every_fifth(&mut rig.gpu);
    let mut selected = Vec::new();
    for x in [0, 16_000, 0, 16_000, 700] {
        rig.render(&unlit(pose_at(x)));
        selected.push(rig.gpu.published_ranges_and_groups(0).0);
        let gather = rig.gpu.committed_gather(0);
        assert_gather_equals_plain_copies(&rig.context, &gather, false);
    }
    let (big, small) = (selected[0], selected[1]);
    assert!(small > 0 && small < big, "{selected:?}");
    assert_eq!(selected[2], big);
    assert_eq!(selected[3], small);
}
