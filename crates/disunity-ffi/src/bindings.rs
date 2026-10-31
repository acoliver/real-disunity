//! The managed identities of a world's scripts and native components, kept in both directions.
//!
//! The host names a script or component by the identity it registered it with, and native work
//! names it by its component handle. Calls such as `disunity_script_enabled` come once per
//! managed `enabled` read, and a clone looks up each component it copies, so finding one side
//! from the other must not walk every binding.

use std::collections::{BTreeMap, HashMap, btree_map};

use worldrunner_world_simulation::ComponentHandle;

/// Script bindings by component handle, in handle order, and by managed identity. Each
/// identity names at most one script.
#[derive(Debug, Default)]
pub(crate) struct ScriptBindings {
    by_handle: BTreeMap<ComponentHandle, u64>,
    by_identity: HashMap<u64, ComponentHandle>,
}

impl ScriptBindings {
    /// Binds `script` to `identity`, replacing any earlier binding of either.
    pub fn insert(&mut self, script: ComponentHandle, identity: u64) {
        if let Some(old) = self.by_handle.insert(script, identity) {
            self.by_identity.remove(&old);
        }
        if let Some(previous) = self.by_identity.insert(identity, script)
            && previous != script
        {
            self.by_handle.remove(&previous);
        }
    }

    /// Removes the binding of `script` and returns its identity.
    pub fn remove(&mut self, script: &ComponentHandle) -> Option<u64> {
        let identity = self.by_handle.remove(script)?;
        self.by_identity.remove(&identity);
        Some(identity)
    }

    /// The identity `script` is bound to.
    pub fn get(&self, script: &ComponentHandle) -> Option<&u64> {
        self.by_handle.get(script)
    }

    /// The script `identity` names.
    pub fn handle_of(&self, identity: u64) -> Option<ComponentHandle> {
        self.by_identity.get(&identity).copied()
    }

    pub fn contains_identity(&self, identity: u64) -> bool {
        self.by_identity.contains_key(&identity)
    }

    /// The bound scripts in handle order.
    pub fn keys(&self) -> btree_map::Keys<'_, ComponentHandle, u64> {
        self.by_handle.keys()
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.by_handle.len()
    }

    /// Keeps the bindings whose script `keep` accepts.
    pub fn retain(&mut self, mut keep: impl FnMut(&ComponentHandle) -> bool) {
        let by_identity = &mut self.by_identity;
        self.by_handle.retain(|script, identity| {
            let kept = keep(script);
            if !kept {
                by_identity.remove(identity);
            }
            kept
        });
    }
}

/// Native component bindings by managed identity, in identity order, and by component handle.
/// Each handle has at most one identity.
#[derive(Debug, Default)]
pub(crate) struct ComponentBindings {
    by_identity: BTreeMap<u64, ComponentHandle>,
    by_handle: HashMap<ComponentHandle, u64>,
}

impl ComponentBindings {
    /// Binds `identity` to `component`, replacing any earlier binding of either.
    pub fn insert(&mut self, identity: u64, component: ComponentHandle) {
        if let Some(old) = self.by_identity.insert(identity, component) {
            self.by_handle.remove(&old);
        }
        if let Some(previous) = self.by_handle.insert(component, identity)
            && previous != identity
        {
            self.by_identity.remove(&previous);
        }
    }

    /// Removes the binding of `identity` and returns its component.
    pub fn remove(&mut self, identity: &u64) -> Option<ComponentHandle> {
        let component = self.by_identity.remove(identity)?;
        self.by_handle.remove(&component);
        Some(component)
    }

    /// The component `identity` names.
    pub fn get(&self, identity: &u64) -> Option<&ComponentHandle> {
        self.by_identity.get(identity)
    }

    /// The identity `component` is bound to.
    pub fn identity_of(&self, component: ComponentHandle) -> Option<u64> {
        self.by_handle.get(&component).copied()
    }

