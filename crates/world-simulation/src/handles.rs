use std::sync::atomic::{AtomicU64, Ordering};

use crate::{LimitKind, WorldError};

static NEXT_WORLD: AtomicU64 = AtomicU64::new(1);

#[cfg(test)]
thread_local! {
    static ARENA_VISITS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

pub(crate) fn next_world() -> Result<u64, WorldError> {
    NEXT_WORLD
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
            value.checked_add(1)
        })
        .map_err(|_| WorldError::CapacityExceeded(LimitKind::WorldIdentity))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct Identity {
    pub world: u64,
    pub slot: u32,
    pub generation: u32,
}

/// An object identity scoped to its originating world and slot generation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ObjectHandle(pub(crate) Identity);

/// A component identity, distinct from its owning object identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ComponentHandle(pub(crate) Identity);

pub(crate) struct Slot<T> {
    generation: u32,
    value: Option<T>,
}

pub(crate) struct Arena<T> {
    slots: Vec<Slot<T>>,
    limit: u32,
    kind: LimitKind,
}

impl<T> Arena<T> {
    pub const fn new(limit: u32, kind: LimitKind) -> Self {
        Self {
            slots: Vec::new(),
            limit,
            kind,
        }
    }

    pub fn check_additional(&self, count: usize) -> Result<(), WorldError> {
        let free: Vec<_> = self
            .slots
            .iter()
            .filter(|slot| slot.value.is_none())
            .take(count)
            .collect();
        if free.iter().any(|slot| slot.generation == u32::MAX) {
            return Err(WorldError::CapacityExceeded(LimitKind::Generation));
        }
        let append = count - free.len();
        if append > (self.limit as usize).saturating_sub(self.slots.len()) {
            return Err(WorldError::CapacityExceeded(self.kind));
        }
        Ok(())
    }

    pub fn vacancy(&self, world: u64) -> Result<Identity, WorldError> {
        let index = self.slots.iter().position(|slot| slot.value.is_none());
        let (slot, generation) = if let Some(index) = index {
            let generation = self.slots[index]
                .generation
                .checked_add(1)
                .ok_or(WorldError::CapacityExceeded(LimitKind::Generation))?;
            (index, generation)
        } else {
            if self.slots.len() >= self.limit as usize {
                return Err(WorldError::CapacityExceeded(self.kind));
            }
            (self.slots.len(), 1)
        };
        Ok(Identity {
            world,
            slot: u32::try_from(slot).map_err(|_| WorldError::CapacityExceeded(self.kind))?,
            generation,
        })
    }

    pub fn insert(&mut self, id: Identity, value: T) {
        let slot = Slot {
            generation: id.generation,
            value: Some(value),
        };
        if id.slot as usize == self.slots.len() {
            self.slots.push(slot);
        } else {
            self.slots[id.slot as usize] = slot;
        }
    }

    pub fn get(&self, id: Identity) -> Option<&T> {
        self.slots
            .get(id.slot as usize)
            .filter(|slot| slot.generation == id.generation)?
            .value
            .as_ref()
    }

    pub fn get_mut(&mut self, id: Identity) -> Option<&mut T> {
        self.slots
            .get_mut(id.slot as usize)
            .filter(|slot| slot.generation == id.generation)?
            .value
            .as_mut()
    }

    pub fn remove(&mut self, id: Identity) {
        self.slots[id.slot as usize].value = None;
    }

    pub fn iter(&self, world: u64) -> impl Iterator<Item = (Identity, &T)> {
        self.slots
            .iter()
            .enumerate()
            .filter_map(move |(index, slot)| {
                #[cfg(test)]
                ARENA_VISITS.with(|visits| visits.set(visits.get() + 1));
                slot.value.as_ref().map(|value| {
                    (
                        Identity {
                            world,
                            slot: u32::try_from(index).expect("arena bounds checked at allocation"),
                            generation: slot.generation,
                        },
                        value,
                    )
                })
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renderer_lookup_never_walks_component_slots() {
        use crate::{ComponentDesc, ObjectDesc, World, WorldSettings};

        let mut world = World::new(WorldSettings::default()).unwrap();
        let objects: Vec<_> = (0..128)
            .map(|_| world.create_object(ObjectDesc::default()).unwrap())
            .collect();
        let renderers: Vec<_> = objects
            .iter()
            .enumerate()
            .map(|(index, &object)| {
                if index % 2 == 0 {
                    Some(
                        world
                            .add_component(
                                object,
                                ComponentDesc::Renderer {
                                    material_slot: u32::try_from(index).unwrap(),
                                },
                            )
                            .unwrap(),
                    )
                } else {
                    None
                }
            })
            .collect();
        ARENA_VISITS.with(|visits| visits.set(0));
        for (&object, &renderer) in objects.iter().zip(&renderers) {
            assert_eq!(world.renderer(object).unwrap(), renderer);
        }
        assert_eq!(
            ARENA_VISITS.with(std::cell::Cell::get),
            0,
            "renderer lookup must use owned handles, not arena scans"
        );
    }

    #[test]
    fn renderer_index_tracks_removal_and_slot_reuse() {
        use crate::{ComponentDesc, ObjectDesc, World, WorldSettings};

        let mut world = World::new(WorldSettings::default()).unwrap();
        let object = world.create_object(ObjectDesc::default()).unwrap();
        let renderer = world
            .add_component(object, ComponentDesc::Renderer { material_slot: 7 })
            .unwrap();
        world.remove_component(renderer).unwrap();
        assert_eq!(world.renderer(object).unwrap(), None);
        let replacement = world
            .add_component(object, ComponentDesc::Renderer { material_slot: 9 })
            .unwrap();
        assert_ne!(renderer, replacement);
        assert_eq!(world.renderer(object).unwrap(), Some(replacement));
        assert!(world.component(renderer).is_err());
        world.destroy_object(object).unwrap();
        let next = world.create_object(ObjectDesc::default()).unwrap();
        assert!(world.renderer(object).is_err());
        assert_eq!(world.renderer(next).unwrap(), None);
        assert!(world.component(replacement).is_err());
    }

    #[test]
    fn exhausted_generation_fails_without_reusing_identity() {
        let mut arena = Arena::<()>::new(1, LimitKind::Objects);
        arena.slots.push(Slot {
            generation: u32::MAX,
            value: None,
        });
        assert_eq!(
            arena.vacancy(1),
            Err(WorldError::CapacityExceeded(LimitKind::Generation))
        );
        assert_eq!(arena.slots[0].generation, u32::MAX);
        assert!(arena.slots[0].value.is_none());
    }
}
