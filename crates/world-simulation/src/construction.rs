//! Objects built in one batch stay out of lifecycle, searches and snapshots until published.
//!
//! A scene load or clone creates every object and component, binds script fields, then publishes.
//! Publishing queues `Awake`/`OnEnable` for all constructed objects under one mutation;
//! activating publishes and runs that work. Discarding removes everything the construction
//! created.
//!
//! A construction that is never published holds prefab templates: its objects take part in
//! nothing, and [`World::instantiate`] clones one into a separate, published construction.

use crate::{Advance, MutationId, ObjectDesc, ObjectHandle, World, WorldError};

/// An open batch of unpublished objects.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Construction(MutationId);

impl Construction {
    /// The mutation whose lifecycle work publishing queues.
    pub fn mutation(self) -> MutationId {
        self.0
    }
}

impl World {
    pub fn begin_construction(&mut self) -> Result<Construction, WorldError> {
        self.ensure_allocation_allowed()?;
        let mutation = self.reserve_mutation()?;
        self.lifecycle
            .unpublished
            .insert(mutation.sequence, Vec::new());
        Ok(Construction(mutation))
    }

    /// Creates an object that stays unpublished until `construction` is published.
    pub fn create_object_in(
        &mut self,
        construction: Construction,
        desc: ObjectDesc,
    ) -> Result<ObjectHandle, WorldError> {
        self.open_construction(construction)?;
        let object = self.create_object(desc)?;
        self.lifecycle
            .unpublished
            .get_mut(&construction.0.sequence)
            .expect("validated construction")
            .push(object);
        Ok(object)
    }

    /// Makes the constructed objects visible and queues their lifecycle work under
    /// `construction.mutation()`. Run the work with [`World::next_lifecycle`].
    pub fn publish_construction(&mut self, construction: Construction) -> Result<(), WorldError> {
        self.open_construction(construction)?;
        let nodes = self
            .lifecycle
            .unpublished
            .remove(&construction.0.sequence)
            .expect("validated construction");
        let live: Vec<_> = nodes
            .into_iter()
            .filter(|&object| self.live_object(object).is_ok())
            .collect();
        for &object in &live {
            self.spatial_touch(object);
        }
        self.refresh_lifecycle(&live, construction.0);
        Ok(())
    }

    /// Publishes the construction and runs its lifecycle work as [`World::next_lifecycle`] does.
    /// Scope checks precede publishing, so a rejected activation leaves the construction open.
    pub fn activate_construction(
        &mut self,
        construction: Construction,
    ) -> Result<Advance, WorldError> {
        self.check_lifecycle_scope()?;
        self.publish_construction(construction)?;
        self.open_lifecycle(construction.mutation())
    }

    /// Removes every object the construction created. Fails once any of their scripts has run.
    pub fn discard_construction(&mut self, construction: Construction) -> Result<(), WorldError> {
        let nodes = self.open_construction(construction)?.to_vec();
        let started = self.lifecycle.scripts.iter().any(|(script, state)| {
            state.entered != crate::lifecycle::EnteredState::Dormant
                && self
                    .components
                    .get(script.0)
                    .is_some_and(|component| nodes.contains(&component.object))
        });
        if started {
            return Err(WorldError::NotReady);
        }
        self.lifecycle.unpublished.remove(&construction.0.sequence);
        self.lifecycle.mutations.remove(&construction.0.sequence);
        for object in nodes {
            self.retire_subtree(object);
        }
        Ok(())
    }

    fn open_construction(&self, construction: Construction) -> Result<&[ObjectHandle], WorldError> {
        if construction.0.world != self.identity {
            return Err(WorldError::WrongMutation);
        }
        self.lifecycle
            .unpublished
            .get(&construction.0.sequence)
            .map(Vec::as_slice)
            .ok_or(WorldError::WrongMutation)
    }
}
