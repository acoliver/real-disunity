//! Frames that bind the same schedule to other allocations rebind it without resolving again once
//! they have met the allocations: caller attachments that alternate, and snapshot banks that are
//! visited again or replaced by an identical reconfigure.
use super::*;
use crate::compositor::StockPlanStatistics;

impl Rig<'_> {
    /// The colour snapshot of every bank.
    fn banks(&mut self) -> [wgpu::Texture; 3] {
        std::array::from_fn(|slot| self.snapshot(slot))
    }

    fn read_banks(&mut self) -> [Vec<u8>; 3] {
        let banks = self.banks();
        std::array::from_fn(|slot| self.read(&banks[slot]))
    }
}

/// Which of the caller's attachments a frame is drawn into.
#[derive(Clone, Copy)]
enum Attachments {
    /// The colour and depth of the first allocation.
    First,
    /// The colour and depth of the second allocation.
    Second,
    /// The colour of the second allocation and the depth of the first.
    Mixed,
}

/// The counters after a frame and what the two colour allocations held.
struct Seen {
    counters: StockPlanStatistics,
    in_first: Vec<u8>,
    in_second: Vec<u8>,
}

/// Two caller allocations with identical descriptors, and a renderer to draw into them at one
/// frame slot.
struct Alternation<'a> {
    rig: Rig<'a>,
    first: Caller,
    second: Caller,
}

impl<'a> Alternation<'a> {
    fn new(context: &'a GpuContext, request: Option<SceneSsr>) -> Self {
        let mut rig = Rig::new(context, request);
        let own = rig.own_caller();
        let first = Caller::like(context.device(), &own);
        let second = Caller::like(context.device(), &own);
        assert_ne!(first.color, second.color);
        assert_ne!(first.depth, second.depth);
        Self { rig, first, second }
    }

    fn draw(&mut self, into: Attachments, pose: usize) -> Seen {
        let caller = match into {
            Attachments::First => self.first.clone(),
            Attachments::Second => self.second.clone(),
            Attachments::Mixed => Caller {
                color: self.second.color.clone(),
                depth: self.first.depth.clone(),
            },
        };
        self.rig.draw_into(0, viewpoint(pose), &caller);
        self.seen()
    }

    fn seen(&mut self) -> Seen {
        Seen {
            counters: self.rig.statistics(),
            in_first: self.rig.read(&self.first.color),
            in_second: self.rig.read(&self.second.color),
        }
    }
}

fn assert_blank(contents: &[u8], what: &str) {
    assert!(contents.iter().all(|byte| *byte == 0), "{what}");
}

fn assert_drawn(contents: &[u8], what: &str) {
    assert!(contents.iter().any(|byte| *byte != 0), "{what}");
}

fn first_attachments_build_the_schedule(rig: &mut Alternation<'_>) -> Seen {
    assert_blank(
        &rig.seen().in_second,
        "nothing has drawn into the second allocation yet",
    );
    let first = rig.draw(Attachments::First, 0);
    assert_eq!(
        first.counters.rebuilds, 1,
        "the first frame builds the schedule"
    );
    assert_eq!(first.counters.binding_resolutions, 1);
    assert_eq!(first.counters.reuses, 0);
    assert_drawn(&first.in_first, "the first frame draws into its attachment");
    assert_blank(&first.in_second, "a frame writes only its own attachment");
    first
}

fn other_attachments_resolve_once(rig: &mut Alternation<'_>, first: &Seen) -> Seen {
    let second = rig.draw(Attachments::Second, 1);
    let (before, now) = (first.counters, second.counters);
    assert_eq!(
        now.rebuilds, before.rebuilds,
        "other attachments of the same shape do not rebuild"
    );
    assert_eq!(
        now.binding_resolutions,
        before.binding_resolutions + 1,
        "the schedule is bound to the other allocations once"
    );
    assert_eq!(now.reuses, before.reuses);
    assert_ne!(now.binding, before.binding);
    assert_drawn(
        &second.in_second,
        "the second frame draws into its attachment",
    );
    assert_ne!(
        second.in_second, first.in_first,
        "the viewpoints must draw differently"
    );
    assert_eq!(
        second.in_first, first.in_first,
        "the second frame does not write the first allocation"
    );
    second
}

