//! Sparse last-colour transforms, independent of physical resource acknowledgement.
use std::{cell::RefCell, collections::BTreeSet, rc::Rc};

use worldrunner_render_gpu::submission::Publications;

use super::totals::Tally;
use crate::geometry::INSTANCE_BYTES as STRIDE;

type Bytes = [u8; STRIDE];

#[derive(Debug)]
struct VisibleSlot {
    visible: Option<Bytes>,
    previous: Option<Bytes>,
    generation: u64,
    revision: u64,
    completed: u64,
    reset: Option<u64>,
}

#[derive(Debug, Default)]
struct State {
    slots: Vec<Option<VisibleSlot>>,
    // Upload acknowledgement cannot consume work owed to a later visible colour frame.
    pending: BTreeSet<usize>,
    settling: BTreeSet<usize>,
    revision: u64,
    serial: u64,
    /// Rises with every change to what `History::ready` reads, from any caller.
    version: u64,
    /// The scene's tally of every batch's `version`.
    total: Tally,
}

impl State {
    fn changed(&mut self) {
        self.version += 1;
        self.total.add(1);
    }
}

#[derive(Debug, Default)]
pub(crate) struct History {
    state: Rc<RefCell<State>>,
    prepared: Option<(Publications, Candidate)>,
}

#[derive(Debug)]
struct Record {
    slot: usize,
    generation: u64,
    revision: u64,
    reset: Option<u64>,
    current: Bytes,
    previous: Bytes,
    write_previous: bool,
}

/// Immutable sparse bytes chosen during the existing batch upload traversal.
#[derive(Clone, Debug)]
pub(crate) struct Candidate {
    state: Rc<RefCell<State>>,
    serial: u64,
    records: Rc<[Record]>,
}

impl History {
    /// Counts this history's changes, past and future, in `total` as well.
    pub fn count_in(&mut self, total: &Tally) {
        let mut state = self.state.borrow_mut();
        total.add(state.version);
        state.total = total.clone();
    }

    pub fn add(&mut self, slot: usize) {
        let mut state = self.state.borrow_mut();
        state.changed();
        state.revision += 1;
        let revision = state.revision;
        let length = (slot + 1).max(state.slots.len());
        state.slots.resize_with(length, || None);
        state.slots[slot] = Some(VisibleSlot {
            visible: None,
            previous: None,
            generation: revision,
            revision,
            completed: 0,
            reset: Some(revision),
        });
        state.pending.insert(slot);
    }

    pub fn edit(&mut self, slot: usize) {
        let mut state = self.state.borrow_mut();
        state.changed();
        state.revision += 1;
        let revision = state.revision;
        state.slots[slot]
            .as_mut()
            .expect("live history slot")
            .revision = revision;
        state.pending.insert(slot);
    }

    pub fn remove(&mut self, slot: usize) {
        let mut state = self.state.borrow_mut();
        state.changed();
        state.slots[slot] = None;
        state.pending.remove(&slot);
        state.settling.remove(&slot);
    }

    pub fn reset(&mut self, slots: impl IntoIterator<Item = usize>) {
        for slot in slots {
            self.edit(slot);
            let mut state = self.state.borrow_mut();
            let slot = state.slots[slot].as_mut().expect("live history slot");
            slot.reset = Some(slot.revision);
        }
    }

    /// A counter that moves whenever anything `ready` reads changes: slots, pending and settling
    /// sets, physical previous bytes, the prepared candidate. Candidates and staged publications
    /// move it too, since they run after the batch's own edits.
    pub fn version(&self) -> u64 {
        self.state.borrow().version
    }

    /// Lets a publication that runs later record a change to what `ready` reads.
    pub fn changes(&self) -> Changes {
        Changes(self.state.clone())
    }

    pub fn has_pending(&self) -> bool {
        let state = self.state.borrow();
        !state.pending.is_empty() || !state.settling.is_empty()
    }

    /// Checks only the logical frontier against its immutable upload snapshot and actual bytes.
    pub fn ready(&self, bytes: &[u8], in_frame: bool, previous: bool) -> bool {
        let state = self.state.borrow();
        state.pending.iter().chain(&state.settling).all(|&index| {
            let slot = state.slots[index].as_ref().expect("live history frontier");
            let Some(record) = self.prepared.as_ref().and_then(|(_, c)| c.record(index)) else {
                return false;
            };
            if record.generation != slot.generation || record.revision != slot.revision {
                return false;
            }
            if !previous || in_frame {
                return true;
            }
            let current = &bytes[index * STRIDE..(index + 1) * STRIDE];
            let expected = if slot.reset.is_some() {
                current
            } else {
                slot.visible
                    .as_ref()
                    .map_or(current, |visible| visible.as_slice())
            };
            slot.previous
                .as_ref()
                .is_some_and(|physical| physical.as_slice() == expected)
        })
    }

