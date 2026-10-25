use super::{
    residency::*,
    residency_tests::{context, settings},
    selection::PageKey,
};
use worldrunner_render_gpu::{configuration::AllocationBudget, submission::SubmissionAuthority};
use worldrunner_render_plan::TerrainPageSettings;

#[test]
fn terrain_pages_edits_cover_neighbour_gutters_and_preserve_remote_pages() {
    let context = context();
    let device = context.device();
    let authority = SubmissionAuthority::new(context.queue().clone());
    let mut parts = settings().parts();
    parts.upload_bytes_per_frame *= 4;
    let settings = TerrainPageSettings::new(parts).unwrap();
    let mut cache =
        ResidentPages::new(device, &authority, settings, 4, AllocationBudget(100_000)).unwrap();
    let left = PageKey::new(3, [2, 2]).unwrap();
    let right = left.neighbor([1, 0]).unwrap();
    let remote = PageKey::new(3, [7, 7]).unwrap();
    let frame = authority.begin(device).unwrap();
    for key in [PageKey::ROOT, left, right, remote] {
        cache.reserve(&frame, key).unwrap().stage(&frame).unwrap();
    }
    let submitted = frame.finish().submit().unwrap();
    let old: Vec<_> = [left, right, remote]
        .map(|key| cache.lookup(key, 0).unwrap())
        .into();
    assert_eq!(
        old.iter().map(|page| page.slot()).collect::<Vec<_>>(),
        [1, 2, 3]
    );
    cache.invalidate([0.374, 0.30], [0.374, 0.30]);
    assert!(cache.lookup(left, 0).is_none());
    assert!(cache.lookup(right, 0).is_none());
    assert!(!cache.is_current(old[0]));
    assert!(!cache.is_current(old[1]));
    assert!(cache.is_current(old[2]));
    device
        .poll(wgpu::PollType::Wait {
            submission_index: Some(submitted),
            timeout: None,
        })
        .unwrap();
    let frame = authority.begin(device).unwrap();
    cache.reserve(&frame, left).unwrap().stage(&frame).unwrap();
    frame.finish().submit().unwrap();
    assert!(!cache.is_current(old[0]));
    assert!(cache.lookup(left, 0).is_some());
    let before_replace = cache.lookup(left, 0).unwrap();
    cache.replace(device, 4).unwrap();
    assert!(!cache.is_current(before_replace));
    assert!(cache.lookup(left, 0).is_none());
}

#[test]
fn terrain_pages_all_neighbours_and_ancestor_indices_are_exact() {
    for depth in 0..=4 {
        let side = 1_u32 << depth;
        for y in 0..side {
            for x in 0..side {
                let key = PageKey::new(depth, [x, y]).unwrap();
                for offset in [[-1, 0], [1, 0], [0, -1], [0, 1], [-1, -1], [1, 1]] {
                    let neighbor = key.neighbor(offset);
                    let expected = x
                        .checked_add_signed(offset[0])
                        .zip(y.checked_add_signed(offset[1]))
                        .filter(|(x, y)| *x < side && *y < side);
                    assert_eq!(
                        neighbor.map(PageKey::coordinate),
                        expected.map(|(x, y)| [x, y])
                    );
                    if let Some(neighbor) = neighbor {
                        assert_eq!(neighbor.neighbor(offset.map(|v| -v)), Some(key));
                    }
                }
                for ancestor_depth in 0..=depth {
                    let ancestor = key.ancestor(ancestor_depth).unwrap();
                    let scale = 1_u32 << (depth - ancestor_depth);
                    assert_eq!(ancestor.coordinate(), [x / scale, y / scale]);
                    let expected =
                        [x % scale, y % scale].map(|v| (f64::from(v) + 0.5) / f64::from(scale));
                    assert_eq!(
                        key.uv_in_ancestor([0.5, 0.5], ancestor)
                            .unwrap()
                            .map(f64::to_bits),
                        expected.map(f64::to_bits)
                    );
                }
            }
        }
    }
}
