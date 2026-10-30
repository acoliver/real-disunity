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
}

impl World {
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
            | ComponentDesc::Tilemap(_)
            | ComponentDesc::Camera(_)
            | ComponentDesc::Light(_)
            | ComponentDesc::GlobalLight2d(_)
            | ComponentDesc::ShadowCaster2d
            | ComponentDesc::UniversalAdditionalCameraData(_)
            | ComponentDesc::AudioListener { .. }
            | ComponentDesc::GUILayer { .. }
            | ComponentDesc::FlareLayer { .. } => {}
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
        let components: Vec<_> = self
            .components
            .iter(self.identity)
            .filter(|(_, c)| nodes.contains(&c.object))
            .map(|(_, c)| c.handle)
            .collect();
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
        if let Some(state) = self.lifecycle.scripts.get_mut(&script) {
            self.coroutines.cancel_script(script);
            if state.active {
                self.lifecycle.cleanup.push(LifecycleWork {
                    script,
                    message: ScriptMessage::OnDisable,
                });
            }
            if state.awake {
                self.lifecycle.cleanup.push(LifecycleWork {
                    script,
                    message: ScriptMessage::OnDestroy,
                });
            }
            state.active = false;
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

    pub(crate) fn retire_component(&mut self, component: ComponentHandle) {
        self.lifetime.components.remove(&component);
        self.lifecycle.scripts.remove(&component);
        self.physics.targets.remove(&component);
        self.components.remove(component.0);
    }

    pub(crate) fn retire_subtree(&mut self, root: ObjectHandle) {
        let mut nodes = BTreeSet::new();
        let mut pending = vec![root];
        while let Some(object) = pending.pop() {
            nodes.insert(object);
            pending.extend(self.objects.iter(self.identity).filter_map(|(id, value)| {
                (value.parent == Some(object)).then_some(ObjectHandle(id))
            }));
        }
        let components: Vec<_> = self
            .components
            .iter(self.identity)
            .filter(|(_, component)| nodes.contains(&component.object))
            .map(|(_, component)| component.handle)
            .collect();
        for component in components {
            self.retire_component(component);
        }
        for object in nodes {
            self.lifetime.delayed.remove(&object);
            self.lifetime.objects.remove(&object);
            self.objects.remove(object.0);
        }
    }

    /// Phase storage retirement is separate from managed cleanup invocation.
    pub fn finish_phase(&mut self) {
        for component in std::mem::take(&mut self.lifetime.components) {
            self.retire_component(component);
        }
        for object in std::mem::take(&mut self.lifetime.objects) {
            self.lifetime.delayed.remove(&object);
            self.objects.remove(object.0);
        }
    }
}
