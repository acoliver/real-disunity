use crate::{
    ComponentDesc, ComponentError, ComponentHandle, LifecycleWork, ObjectHandle, ScriptMessage,
    World, WorldError,
};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DestroyWhen {
    EndOfFrame,
    AfterScaled(crate::TimeSpan),
}

#[derive(Clone, Default)]
pub(crate) struct Lifetime {
    pub delayed: BTreeMap<ObjectHandle, crate::TimeSpan>,
    pub objects: BTreeSet<ObjectHandle>,
    pub components: BTreeSet<ComponentHandle>,
    /// How many objects the world has removed, ever.
    pub removed: u64,
}

impl World {
    /// How many objects this world has removed since it was made. An object handle that was
    /// live stays live until this count changes, so a caller that keeps handles can skip
    /// checking them while it holds still.
    pub fn removed_objects(&self) -> u64 {
        self.lifetime.removed
    }

    pub fn destroy_component(&mut self, handle: ComponentHandle) -> Result<(), WorldError> {
        if self.lifetime.components.contains(&handle) {
            return Ok(());
        }
        let component = self.component(handle)?;
        match component.desc {
            ComponentDesc::Transform(_) => {
                return Err(WorldError::InvalidComponent(
                    ComponentError::RequiredTransform,
                ));
            }
            ComponentDesc::Renderer { .. } => {
                self.objects
                    .get_mut(component.object.0)
                    .expect("live component owner")
                    .renderer = None;
            }
            ComponentDesc::MeshInstance(_) => {
                self.objects
                    .get_mut(component.object.0)
                    .expect("live component owner")
                    .mesh_instance = None;
            }
            #[cfg(feature = "ui")]
            ComponentDesc::RectTransform(_)
            | ComponentDesc::Canvas(_)
            | ComponentDesc::CanvasGroup(_)
            | ComponentDesc::CanvasRenderer(_)
            | ComponentDesc::Text(_) => {}
            ComponentDesc::Script { .. }
            | ComponentDesc::Body2d(_)
            | ComponentDesc::Collider2d(_)
            | ComponentDesc::Sprite(_)
            | ComponentDesc::MeshRenderer(_)
            | ComponentDesc::MeshFilter(_)
            | ComponentDesc::Tilemap(_)
            | ComponentDesc::Camera(_)
            | ComponentDesc::Light(_)
            | ComponentDesc::GlobalLight2d(_)
            | ComponentDesc::ShadowCaster2d
            | ComponentDesc::UniversalAdditionalCameraData(_)
            | ComponentDesc::AudioListener { .. }
            | ComponentDesc::GUILayer { .. }
            | ComponentDesc::FlareLayer { .. }
            | ComponentDesc::Animator { .. }
            | ComponentDesc::Skybox { .. }
            | ComponentDesc::ParticleSystem
            | ComponentDesc::SortingGroup(_) => {}
        }
        self.queue_script_cleanup(handle);
        self.lifetime.components.insert(handle);
        Ok(())
    }

    /// Occupied native slots, including invalidated entries awaiting phase retirement.
    pub fn storage_counts(&self) -> (usize, usize) {
        (
            self.objects.iter(self.identity).count(),
            self.components.iter(self.identity).count(),
        )
    }

    /// Marks the subtree pending; handles remain usable until final frame retirement.
    pub fn destroy(&mut self, object: ObjectHandle, when: DestroyWhen) -> Result<(), WorldError> {
        if self.lifetime.objects.contains(&object) {
            return Ok(());
        }
        if let DestroyWhen::AfterScaled(delay) = when {
            self.live_object(object)?;
            let deadline = self.clock.time.checked_add(delay)?;
            self.lifetime
                .delayed
                .entry(object)
                .and_modify(|old| *old = (*old).min(deadline))
                .or_insert(deadline);
            return Ok(());
        }
        self.lifetime.delayed.remove(&object);
        let nodes = self.subtree(object)?;
        let components = self.owned_components(&nodes);
        for &script in &components {
            if !self.lifetime.components.contains(&script) {
                self.queue_script_cleanup(script);
            }
        }
        self.lifetime.objects.extend(nodes);
        self.lifetime.components.extend(components);
        Ok(())
    }

    fn queue_script_cleanup(&mut self, script: ComponentHandle) {
        if self
            .components
            .get(script.0)
            .is_some_and(|component| self.is_unpublished(component.object))
        {
            return;
        }
        if let Some(state) = self.lifecycle.scripts.get(&script) {
            let (active, awake) = (state.active, state.awake);
            self.coroutines.cancel_script(script);
            if active {
                self.lifecycle.cleanup.push(LifecycleWork {
                    script,
                    message: ScriptMessage::OnDisable,
                });
            }
            if awake {
                self.lifecycle.cleanup.push(LifecycleWork {
                    script,
                    message: ScriptMessage::OnDestroy,
                });
            }
            self.lifecycle
                .edit_script(script, |state| state.active = false);
        }
    }

    pub(crate) fn expire_destruction(&mut self) -> Result<(), WorldError> {
        let due: Vec<_> = self
            .lifetime
            .delayed
            .iter()
            .filter(|(_, deadline)| **deadline <= self.clock.time)
            .map(|(&object, _)| object)
            .collect();
        for object in due {
            self.destroy(object, DestroyWhen::EndOfFrame)?;
        }
        Ok(())
    }

