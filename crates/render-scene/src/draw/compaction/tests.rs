use std::ops::Range;

use std::collections::BTreeMap;

use super::gather::{SOURCE, run_bytes, workgroups};
use super::mirror::{Extent, Layout};
use super::plan::{Plan, RUN_RECORDS, Run};
use crate::draw::visibility::SelectedRange;
use crate::geometry::INSTANCE_BYTES;
use crate::instances::BatchKey;

const A: BatchKey = (0, 0, 0);
const B: BatchKey = (0, 0, 2);
const C: BatchKey = (1, 0, 1);

fn range(key: BatchKey, lod: usize, slots: Range<u32>) -> SelectedRange {
    SelectedRange { key, lod, slots }
}

#[test]
fn groups_follow_batch_then_lod_with_slot_order_inside_a_group() {
    let plan = Plan::new(&[
        range(A, 0, 0..2),
        range(A, 1, 2..3),
        range(A, 0, 4..6),
        range(B, 0, 0..1),
    ])
    .unwrap();
    let groups: Vec<_> = plan
        .groups
        .iter()
        .map(|group| (group.key, group.lod, group.slots.clone()))
        .collect();
    assert_eq!(groups, [(A, 0, 0..4), (A, 1, 4..5), (B, 0, 5..6)]);
    assert_eq!(plan.total, 6);
}

#[test]
fn every_selected_slot_is_copied_once_to_its_group_position() {
    let plan = Plan::new(&[
        range(A, 0, 0..2),
        range(A, 1, 2..3),
        range(A, 0, 4..6),
        range(B, 0, 0..1),
    ])
    .unwrap();
    let copies: Vec<_> = plan
        .copies
        .iter()
        .map(|copy| (copy.key, copy.source.clone(), copy.destination))
        .collect();
    assert_eq!(
        copies,
        [(A, 0..2, 0), (A, 4..6, 2), (A, 2..3, 4), (B, 0..1, 5)]
    );
}

#[test]
fn an_empty_selection_has_no_groups_and_no_copies() {
    let plan = Plan::new(&[]).unwrap();
    assert!(plan.groups.is_empty() && plan.copies.is_empty());
    assert_eq!(plan.total, 0);
}

#[test]
fn a_selection_beyond_the_draw_range_is_not_compacted() {
    assert!(Plan::new(&[range(A, 0, 0..u32::MAX), range(B, 0, 0..2)]).is_none());
}

fn run(source: u32, destination: u32, count: u32) -> Run {
    Run {
        source,
        destination,
        count,
    }
}

fn fragmented() -> Plan {
    Plan::new(&[
        range(A, 0, 0..2),
        range(A, 1, 2..3),
        range(A, 0, 4..6),
        range(B, 0, 0..1),
    ])
    .unwrap()
}

#[test]
fn runs_read_each_selected_range_at_its_batch_base_in_the_mirror() {
    let runs = fragmented().runs(|key| if key == A { 100 } else { 10 });
    assert_eq!(
        runs,
        [
            run(100, 0, 2),
            run(104, 2, 2),
            run(102, 4, 1),
            run(10, 5, 1)
        ]
    );
}

#[test]
fn ranges_adjacent_in_the_mirror_and_the_view_merge_into_one_run() {
    let plan = Plan::new(&[range(A, 0, 6..8), range(B, 0, 0..3), range(B, 0, 5..6)]).unwrap();
    // Batch B starts right after slot 7 of batch A in this mirror.
    let runs = plan.runs(|key| if key == A { 0 } else { 8 });
    assert_eq!(runs, [run(6, 0, 5), run(13, 5, 1)]);
}

#[test]
fn long_runs_split_into_bounded_workgroup_runs() {
    let plan = Plan::new(&[range(A, 0, 0..150)]).unwrap();
    let runs = plan.runs(|_| 7);
    assert_eq!(RUN_RECORDS, 64);
    assert_eq!(runs, [run(7, 0, 64), run(71, 64, 64), run(135, 128, 22)]);
}