    pub fn prepare(
        &mut self,
        bytes: &[u8],
        dirty: &BTreeSet<usize>,
        owner: &Publications,
    ) -> Candidate {
        let prior = self.prepared.as_ref().filter(|(p, _)| p.same_frame(owner));
        let mut state = self.state.borrow_mut();
        let frontier: BTreeSet<_> = state
            .pending
            .iter()
            .chain(&state.settling)
            .chain(dirty)
            .copied()
            .collect();
        state.serial += 1;
        state.changed();
        let records = frontier
            .into_iter()
            .filter_map(|slot| {
                let visible = state.slots.get(slot)?.as_ref()?;
                let current: Bytes = bytes[slot * STRIDE..(slot + 1) * STRIDE]
                    .try_into()
                    .expect("instance stride");
                let prior = prior.and_then(|(_, candidate)| candidate.record(slot));
                Some(Record::choose(
                    slot,
                    visible,
                    current,
                    state.settling.contains(&slot),
                    prior,
                ))
            })
            .collect::<Vec<_>>()
            .into();
        let candidate = Candidate {
            state: self.state.clone(),
            serial: state.serial,
            records,
        };
        drop(state);
        self.prepared = Some((owner.clone(), candidate.clone()));
        candidate
    }

    /// Full initialization may visit the allocation; ordinary updates visit only candidates.
    pub fn previous_uploads(
        &self,
        candidate: &Candidate,
        bytes: &[u8],
        full: bool,
    ) -> Vec<(u64, Vec<u8>)> {
        let mut uploads = Vec::new();
        if full {
            let state = self.state.borrow();
            for (slot, current) in bytes.chunks_exact(STRIDE).enumerate() {
                let previous = candidate
                    .record(slot)
                    .map(|record| &record.previous)
                    .or_else(|| state.slots.get(slot)?.as_ref()?.visible.as_ref());
                append(
                    &mut uploads,
                    slot,
                    previous.map_or(current, |bytes| bytes.as_slice()),
                );
            }
        } else {
            for record in candidate
                .records
                .iter()
                .filter(|record| record.write_previous)
            {
                append(&mut uploads, record.slot, &record.previous);
            }
        }
        uploads
    }
}

/// Records, from a publication that runs after the batch's own edits, that readiness may have
/// changed.
pub(crate) struct Changes(Rc<RefCell<State>>);

impl Changes {
    pub fn record(&self) {
        self.0.borrow_mut().changed();
    }
}

impl Record {
    fn choose(
        slot: usize,
        visible: &VisibleSlot,
        current: Bytes,
        settling: bool,
        prior: Option<&Self>,
    ) -> Self {
        let prior = prior.filter(|record| {
            record.generation == visible.generation && record.reset == visible.reset
        });
        let previous = if visible.reset.is_some() {
            current
        } else {
            prior.map_or(visible.visible.unwrap_or(current), |record| record.previous)
        };
        Self {
            slot,
            generation: visible.generation,
            revision: visible.revision,
            reset: visible.reset,
            current,
            previous,
            write_previous: (visible.reset.is_some() || settling || previous != current)
                && prior.is_none_or(|record| record.previous != previous),
        }
    }
}

impl Candidate {
    fn record(&self, slot: usize) -> Option<&Record> {
        self.records
            .binary_search_by_key(&slot, |record| record.slot)
            .ok()
            .map(|index| &self.records[index])
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// Acknowledges previous bytes only after the upload's actual encoder was enqueued.
    pub fn acknowledge_previous(&self, full: bool) {
        let mut state = self.state.borrow_mut();
        state.changed();
        for record in self
            .records
            .iter()
            .filter(|record| full || record.write_previous)
        {
            let Some(slot) = state.slots.get_mut(record.slot).and_then(Option::as_mut) else {
                continue;
            };
            if slot.generation == record.generation {
                slot.previous = Some(record.previous);
            }
        }
    }

    /// Called exclusively by a submitted final-colour publication.
    pub fn publish(self) {
        let mut state = self.state.borrow_mut();
        state.changed();
        for record in self.records.iter() {
            let Some(slot) = state.slots.get_mut(record.slot).and_then(Option::as_mut) else {
                continue;
            };
            if slot.generation != record.generation || slot.completed >= self.serial {
                continue;
            }
            slot.visible = Some(record.current);
            slot.completed = self.serial;
            if slot.reset == record.reset {
                slot.reset = None;
            }
            if slot.revision == record.revision {
                state.pending.remove(&record.slot);
            }
            if record.previous == record.current {
                state.settling.remove(&record.slot);
            } else {
                state.settling.insert(record.slot);
            }
        }
    }
}

fn append(uploads: &mut Vec<(u64, Vec<u8>)>, slot: usize, bytes: &[u8]) {
    let offset = (slot * STRIDE) as u64;
    if let Some((_, previous)) = uploads
        .last_mut()
        .filter(|(start, bytes)| *start + bytes.len() as u64 == offset)
    {
        previous.extend_from_slice(bytes);
    } else {
        uploads.push((offset, bytes.to_vec()));
    }
}

#[cfg(test)]
#[path = "history_tests.rs"]
mod tests;
