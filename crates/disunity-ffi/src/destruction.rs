use crate::DisunityWorld;
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeSet;

#[derive(Clone, Debug, Default)]
pub(crate) struct PendingDestruction {
    active: bool,
    objects: BTreeSet<u64>,
    components: BTreeSet<u64>,
}

impl PendingDestruction {
    pub(crate) fn is_active(&self) -> bool {
        self.active
    }
}

#[derive(Deserialize)]
#[serde(tag = "op", deny_unknown_fields)]
pub(crate) enum LifetimeRequest {
    #[serde(rename = "lifetime.begin")]
    Begin {},
    #[serde(rename = "lifetime.end")]
    End {},
}

impl LifetimeRequest {
    pub(crate) fn execute(self, world: &mut DisunityWorld) -> Result<Value, String> {
        match self {
            Self::Begin {} => {
                if world.destruction.active {
                    return Err("native lifetime scope is already active".into());
                }
                world.destruction.active = true;
            }
            Self::End {} => world.flush_destruction()?,
        }
        Ok(Value::Null)
    }
}

impl DisunityWorld {
    pub(crate) fn destroy_object(&mut self, handle: u64) -> Result<(), String> {
        self.object_index(handle)?;
        if self.destruction.active {
            self.destruction.objects.insert(handle);
            Ok(())
        } else {
            self.remove_object(handle)
        }
    }

    pub(crate) fn destroy_component(&mut self, handle: u64) -> Result<(), String> {
        let index = self.component_index(handle)?;
        if self.components[index].as_ref().unwrap().is_transform() {
            return Err("Transform cannot be removed".into());
        }
        if self.destruction.active {
            self.destruction.components.insert(handle);
        } else {
            self.components[index] = None;
        }
        Ok(())
    }

    fn flush_destruction(&mut self) -> Result<(), String> {
        if !self.destruction.active {
            return Err("native lifetime scope is not active".into());
        }
        let pending = std::mem::take(&mut self.destruction);
        for handle in pending.objects {
            // An earlier ancestor removal can retire another queued generation.
            if self.object_index(handle).is_ok() {
                self.remove_object(handle)?;
            }
        }
        for handle in pending.components {
            if let Ok(index) = self.component_index(handle) {
                self.components[index] = None;
            }
        }
        Ok(())
    }
}

impl DisunityWorld {
    pub(crate) fn pending_script(&self, component: u64, object: u64) -> bool {
        if self.destruction.components.contains(&component) {
            return true;
        }
        let mut current = Some(object);
        while let Some(id) = current {
            if self.destruction.objects.contains(&id) {
                return true;
            }
            current = self.metadata.get(&id).and_then(|m| m.parent);
        }
        false
    }
}
