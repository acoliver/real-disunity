use crate::{
    ComponentDesc, ComponentHandle, Construction, MutationId, ObjectDesc, ObjectHandle, World,
    WorldError,
};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq)]
pub struct Instantiation {
    pub root: ObjectHandle,
    pub objects: BTreeMap<ObjectHandle, ObjectHandle>,
    pub components: BTreeMap<ComponentHandle, ComponentHandle>,
    pub mutation: MutationId,
    construction: Construction,
}

impl Instantiation {
    pub fn construction(&self) -> Construction {
        self.construction
    }
}

impl World {
    /// Clones `template` and publishes it, queueing its lifecycle work under `mutation`.
    pub fn instantiate(
        &mut self,
        template: ObjectHandle,
        parent: Option<ObjectHandle>,
    ) -> Result<Instantiation, WorldError> {
        let clone = self.instantiate_unpublished(template, parent)?;
        self.publish_construction(clone.construction)?;
        Ok(clone)
    }

    /// Clones `template` into an open construction for foreign construction and field binding.
    /// All capacity and identity checks precede allocation.
    pub fn instantiate_unpublished(
        &mut self,
        template: ObjectHandle,
        parent: Option<ObjectHandle>,
    ) -> Result<Instantiation, WorldError> {
        self.ensure_allocation_allowed()?;
        let source = self.subtree(template)?;
        if let Some(parent) = parent {
            self.live_object(parent)?;
        }
        let components: Vec<_> = self
            .components
            .iter(self.identity)
            .filter(|(_, c)| source.contains(&c.object))
            .map(|(_, c)| c.clone())
            .collect();
        self.objects.check_additional(source.len())?;
        self.components.check_additional(components.len())?;
        let construction = self.begin_construction()?;
        let result = self.clone_into(construction, template, parent, &source, &components);
        if result.is_err() {
            self.discard_construction(construction)?;
        }
        result
    }

    /// Discards an unpublished clone when managed construction or field binding fails.
    pub fn discard_instantiation(&mut self, clone: &Instantiation) -> Result<(), WorldError> {
        self.discard_construction(clone.construction)
    }

    fn clone_into(
        &mut self,
        construction: Construction,
        template: ObjectHandle,
        parent: Option<ObjectHandle>,
        source: &[ObjectHandle],
        components: &[crate::ComponentSnapshot],
    ) -> Result<Instantiation, WorldError> {
        let mut objects = BTreeMap::new();
        let mut remap = BTreeMap::new();
        for &old in source {
            let value = self.object(old)?;
            // Unity names the instantiated root after its original with a `(Clone)` suffix.
            let name = if old == template {
                format!("{}(Clone)", value.name)
            } else {
                value.name
            };
            let new = self.create_object_in(
                construction,
                ObjectDesc {
                    name,
                    tag: value.tag,
                    layer: value.layer,
                    active: value.active,
                    local_transform: value.local_transform,
                },
            )?;
            objects.insert(old, new);
            remap.insert(value.transform, self.object(new)?.transform);
        }
        self.clone_components(components, &objects, &mut remap)?;
        for &old in source {
            let new_parent = if old == template {
                parent
            } else {
                self.parent(old)?.map(|p| objects[&p])
            };
            self.attach_parent(objects[&old], new_parent)?;
        }
        Ok(Instantiation {
            root: objects[&template],
            objects,
            components: remap,
            mutation: construction.mutation(),
            construction,
        })
    }

    fn clone_components(
        &mut self,
        source: &[crate::ComponentSnapshot],
        objects: &BTreeMap<ObjectHandle, ObjectHandle>,
        remap: &mut BTreeMap<ComponentHandle, ComponentHandle>,
    ) -> Result<(), WorldError> {
        for old in source {
            if !matches!(old.desc, ComponentDesc::Transform(_)) {
                let new = self.add_component(objects[&old.object], old.desc.clone())?;
                remap.insert(old.handle, new);
            }
        }
        #[cfg(feature = "ui")]
        for old in source {
            if let ComponentDesc::Canvas(mut canvas) = old.desc {
                if let crate::ui::CanvasMode::ScreenSpaceCamera { ref mut camera, .. } = canvas.mode
                {
                    *camera = objects.get(camera).copied().unwrap_or(*camera);
                }
                self.set_component(remap[&old.handle], ComponentDesc::Canvas(canvas))?;
            }
        }
        for old in source {
            if let Some(state) = self.lifecycle.scripts.get(&old.handle) {
                let mut state = state.clone();
                state.start = crate::lifecycle::StartState::Pending;
                state.awake = false;
                state.active = false;
                state.entered = crate::lifecycle::EnteredState::Dormant;
                for (_, value) in &mut state.fields {
                    value.remap(objects, remap);
                }
                self.lifecycle.scripts.insert(remap[&old.handle], state);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::{ComponentDesc, DestroyWhen, ObjectDesc, ParentSpace, World, WorldSettings};

    #[test]
    fn abort_owns_invalidated_subtree_without_waiting_for_phase_cleanup() {
        for finish_first in [false, true] {
            let mut world = World::new(WorldSettings::default()).unwrap();
            let template = world.create_object(ObjectDesc::default()).unwrap();
            world
                .add_component(
                    template,
                    ComponentDesc::Script {
                        assembly: "Example".into(),
                        type_name: "Actor".into(),
                    },
                )
                .unwrap();
            let child = world.create_object(ObjectDesc::default()).unwrap();
            let mutation = world
                .set_parent(child, Some(template), ParentSpace::KeepLocal)
                .unwrap();
            world.take_lifecycle_work(mutation).unwrap();
            let parent = world.create_object(ObjectDesc::default()).unwrap();
            let baseline = world.storage_counts();
            let clone = world
                .instantiate_unpublished(template, Some(parent))
                .unwrap();
            world.destroy(parent, DestroyWhen::EndOfFrame).unwrap();
            assert!(world.take_cleanup().is_empty());
            assert!(world.object(clone.root).is_ok());
            if finish_first {
                world.finish_phase();
            }
            world.discard_instantiation(&clone).unwrap();
            assert!(world.lifecycle.unpublished.is_empty());
            assert!(world.lifecycle.mutations.is_empty());
            let expected = if finish_first {
                (baseline.0 - 1, baseline.1 - 1)
            } else {
                baseline
            };
            assert_eq!(world.storage_counts(), expected);
            assert_eq!(world.lifecycle.scripts.len(), 1);
        }
    }
}