    /// The bindings in identity order.
    pub fn iter(&self) -> btree_map::Iter<'_, u64, ComponentHandle> {
        self.by_identity.iter()
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.by_identity.len()
    }

    /// Keeps the bindings whose component `keep` accepts.
    pub fn retain(&mut self, mut keep: impl FnMut(&ComponentHandle) -> bool) {
        let by_handle = &mut self.by_handle;
        self.by_identity.retain(|_, component| {
            let kept = keep(component);
            if !kept {
                by_handle.remove(component);
            }
            kept
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use worldrunner_world_simulation::{ComponentDesc, ObjectDesc, World, WorldSettings};

    fn scripts(count: usize) -> Vec<ComponentHandle> {
        let mut world = World::new(WorldSettings::default()).unwrap();
        let owner = world.create_object(ObjectDesc::default()).unwrap();
        (0..count)
            .map(|n| {
                let desc = ComponentDesc::Script {
                    assembly: "Managed".into(),
                    type_name: n.to_string(),
                };
                world.add_component(owner, desc).unwrap()
            })
            .collect()
    }

    #[test]
    fn both_directions_agree_after_inserts_and_removals() {
        let [a, b, c] = scripts(3)[..] else { panic!() };
        let mut bindings = ScriptBindings::default();
        bindings.insert(a, 10);
        bindings.insert(b, 20);
        bindings.insert(c, 30);
        assert_eq!(bindings.handle_of(20), Some(b));
        assert_eq!(bindings.get(&c), Some(&30));
        assert_eq!(bindings.remove(&b), Some(20));
        assert_eq!(bindings.remove(&b), None);
        assert_eq!(bindings.handle_of(20), None);
        assert!(!bindings.contains_identity(20) && bindings.contains_identity(30));
        assert_eq!(bindings.keys().copied().collect::<Vec<_>>(), [a, c]);
        assert_eq!(bindings.len(), 2);
    }

    #[test]
    fn rebinding_a_script_or_an_identity_drops_the_old_binding() {
        let [a, b] = scripts(2)[..] else { panic!() };
        let mut bindings = ScriptBindings::default();
        bindings.insert(a, 10);
        bindings.insert(a, 11);
        assert_eq!(
            (bindings.handle_of(10), bindings.handle_of(11)),
            (None, Some(a))
        );
        bindings.insert(b, 11);
        assert_eq!(bindings.get(&a), None);
        assert_eq!(bindings.handle_of(11), Some(b));
        assert_eq!(bindings.len(), 1);
    }

    #[test]
    fn retain_drops_both_directions() {
        let [a, b, c] = scripts(3)[..] else { panic!() };
        let mut bindings = ScriptBindings::default();
        for (script, identity) in [(a, 1), (b, 2), (c, 3)] {
            bindings.insert(script, identity);
        }
        bindings.retain(|script| *script != b);
        assert_eq!(bindings.handle_of(2), None);
        assert_eq!(bindings.handle_of(3), Some(c));
        assert_eq!(bindings.keys().copied().collect::<Vec<_>>(), [a, c]);
    }

    #[test]
    fn component_bindings_agree_in_both_directions() {
        let [a, b, c] = scripts(3)[..] else { panic!() };
        let mut bindings = ComponentBindings::default();
        bindings.insert(7, a);
        bindings.insert(3, b);
        assert_eq!(bindings.identity_of(b), Some(3));
        assert_eq!(bindings.get(&7), Some(&a));
        bindings.insert(7, c);
        assert_eq!(
            (bindings.identity_of(a), bindings.identity_of(c)),
            (None, Some(7))
        );
        bindings.insert(9, b);
        assert_eq!((bindings.get(&3), bindings.identity_of(b)), (None, Some(9)));
        assert_eq!(bindings.remove(&9), Some(b));
        assert_eq!(bindings.identity_of(b), None);
        bindings.insert(1, a);
        bindings.retain(|component| *component != c);
        assert_eq!(bindings.identity_of(c), None);
        assert_eq!(bindings.iter().map(|(&id, _)| id).collect::<Vec<_>>(), [1]);
        assert_eq!(bindings.len(), 1);
    }
}
