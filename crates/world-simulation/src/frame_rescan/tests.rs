use super::reference::assert_frame_start_matches;
use super::walk::Walk;

#[test]
fn a_long_mixed_history_leaves_the_world_as_the_full_rescan_would() {
    for seed in 0..6 {
        let mut walk = Walk::new(seed);
        for _ in 0..200 {
            if walk.next(3) == 0 {
                walk.advance();
            } else if walk.next(5) == 0 {
                walk.idle_mutation();
            } else {
                walk.mutate();
            }
            assert_frame_start_matches(&walk.world);
            walk.world.lifecycle.assert_index_matches_scripts();
        }
    }
}