    /// Removes a stored component. A handle whose component is already gone only drops its
    /// pending retirement mark: every other step reads the component by handle first, and the
    /// arena removes only the slot's current occupant.
    pub(crate) fn retire_component(&mut self, component: ComponentHandle) {
        self.lifetime.components.remove(&component);
        self.lifecycle.remove_script(component);
        self.physics.targets.remove(&component);
        self.note_component_removed(component);
        if let Some(owner) = self.components.get(component.0).map(|c| c.object)
            && let Some(object) = self.objects.get_mut(owner.0)
        {
            object.components.retain(|owned| *owned != component);
        }
        if let Some(stored) = self.components.get(component.0) {
            self.physics_population.note_retired(&stored.desc);
        }
        self.components.remove(component.0);
    }

    /// Handles of every stored component owned by `nodes`, in slot order.
    pub(crate) fn owned_components(&self, nodes: &[ObjectHandle]) -> Vec<ComponentHandle> {
        let mut owned: Vec<_> = nodes
            .iter()
            .filter_map(|node| self.objects.get(node.0))
            .flat_map(|object| object.components.iter().copied())
            .collect();
        owned.sort_unstable();
        owned
    }

    /// Removes a stored object. A handle whose object is already gone only drops its pending
    /// destruction marks, so the object that reused its slot is never taken.
    fn remove_object(&mut self, object: ObjectHandle) {
        self.lifetime.delayed.remove(&object);
        self.lifetime.objects.remove(&object);
        if self.objects.get(object.0).is_none() {
            return;
        }
        self.note_object_removed(object);
        self.clear_follow_links(object);
        let parent = self.objects.get(object.0).and_then(|stored| stored.parent);
        self.relink_child(object, parent, None);
        self.objects.remove(object.0);
        self.lifetime.removed += 1;
    }

    /// Removes `root` and everything below it. Children are read only from live objects, and a
    /// stale `root` removes nothing.
    pub(crate) fn retire_subtree(&mut self, root: ObjectHandle) {
        let mut nodes = Vec::new();
        let mut pending = vec![root];
        while let Some(object) = pending.pop() {
            nodes.push(object);
            if let Some(stored) = self.objects.get(object.0) {
                pending.extend(stored.children.iter().copied());
            }
        }
        nodes.sort_unstable();
        for component in self.owned_components(&nodes) {
            self.retire_component(component);
        }
        for object in nodes {
            self.remove_object(object);
        }
    }

    /// Phase storage retirement is separate from managed cleanup invocation.
    pub fn finish_phase(&mut self) {
        for component in std::mem::take(&mut self.lifetime.components) {
            self.retire_component(component);
        }
        for object in std::mem::take(&mut self.lifetime.objects) {
            self.remove_object(object);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        ComponentDesc, ComponentHandle, ObjectDesc, ObjectHandle, TimeSpan, World, WorldSettings,
    };

    struct Reuse {
        world: World,
        stale: (ObjectHandle, ComponentHandle),
        current: (ObjectHandle, ComponentHandle),
    }

    fn scripted(world: &mut World) -> (ObjectHandle, ComponentHandle) {
        let object = world.create_object(ObjectDesc::default()).unwrap();
        let script = ComponentDesc::Script {
            assembly: "Game".into(),
            type_name: "Script".into(),
        };
        let component = world.add_component(object, script).unwrap();
        (object, component)
    }

    /// A world where a scripted object was removed and a new one took its object and component
    /// slots, so the first pair of handles is stale.
    fn reused_slots() -> Reuse {
        let mut world = World::new(WorldSettings::default()).unwrap();
        let stale = scripted(&mut world);
        world.destroy_object(stale.0).unwrap();
        let current = scripted(&mut world);
        assert_eq!(current.0.0.slot, stale.0.0.slot);
        assert_eq!(current.1.0.slot, stale.1.0.slot);
        assert_ne!(current.0, stale.0);
        Reuse {
            world,
            stale,
            current,
        }
    }

    fn assert_current_intact(reuse: &Reuse) {
        let (object, component) = reuse.current;
        assert!(reuse.world.object_alive(object));
        assert!(reuse.world.component(component).is_ok());
        assert_eq!(reuse.world.storage_counts(), (1, 2));
    }

    #[test]
    fn retiring_a_stale_subtree_root_removes_nothing() {
        let mut reuse = reused_slots();
        let removed = reuse.world.removed_objects();
        reuse.world.retire_subtree(reuse.stale.0);
        assert_current_intact(&reuse);
        assert_eq!(reuse.world.removed_objects(), removed);
    }

    #[test]
    fn retiring_a_stale_component_removes_nothing() {
        let mut reuse = reused_slots();
        reuse.world.lifetime.components.insert(reuse.stale.1);
        reuse.world.retire_component(reuse.stale.1);
        assert_current_intact(&reuse);
        assert!(reuse.world.lifetime.components.is_empty());
        let owner = reuse.world.objects.get(reuse.current.0.0).unwrap();
        assert!(owner.components.contains(&reuse.current.1));
    }

    #[test]
    fn finishing_a_phase_skips_stale_marks() {
        let mut reuse = reused_slots();
        reuse.world.lifetime.objects.insert(reuse.stale.0);
        reuse.world.lifetime.components.insert(reuse.stale.1);
        reuse
            .world
            .lifetime
            .delayed
            .insert(reuse.stale.0, TimeSpan::ZERO);
        let removed = reuse.world.removed_objects();
        reuse.world.finish_phase();
        assert_current_intact(&reuse);
        assert_eq!(reuse.world.removed_objects(), removed);
        assert!(reuse.world.lifetime.objects.is_empty());
        assert!(reuse.world.lifetime.components.is_empty());
        assert!(reuse.world.lifetime.delayed.is_empty());
    }
}
