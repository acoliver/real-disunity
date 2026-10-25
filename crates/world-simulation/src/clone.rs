use crate::{
    ComponentDesc, ComponentHandle, MutationId, ObjectDesc, ObjectHandle, World, WorldError,
};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq)]
pub struct Instantiation {
    pub root: ObjectHandle,
    pub objects: BTreeMap<ObjectHandle, ObjectHandle>,
    pub components: BTreeMap<ComponentHandle, ComponentHandle>,
    pub mutation: MutationId,
}

impl World {
    /// All capacity and identity checks precede allocation. Activation follows field binding.
    pub fn instantiate(
        &mut self,
        template: ObjectHandle,
        parent: Option<ObjectHandle>,
    ) -> Result<Instantiation, WorldError> {
        let source = self.subtree(template)?;
        if let Some(parent) = parent {
            self.live_object(parent)?;
        }
        let components: Vec<_> = self
            .components
            .iter(self.identity)
            .filter(|(_, c)| {
                source.contains(&c.object) && !self.lifetime.components.contains(&c.handle)
            })
            .map(|(_, c)| c.clone())
            .collect();
        self.objects.check_additional(source.len())?;
        self.components.check_additional(components.len())?;
        let mutation = self.reserve_mutation()?;
        let mut objects = BTreeMap::new();
        let mut remap = BTreeMap::new();
        for &old in &source {
            let value = self.object(old)?;
            let new = self.create_object(ObjectDesc {
                name: value.name,
                tag: value.tag,
                layer: value.layer,
                active: value.active,
                local_transform: value.local_transform,
            })?;
            objects.insert(old, new);
            remap.insert(value.transform, self.object(new)?.transform);
        }
        self.clone_components(&components, &objects, &mut remap)?;
        for &old in &source {
            let target = objects[&old];
            let new_parent = if old == template {
                parent
            } else {
                self.parent(old)?.map(|p| objects[&p])
            };
            self.objects
                .get_mut(target.0)
                .expect("allocated clone")
                .parent = new_parent;
        }
        let nodes: Vec<_> = source.iter().map(|old| objects[old]).collect();
        self.refresh_lifecycle(&nodes, mutation);
        Ok(Instantiation {
            root: objects[&template],
            objects,
            components: remap,
            mutation,
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
        for old in source {
            if let Some(state) = self.lifecycle.scripts.get(&old.handle) {
                let mut state = state.clone();
                state.awake = false;
                state.active = false;
                for (_, value) in &mut state.fields {
                    value.remap(objects, remap);
                }
                self.lifecycle.scripts.insert(remap[&old.handle], state);
            }
        }
        Ok(())
    }
}