#[test]
fn every_selected_record_is_gathered_once_to_its_group_position() {
    let plan = fragmented();
    let runs = plan.runs(|key| if key == A { 0 } else { 6 });
    let mut written = vec![None; plan.total as usize];
    for run in runs {
        for offset in 0..run.count {
            let slot = &mut written[(run.destination + offset) as usize];
            assert!(slot.replace(run.source + offset).is_none());
        }
    }
    assert_eq!(
        written,
        [0, 1, 4, 5, 2, 6].map(Some),
        "group order A/0, A/1, B/0"
    );
}

#[test]
fn the_run_list_is_a_count_then_little_endian_runs() {
    let bytes = run_bytes(&[run(1, 2, 3), run(0x0102_0304, 5, 6)]);
    let words: Vec<u32> = bytes
        .chunks_exact(4)
        .map(|word| u32::from_le_bytes(word.try_into().unwrap()))
        .collect();
    assert_eq!(words, [2, 1, 2, 3, 0x0102_0304, 5, 6]);
}

#[test]
fn workgroups_fold_into_rows_of_the_dispatch_limit() {
    assert_eq!(workgroups(5, 65_535), [5, 1, 1]);
    assert_eq!(workgroups(65_535, 65_535), [65_535, 1, 1]);
    assert_eq!(workgroups(70_000, 65_535), [65_535, 2, 1]);
    assert_eq!(workgroups(9, 4), [4, 3, 1]);
}

#[test]
#[should_panic(expected = "exceed the device's dispatch limit")]
fn a_dispatch_beyond_the_limit_fails_instead_of_dropping_runs() {
    workgroups(17, 4);
}

#[test]
fn the_mirror_appends_new_and_grown_batches_and_reports_a_full_mirror() {
    let mut layout = Layout {
        capacity: 16,
        ..Layout::default()
    };
    assert!(layout.reserve(A, 4));
    assert!(layout.reserve(B, 4));
    assert!(layout.reserve(A, 3), "a range that holds the batch stays");
    assert_eq!(layout.base(A), 0);
    assert!(layout.reserve(A, 8), "a grown batch moves to the end");
    assert_eq!(
        layout.extents[&A],
        Extent {
            base: 8,
            reserved: 8
        }
    );
    assert_eq!(layout.end, 16);
    assert!(!layout.reserve(C, 1));
    assert_eq!(layout.extents.len(), 2);
}

#[test]
fn a_packed_mirror_orders_batches_by_key_in_a_power_of_two() {
    let layout = Layout::packed(&BTreeMap::from([(C, 16), (A, 4), (B, 2)]));
    assert_eq!(layout.capacity, 32);
    assert_eq!(layout.end, 22);
    let bases: Vec<_> = layout.extents.iter().map(|(k, e)| (*k, e.base)).collect();
    assert_eq!(bases, [(A, 0), (B, 4), (C, 6)]);
}

#[test]
fn the_gather_shader_validates_and_copies_whole_instance_records() {
    let module = naga::front::wgsl::parse_str(SOURCE).unwrap();
    naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .unwrap();
    let words = INSTANCE_BYTES / 4;
    assert!(SOURCE.contains(&format!("const RECORD_WORDS: u32 = {words}u;")));
    let skin_words = crate::skinning::SKIN_INSTANCE_BYTES / 4;
    assert!(SOURCE.contains(&format!("const SKIN_WORDS: u32 = {skin_words}u;")));
    let entries: Vec<_> = module
        .entry_points
        .iter()
        .map(|entry| (entry.name.as_str(), entry.workgroup_size))
        .collect();
    assert_eq!(
        entries,
        [
            ("gather", [64, 1, 1]),
            ("gather_with_previous", [64, 1, 1]),
            ("gather_with_skin", [64, 1, 1]),
            ("gather_with_previous_and_skin", [64, 1, 1]),
        ]
    );
    assert!(SOURCE.contains("const LANES: u32 = 64u;"));
}

