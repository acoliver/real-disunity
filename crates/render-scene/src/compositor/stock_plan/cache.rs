//! The cache of logical schedules and the binding tables resolved for them.
//!
//! A logical schedule is the entries, roles, occurrences, attachment resources and effects of a
//! frame. It is a function of its [`Key`] and nothing else, so it is built when the key first
//! appears and reused while it holds. A binding table is the checked views the schedule's
//! attachments select. It is a function of the allocations the frame is bound to, so each
//! schedule keeps the tables it has been resolved to, selected by [`Identity`].
use super::shape::{Identity, Shape};
use std::rc::Rc;
use worldrunner_render_post::{AttachmentView, FeatureDeclaration, PassDeclaration, ResourceRef};

/// Distinct schedules kept. A frame that alternates between a few cascade refresh patterns or
/// between a prefix and its suffix keeps each one.
const SCHEDULES: usize = 4;
/// Binding tables kept per schedule: one per frame slot and one spare for a replaced allocation.
const BINDINGS: usize = 6;

/// What a logical schedule is a function of.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Key {
    pub shape: Shape,
    /// The passes the caller authored, with their attachments and loads.
    pub passes: Vec<PassDeclaration>,
    /// The declarations of the caller's own features.
    pub features: Vec<FeatureDeclaration>,
    /// The resources of the views the caller supplied, which water reads the shadow cascades from.
    pub views: Vec<ResourceRef>,
    /// Whether a shadow binding accompanies the frame, which water reads.
    pub shadow: bool,
}

/// A frame's key and the allocations it is bound to.
pub(super) struct Probe {
    pub key: Key,
    pub identity: Identity,
}

/// A binding table: the views of a schedule, resolved against one identity.
struct Binding {
    serial: u64,
    identity: Identity,
    views: Rc<Vec<AttachmentView>>,
}

struct Entry<T> {
    key: Key,
    schedule: Rc<T>,
    bindings: Vec<Binding>,
}

/// What the cache holds for a frame.
pub(super) enum Lookup<T> {
    /// A schedule and a binding table for these allocations.
    Bound {
        schedule: Rc<T>,
        views: Rc<Vec<AttachmentView>>,
        binding: u64,
    },
    /// A schedule that has not been resolved to these allocations.
    Unbound,
    Absent,
}

/// The plans the renderer has built, most recently used first.
pub(super) struct Slots<T> {
    entries: Vec<Entry<T>>,
}

impl<T> Default for Slots<T> {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
        }
    }
}

/// What the cache did, for the frame statistics.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StockPlanStatistics {
    /// Schedules built because none cached matched what decides them, and every plan of a
    /// frame that has features the host registered, which are planned each frame.
    pub rebuilds: u64,
    /// Binding tables resolved: a schedule's first, and one for each allocation set it meets
    /// that it has no table for.
    pub binding_resolutions: u64,
    /// Frames that reused a schedule and a binding table.
    pub reuses: u64,
    /// The binding table the latest frame drew with. It changes when the frame is bound to
    /// different allocations, such as another frame slot or a replaced renderer.
    pub binding: u64,
}

/// The counters of a [`PlanCache`], kept apart from its slot lists so both can be borrowed.
#[derive(Default)]
pub(super) struct Counters {
    statistics: StockPlanStatistics,
    serial: u64,
}

impl Counters {
    /// A plan built without the cache: its schedule and its binding table are both new.
    pub(super) fn uncached(&mut self) {
        self.statistics.rebuilds += 1;
        self.resolved();
    }

    pub(super) fn reused(&mut self, binding: u64) {
        self.statistics.reuses += 1;
        self.statistics.binding = binding;
    }

    fn resolved(&mut self) {
        self.serial += 1;
        self.statistics.binding_resolutions += 1;
        self.statistics.binding = self.serial;
    }
}

/// The cache of a renderer, shared with the renderer that replaces it on reconfiguration.
#[derive(Default)]
pub(crate) struct PlanCache {
    pub(super) frames: Slots<super::LogicalSchedule>,
    pub(super) prefixes: Slots<super::LogicalPrefix>,
    pub(super) counters: Counters,
}

impl PlanCache {
    pub(crate) const fn statistics(&self) -> StockPlanStatistics {
        self.counters.statistics
    }
}

impl<T: PartialEq> Slots<T> {
    pub(super) fn lookup(&mut self, probe: &Probe) -> Lookup<T> {
        let Some(index) = self.entries.iter().position(|entry| entry.key == probe.key) else {
            return Lookup::Absent;
        };
        self.entries[..=index].rotate_right(1);
        let entry = &mut self.entries[0];
        let found = entry
            .bindings
            .iter()
            .position(|binding| binding.identity == probe.identity);
        match found {
            Some(position) => {
                entry.bindings[..=position].rotate_right(1);
                let binding = &entry.bindings[0];
                Lookup::Bound {
                    schedule: entry.schedule.clone(),
                    views: binding.views.clone(),
                    binding: binding.serial,
                }
            }
            None => Lookup::Unbound,
        }
    }

    /// Keeps a schedule built for `probe` and its binding table. A schedule equal to the one
    /// cached under the key is not a rebuild and keeps its identity.
    pub(super) fn store(
        &mut self,
        counters: &mut Counters,
        probe: Probe,
        built: (T, Vec<AttachmentView>),
    ) -> (Rc<T>, Rc<Vec<AttachmentView>>) {
        let (schedule, views) = built;
        let views = Rc::new(views);
        counters.resolved();
        let binding = Binding {
            serial: counters.serial,
            identity: probe.identity,
            views: views.clone(),
        };
        if let Some(index) = self.entries.iter().position(|entry| entry.key == probe.key) {
            self.entries[..=index].rotate_right(1);
            let entry = &mut self.entries[0];
            if *entry.schedule != schedule {
                // The key did not decide the schedule; what it missed is now part of the entry.
                counters.statistics.rebuilds += 1;
                entry.schedule = Rc::new(schedule);
                entry.bindings.clear();
            }
            entry.bindings.insert(0, binding);
            entry.bindings.truncate(BINDINGS);
            return (entry.schedule.clone(), views);
        }
        counters.statistics.rebuilds += 1;
        let schedule = Rc::new(schedule);
        self.entries.insert(
            0,
            Entry {
                key: probe.key,
                schedule: schedule.clone(),
                bindings: vec![binding],
            },
        );
        self.entries.truncate(SCHEDULES);
        (schedule, views)
    }
}
