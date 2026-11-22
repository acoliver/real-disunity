use std::ops::Range;

use std::collections::BTreeMap;

use super::gather::{SOURCE, run_bytes, workgroups, write_run_bytes};
use super::mirror::{Extent, Layout};
use super::plan::{Gather, Plan, RUN_RECORDS, Run, Workspace};
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
fn the_mirror_is_asked_for_each_batch_once_however_many_ranges_it_holds() {
    let plan = fragmented();
    let asked = std::cell::Cell::new(0);
    let runs = plan.runs(|key| {
        asked.set(asked.get() + 1);
        if key == A { 100 } else { 10 }
    });
    assert_eq!(runs.len(), 4);
    assert_eq!(asked.get(), 2, "batches A and B");
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
fn plans_equal_a_stable_sort_when_only_a_late_part_of_the_batch_key_is_out_of_order() {
    // Each list is in order up to its last or middle key part, so a check that read only the
    // first parts would take the per-batch sort and lose the batch order.
    let lists: [[BatchKey; 5]; 3] = [
        [B, A, C, (2, 0, 0), (2, 1, 5)],
        [A, B, C, (2, 1, 5), (2, 0, 0)],
        [A, C, B, (2, 0, 0), (2, 1, 5)],
    ];
    for seed in 0..64 {
        for batches in lists {
            let ranges = generated(seed, &batches);
            let plan = Plan::new(&ranges).unwrap();
            let copies: Vec<_> = plan
                .copies
                .iter()
                .map(|copy| (copy.key, copy.source.clone(), copy.destination))
                .collect();
            assert_eq!(copies, sorted_copies(&ranges), "seed {seed} {batches:?}");
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

/// A layout holding the five `BATCHES` and `extra` more batches around them, whose ranges were
/// handed out in an order that has nothing to do with the keys, so a base is never a function
/// of how many keys come before it.
fn scrambled_layout(extra: usize, seed: u64) -> Layout {
    let mut keys: Vec<BatchKey> = BATCHES.to_vec();
    keys.extend((0..extra).map(|index| (index % 3, index / 3 % 2, 10 + index)));
    let mut state = seed;
    keys.sort_by_cached_key(|_| below(&mut state, 1 << 20));
    let mut layout = Layout {
        capacity: 1 << 24,
        ..Layout::default()
    };
    for (index, key) in keys.into_iter().enumerate() {
        assert!(layout.reserve(key, 70 + u32::try_from(index % 5).unwrap()));
    }
    layout
}

fn distinct_batches(plan: &Plan) -> usize {
    let mut keys: Vec<BatchKey> = plan.copies.iter().map(|copy| copy.key).collect();
    keys.dedup();
    keys.len()
}

#[test]
fn runs_from_an_ascending_cursor_equal_runs_from_an_indexed_lookup() {
    let mut compared = 0;
    // A few extra batches are walked past; hundreds are searched for.
    for extra in [0, 6, 600] {
        for seed in 0..32 {
            let layout = scrambled_layout(extra, seed);
            let plan = Plan::new(&generated(seed, &BATCHES)).unwrap();
            let mut bases = layout.bases(distinct_batches(&plan));
            let by_cursor = plan.runs(|key| bases.base(key));
            let by_index = plan.runs(|key| layout.base(key));
            assert_eq!(by_cursor, by_index, "extra {extra} seed {seed}");
            compared += by_index.len();
        }
    }
    assert!(compared > 1_000, "{compared} runs compared");
}

/// The run list as the shader reads it, decoded into destination record -> mirror record.
fn decoded(bytes: &[u8]) -> BTreeMap<u32, u32> {
    let words: Vec<u32> = bytes
        .chunks_exact(4)
        .map(|word| u32::from_le_bytes(word.try_into().unwrap()))
        .collect();
    assert_eq!(words.len(), 1 + 3 * words[0] as usize);
    let mut gathered = BTreeMap::new();
    for run in words[1..].chunks_exact(3) {
        assert!(run[2] > 0 && run[2] <= RUN_RECORDS);
        for offset in 0..run[2] {
            assert!(
                gathered.insert(run[1] + offset, run[0] + offset).is_none(),
                "a record is written twice"
            );
        }
    }
    gathered
}

#[test]
fn the_run_list_bytes_gather_each_record_from_where_a_plain_copy_reads_it() {
    for seed in 0..32 {
        let layout = scrambled_layout(30, seed);
        let plan = Plan::new(&generated(seed, &BATCHES)).unwrap();
        let mut cursor = layout.bases(distinct_batches(&plan));
        let runs = plan.runs(|key| cursor.base(key));
        let mut reference = BTreeMap::new();
        for copy in &plan.copies {
            for offset in 0..copy.source.end - copy.source.start {
                reference.insert(
                    copy.destination + offset,
                    layout.base(copy.key) + copy.source.start + offset,
                );
            }
        }
        assert_eq!(decoded(&run_bytes(&runs)), reference, "seed {seed}");
    }
}

#[test]
fn a_reused_run_byte_buffer_holds_only_the_latest_list() {
    let big: Vec<Run> = (0..300).map(|index| run(index * 7, index * 3, 5)).collect();
    let small = [run(9, 8, 7)];
    let mut bytes = Vec::new();
    write_run_bytes(&big, &mut bytes);
    assert_eq!(bytes, run_bytes(&big));
    write_run_bytes(&small, &mut bytes);
    assert_eq!(bytes, run_bytes(&small));
    write_run_bytes(&big, &mut bytes);
    assert_eq!(bytes, run_bytes(&big));
}

/// Everything a gather takes from a plan, as a fresh plan and a stable sort give it.
#[derive(Debug, PartialEq, Eq)]
struct Arranged {
    groups: Vec<(BatchKey, usize, Range<u32>)>,
    copies: Vec<Gather>,
    total: u32,
    keys: Vec<BatchKey>,
    runs: Vec<Run>,
}

fn arranged(work: &mut Workspace, ranges: &[SelectedRange], layout: &Layout) -> Option<Arranged> {
    if !work.arrange(ranges) {
        return None;
    }
    let mut bases = layout.bases(work.keys().len());
    let runs = work.runs(|key| bases.base(key)).to_vec();
    Some(Arranged {
        groups: work
            .plan()
            .groups
            .iter()
            .map(|group| (group.key, group.lod, group.slots.clone()))
            .collect(),
        copies: work.plan().copies.clone(),
        total: work.plan().total,
        keys: work.keys().to_vec(),
        runs,
    })
}

/// The same selection through a plan and the indexed lookup, which hold no earlier selection.
fn reference(ranges: &[SelectedRange], layout: &Layout) -> Option<Arranged> {
    let plan = Plan::new(ranges)?;
    let mut keys: Vec<BatchKey> = plan.copies.iter().map(|copy| copy.key).collect();
    keys.dedup();
    let runs = plan.runs(|key| layout.base(key));
    Some(Arranged {
        groups: plan
            .groups
            .iter()
            .map(|group| (group.key, group.lod, group.slots.clone()))
            .collect(),
        copies: plan.copies,
        total: plan.total,
        keys,
        runs,
    })
}

#[test]
fn a_reused_workspace_arranges_as_a_fresh_one_after_big_small_and_big_selections() {
    let overflow = [range(A, 0, 0..u32::MAX), range(B, 0, 0..2)];
    let mut turned = BATCHES;
    turned.rotate_left(2);
    for seed in 0..48 {
        let layout = scrambled_layout(8, seed);
        let mut reused = Workspace::default();
        let selections = [
            generated(seed, &BATCHES),
            generated(seed + 1, &BATCHES[..1]),
            generated(seed + 2, &turned),
            Vec::new(),
            overflow.to_vec(),
            generated(seed + 3, &BATCHES),
            generated(seed + 4, &BATCHES[2..4]),
        ];
        for (index, ranges) in selections.iter().enumerate() {
            let fresh = arranged(&mut Workspace::default(), ranges, &layout);
            let again = arranged(&mut reused, ranges, &layout);
            assert_eq!(again, fresh, "seed {seed} selection {index}");
            assert_eq!(again, reference(ranges, &layout), "seed {seed} {index}");
        }
    }
}

#[test]
fn a_workspace_reserves_for_its_selection_and_stops_growing_when_it_has_seen_the_largest() {
    let layout = scrambled_layout(8, 1);
    let selections: Vec<_> = (0..24).map(|seed| generated(seed, &BATCHES)).collect();
    let mut work = Workspace::default();
    for ranges in &selections {
        arranged(&mut work, ranges, &layout).unwrap();
        let [order, groups, copies, ..] = work.buffers();
        assert!(order.1 >= ranges.len() && groups.1 >= ranges.len() && copies.1 >= ranges.len());
    }
    let warm = work.buffers();
    for _ in 0..3 {
        for ranges in selections.iter().rev() {
            arranged(&mut work, ranges, &layout).unwrap();
        }
    }
    assert_eq!(work.buffers(), warm, "a buffer grew or moved");
}

#[test]
fn levels_of_batches_with_many_ranges_order_as_a_stable_sort_does() {
    // Past the length a small sort handles by insertion, equal keys keep their order only
    // when the sort is told to keep it.
    let long = |key: BatchKey, ranges: u32| -> Vec<SelectedRange> {
        (0..ranges)
            .map(|index| {
                range(
                    key,
                    usize::try_from(index % 3).unwrap(),
                    2 * index..2 * index + 1,
                )
            })
            .collect()
    };
    let in_key_order: Vec<_> = BATCHES[..3].iter().flat_map(|&key| long(key, 90)).collect();
    let mut out_of_order: Vec<_> = BATCHES[..3].iter().flat_map(|&key| long(key, 90)).collect();
    out_of_order.rotate_left(100);
    for ranges in [in_key_order, out_of_order] {
        let plan = Plan::new(&ranges).unwrap();
        let copies: Vec<_> = plan
            .copies
            .iter()
            .map(|copy| (copy.key, copy.source.clone(), copy.destination))
            .collect();
        assert_eq!(copies, sorted_copies(&ranges));
    }
}