fn below(state: &mut u64, bound: u32) -> u32 {
    *state = state
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(1_442_695_040_888_963_407);
    u32::try_from(*state >> 33).expect("31 bits") % bound
}

/// Ranges as a selection lists them: batches one after another, ascending slots inside each,
/// and levels in whatever order the placements' distances give.
fn generated(seed: u64, batches: &[BatchKey]) -> Vec<SelectedRange> {
    let mut state = seed;
    let mut ranges = Vec::new();
    for &key in batches {
        let mut slot = 0;
        for _ in 0..below(&mut state, 9) {
            slot += below(&mut state, 4) + 1;
            let length = below(&mut state, 3) + 1;
            let lod = usize::try_from(below(&mut state, 3)).expect("small");
            ranges.push(range(key, lod, slot..slot + length));
            slot += length;
        }
    }
    ranges
}

/// Every copy of `ranges` in (batch, LOD) order as a stable sort of all of them gives it.
fn sorted_copies(ranges: &[SelectedRange]) -> Vec<(BatchKey, Range<u32>, u32)> {
    let mut order: Vec<&SelectedRange> = ranges.iter().collect();
    order.sort_by_key(|range| (range.key, range.lod));
    let mut total = 0;
    order
        .into_iter()
        .map(|range| {
            let copy = (range.key, range.slots.clone(), total);
            total += range.slots.end - range.slots.start;
            copy
        })
        .collect()
}

const BATCHES: [BatchKey; 5] = [A, B, C, (2, 0, 0), (2, 1, 5)];

#[test]
fn plans_equal_a_stable_sort_by_batch_and_level_in_any_batch_order() {
    for seed in 0..64 {
        for turn in [0, 2, 3] {
            // A turned list is not in key order, as only hand-built selections are.
            let mut batches = BATCHES;
            batches.rotate_left(turn);
            let ranges = generated(seed, &batches);
            let plan = Plan::new(&ranges).unwrap();
            let copies: Vec<_> = plan
                .copies
                .iter()
                .map(|copy| (copy.key, copy.source.clone(), copy.destination))
                .collect();
            assert_eq!(copies, sorted_copies(&ranges), "seed {seed} turn {turn}");
            let grouped: u32 = plan
                .groups
                .iter()
                .map(|group| group.slots.end - group.slots.start)
                .sum();
            assert_eq!(grouped, plan.total);
            let keys: Vec<_> = plan.groups.iter().map(|g| (g.key, g.lod)).collect();
            assert!(keys.windows(2).all(|pair| pair[0] < pair[1]), "{keys:?}");
        }
    }
}

#[test]
fn runs_look_up_each_batch_once_and_still_read_every_record_from_its_own_base() {
    let base = |key: BatchKey| 1_000 * u32::try_from(key.0 + key.1 + key.2).unwrap();
    for seed in 0..32 {
        let plan = Plan::new(&generated(seed, &BATCHES)).unwrap();
        let lookups = std::cell::Cell::new(0);
        let runs = plan.runs(|key| {
            lookups.set(lookups.get() + 1);
            base(key)
        });
        let mut distinct: Vec<BatchKey> = plan.copies.iter().map(|copy| copy.key).collect();
        distinct.dedup();
        assert_eq!(lookups.get(), distinct.len(), "seed {seed}");
        let mut read = BTreeMap::new();
        for run in runs {
            for offset in 0..run.count {
                assert!(
                    read.insert(run.destination + offset, run.source + offset)
                        .is_none()
                );
            }
        }
        let mut expected = BTreeMap::new();
        for copy in &plan.copies {
            for offset in 0..copy.source.end - copy.source.start {
                expected.insert(
                    copy.destination + offset,
                    base(copy.key) + copy.source.start + offset,
                );
            }
        }
        assert_eq!(read, expected, "seed {seed}");
    }
}