fn first_attachments_again_are_found(rig: &mut Alternation<'_>, seen: [&Seen; 2]) -> Seen {
    let [first, second] = seen;
    let third = rig.draw(Attachments::First, 2);
    let (before, now) = (second.counters, third.counters);
    assert_eq!(now.rebuilds, first.counters.rebuilds);
    assert_eq!(
        now.binding_resolutions, before.binding_resolutions,
        "the first allocations are met again, so their table is found"
    );
    assert_eq!(now.reuses, before.reuses + 1);
    assert_eq!(
        now.binding, first.counters.binding,
        "the frame draws with the table it was first resolved to"
    );
    assert!(third.in_first != first.in_first && third.in_first != second.in_second);
    assert_eq!(
        third.in_second, second.in_second,
        "the third frame does not write the second allocation"
    );
    third
}

/// The colour of the second allocation with the depth of the first has not been met, and is then.
fn a_new_pairing_resolves_once(rig: &mut Alternation<'_>, seen: [&Seen; 3]) {
    let [first, second, third] = seen;
    let fourth = rig.draw(Attachments::Mixed, 3);
    assert_eq!(fourth.counters.rebuilds, first.counters.rebuilds);
    assert_eq!(
        fourth.counters.binding_resolutions,
        third.counters.binding_resolutions + 1,
        "a pairing of attachments the frame has not met is a new table"
    );
    assert!(![first.counters.binding, second.counters.binding].contains(&fourth.counters.binding));
    let fifth = rig.draw(Attachments::Mixed, 3);
    assert_eq!(
        fifth.counters.binding_resolutions,
        fourth.counters.binding_resolutions
    );
    assert_eq!(fifth.counters.reuses, fourth.counters.reuses + 1);
    assert_eq!(fifth.counters.binding, fourth.counters.binding);
    assert_eq!(
        fifth.in_first, third.in_first,
        "frames into the second colour do not write the first"
    );
}

/// Draws into the first allocations, the second and the first again at one frame slot. The
/// three frames share a renderer, a slot, the resource names and the descriptors; only the
/// allocations differ.
fn alternating_caller_attachments(request: Option<SceneSsr>) {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let mut rig = Alternation::new(&context, request);
    let first = first_attachments_build_the_schedule(&mut rig);
    let second = other_attachments_resolve_once(&mut rig, &first);
    let third = first_attachments_again_are_found(&mut rig, [&first, &second]);
    a_new_pairing_resolves_once(&mut rig, [&first, &second, &third]);
}

#[test]
fn gpu_alternating_caller_attachments_rebind_the_forward_plan_without_resolving_again() {
    alternating_caller_attachments(None);
}

#[test]
fn gpu_alternating_caller_attachments_rebind_the_reflection_plan_without_resolving_again() {
    alternating_caller_attachments(Some(reflections()));
}

/// The viewpoint of each slot's next frame, which differs from the one its bank was last drawn
/// from, so a frame that writes the bank changes it.
struct Poses([usize; 3]);

impl Poses {
    fn next(&mut self, slot: usize) -> CameraPose {
        self.0[slot] = (self.0[slot] + 1) % 4;
        viewpoint(self.0[slot])
    }
}

/// A renderer with a water body and the viewpoints its frames are drawn from.
struct Banked<'a> {
    rig: Rig<'a>,
    poses: Poses,
}

impl Banked<'_> {
    /// One frame at `slot`, then the colour of every bank before and after it.
    fn visit(&mut self, slot: usize) -> ([Vec<u8>; 3], [Vec<u8>; 3]) {
        let before = self.rig.read_banks();
        self.rig.draw(slot, self.poses.next(slot));
        (before, self.rig.read_banks())
    }
}

/// A visit wrote the bank of its slot and no other.
fn assert_wrote_only(slot: usize, (before, after): &([Vec<u8>; 3], [Vec<u8>; 3])) {
    for other in 0..3 {
        if other == slot {
            assert_ne!(
                before[other], after[other],
                "the frame writes the snapshots of slot {slot}"
            );
        } else {
            assert_eq!(
                before[other], after[other],
                "the frame of slot {slot} does not write the snapshots of slot {other}"
            );
        }
    }
}

/// The renderer allocates the banks of all three slots when it is configured. The first cycle
/// resolves a table for each slot, and the second finds them.
fn warm_every_slot(rig: &mut Banked<'_>) -> [u64; 3] {
    let mut bindings = [0; 3];
    for cycle in 0..2 {
        for (slot, binding) in bindings.iter_mut().enumerate() {
            rig.rig.draw(slot, rig.poses.next(slot));
            if cycle == 1 {
                *binding = rig.rig.statistics().binding;
            }
        }
    }
    let warm = rig.rig.statistics();
    assert_eq!(warm.rebuilds, 1, "one schedule for every slot");
    assert_eq!(warm.binding_resolutions, 3, "one table for each slot");
    assert_eq!(warm.reuses, 3, "the second cycle finds all three");
    let distinct: std::collections::BTreeSet<_> = bindings.iter().collect();
    assert_eq!(distinct.len(), 3, "each slot binds to its own allocations");
    bindings
}

/// Revisits of warmed banks, including the same slot twice in a row.
fn revisits_find_their_tables(rig: &mut Banked<'_>, bindings: [u64; 3]) {
    let mut seen = rig.rig.statistics();
    for slot in [1, 2, 0, 0, 2, 1] {
        let frame = rig.visit(slot);
        let now = rig.rig.statistics();
        assert_eq!(now.rebuilds, seen.rebuilds);
        assert_eq!(
            now.binding_resolutions, seen.binding_resolutions,
            "slot {slot} is warmed, so its table is found"
        );
        assert_eq!(now.reuses, seen.reuses + 1);
        assert_eq!(now.binding, bindings[slot]);
        assert_wrote_only(slot, &frame);
        seen = now;
    }
}

/// The banks of a renderer that has been replaced, and what they held.
struct Replaced {
    banks: [wgpu::Texture; 3],
    held: [Vec<u8>; 3],
}

impl Replaced {
    fn assert_untouched(&self, rig: &Rig<'_>) {
        for (bank, held) in self.banks.iter().zip(&self.held) {
            assert_eq!(&rig.read(bank), held, "the replaced banks are not written");
        }
    }
}

/// The first cycle of a replacement: each slot binds to allocations the schedule has not met,
/// without rebuilding it. Returns the tables of the slots.
fn replacement_resolves_each_slot_once(
    rig: &mut Banked<'_>,
    old: &Replaced,
    warmed: [u64; 3],
) -> [u64; 3] {
    let replaced = rig.rig.statistics();
    // The replacement's banks hold nothing until a frame draws into them.
    let mut written: [Vec<u8>; 3] = std::array::from_fn(|_| vec![0; old.held[0].len()]);
    let mut fresh = [0; 3];
    for (slot, binding) in fresh.iter_mut().enumerate() {
        rig.rig.draw(slot, rig.poses.next(slot));
        let now = rig.rig.statistics();
        assert_eq!(
            now.rebuilds, replaced.rebuilds,
            "an identical configuration has the schedule of the one it replaces"
        );
        assert_eq!(
            now.binding_resolutions,
            replaced.binding_resolutions + slot as u64 + 1,
            "slot {slot} binds to allocations the schedule has not met"
        );
        assert_eq!(now.reuses, replaced.reuses);
        assert!(!warmed.contains(&now.binding));
        *binding = now.binding;
        let banks = rig.rig.banks();
        let after: [Vec<u8>; 3] = std::array::from_fn(|index| rig.rig.read(&banks[index]));
        assert_wrote_only(slot, &(written, after.clone()));
        written = after;
        assert!(
            banks
                .iter()
                .zip(&old.banks)
                .all(|(new, stale)| new != stale)
        );
        old.assert_untouched(&rig.rig);
    }
    fresh
}

fn second_cycle_finds_the_tables(rig: &mut Banked<'_>, old: &Replaced, fresh: [u64; 3]) {
    let cycled = rig.rig.statistics();
    for slot in [2, 0, 1, 1, 0, 2] {
        let frame = rig.visit(slot);
        let now = rig.rig.statistics();
        assert_eq!(now.rebuilds, cycled.rebuilds);
        assert_eq!(
            now.binding_resolutions, cycled.binding_resolutions,
            "the second cycle finds the tables of the first"
        );
        assert_eq!(now.binding, fresh[slot]);
        assert_wrote_only(slot, &frame);
        old.assert_untouched(&rig.rig);
    }
    assert_eq!(rig.rig.statistics().reuses, cycled.reuses + 6);
}

#[test]
fn gpu_warmed_snapshot_banks_hit_and_an_identical_reconfigure_resolves_each_slot_once() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let mut rig = Banked {
        rig: Rig::with_water(&context, Some(reflections())),
        poses: Poses([3; 3]),
    };
    let warmed = warm_every_slot(&mut rig);
    revisits_find_their_tables(&mut rig, warmed);

    // The test keeps the banks of the renderer it replaces, so nothing can be written to them
    // without the test seeing it.
    let old = Replaced {
        banks: rig.rig.banks(),
        held: rig.rig.read_banks(),
    };
    rig.rig.reconfigure(SIZE);
    let fresh = replacement_resolves_each_slot_once(&mut rig, &old, warmed);
    second_cycle_finds_the_tables(&mut rig, &old, fresh);
}
