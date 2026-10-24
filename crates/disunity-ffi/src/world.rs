use crate::{
    DisunityObject, DisunityWorld,
    components::{Component, Fields},
    make_handle, next_generation,
};
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub(crate) struct ObjectMetadata {
    pub name: String,
    pub tag: String,
    pub layer: u8,
    pub active: bool,
    pub parent: Option<u64>,
}
impl Default for ObjectMetadata {
    fn default() -> Self {
        Self {
            name: "GameObject".into(),
            tag: "Untagged".into(),
            layer: 0,
            active: true,
            parent: None,
        }
    }
}

impl DisunityWorld {
    pub(crate) fn allocate_object(&mut self) -> u64 {
        let generation = next_generation();
        let index = if let Some(i) = self.objects.iter().position(Option::is_none) {
            self.objects[i] = Some(DisunityObject::default());
            self.object_generations[i] = generation;
            i
        } else {
            self.objects.push(Some(DisunityObject::default()));
            self.object_generations.push(generation);
            self.objects.len() - 1
        };
        let h = make_handle(index, generation);
        self.metadata.insert(h, ObjectMetadata::default());
        self.add_component(h, "Transform".into(), json!({}))
            .expect("new transform");
        h
    }
    pub(crate) fn set_metadata(
        &mut self,
        object: u64,
        name: Option<String>,
        tag: Option<String>,
        active: Option<bool>,
        layer: Option<u8>,
    ) -> Result<(), String> {
        self.object_index(object)?;
        if layer.is_some_and(|v| v > 31) {
            return Err("layer must be in 0..31".into());
        }
        let m = self.metadata.get_mut(&object).unwrap();
        if let Some(layer) = layer {
            m.layer = layer;
        }
        if let Some(n) = name {
            m.name = n;
        }
        if let Some(t) = tag {
            m.tag = t;
        }
        if let Some(a) = active {
            m.active = a;
        }
        Ok(())
    }
    pub(crate) fn find_objects(
        &self,
        name: Option<String>,
        tag: Option<String>,
        kind: Option<String>,
        include_inactive: bool,
    ) -> Result<Value, String> {
        let mut out = Vec::new();
        for (i, o) in self.objects.iter().enumerate() {
            if o.is_none() {
                continue;
            }
            let h = crate::make_handle(i, self.object_generations[i]);
            let m = &self.metadata[&h];
            if (!include_inactive && !self.active(h))
                || name.as_ref().is_some_and(|n| n != &m.name)
                || tag.as_ref().is_some_and(|t| t != &m.tag)
                || kind.as_ref().is_some_and(|k| {
                    !self.components.iter().flatten().any(|c| {
                        c.object == h
                            && (c.matches_kind(k)
                                || (c.kind == "Script"
                                    && c.fields.value()["type"].as_str() == Some(k)))
                    })
                })
            {
                continue;
            }
            out.push(self.object_snapshot(h)?);
        }
        Ok(json!(out))
    }
    pub(crate) fn children(&self, h: u64) -> Vec<u64> {
        self.metadata
            .iter()
            .filter_map(|(id, m)| (m.parent == Some(h)).then_some(*id))
            .collect()
    }
    pub(crate) fn active(&self, h: u64) -> bool {
        let mut current = Some(h);
        let mut depth = 0;
        while let Some(h) = current {
            let Some(m) = self.metadata.get(&h) else {
                return false;
            };
            if !m.active {
                return false;
            }
            current = m.parent;
            depth += 1;
            if depth > 256 {
                return false;
            }
        }
        true
    }
    pub(crate) fn object_snapshot(&self, h: u64) -> Result<Value, String> {
        self.object_index(h)?;
        let m = &self.metadata[&h];
        let mut value = json!({"handle":h,"name":m.name,"tag":m.tag,"layer":m.layer,"active":m.active,"active_in_hierarchy":self.active(h)});
        value["transform"] = self.transform_snapshot(h)?;
        value["components"] = Value::Array(
            self.components
                .iter()
                .flatten()
                .filter(|c| c.object == h)
                .map(Component::snapshot)
                .collect(),
        );
        Ok(value)
    }
    pub(crate) fn remove_object(&mut self, h: u64) -> Result<(), String> {
        let i = self.object_index(h)?;
        for child in self.children(h) {
            self.remove_object(child)?;
        }
        for c in &mut self.components {
            if c.as_ref().is_some_and(|c| c.object == h) {
                *c = None;
            }
        }
        self.metadata.remove(&h);
        self.objects[i] = None;
        Ok(())
    }
    pub(crate) fn component_index(&self, h: u64) -> Result<usize, String> {
        let i = (h & 0xffffffff) as usize;
        if self
            .components
            .get(i)
            .and_then(Option::as_ref)
            .is_some_and(|c| c.handle == h)
        {
            Ok(i)
        } else {
            Err("component handle is stale or belongs to another world".into())
        }
    }
    pub(crate) fn add_component(
        &mut self,
        object: u64,
        kind: String,
        fields: Value,
    ) -> Result<u64, String> {
        self.object_index(object)?;
        let fields = Fields::parse(&kind, fields)?;
        if kind == "RectTransform"
            && let Some(c) = self
                .components
                .iter_mut()
                .flatten()
                .find(|c| c.object == object && c.kind == "Transform")
        {
            c.kind = kind;
            c.fields = fields;
            return Ok(c.handle);
        }
        if kind != "Script"
            && self
                .components
                .iter()
                .flatten()
                .any(|c| c.object == object && c.matches_kind(&kind))
        {
            return Err("duplicate singleton component".into());
        }
        let i = self
            .components
            .iter()
            .position(Option::is_none)
            .unwrap_or(self.components.len());
        let handle = make_handle(i, next_generation());
        let c = Some(Component {
            handle,
            object,
            kind,
            enabled: true,
            fields,
        });
        if i == self.components.len() {
            self.components.push(c);
        } else {
            self.components[i] = c;
        }
        Ok(handle)
    }
    pub(crate) fn patch_component(
        &mut self,
        component: u64,
        enabled: Option<bool>,
        fields: Option<Value>,
    ) -> Result<Value, String> {
        let i = self.component_index(component)?;
        let c = self.components[i].as_mut().unwrap();
        if let Some(fields) = fields {
            let patch = fields.as_object().ok_or("fields must be an object")?;
            let mut merged = c.fields.value();
            for (k, v) in patch {
                merged.as_object_mut().unwrap().insert(k.clone(), v.clone());
            }
            let mut parsed = Fields::parse(&c.kind, merged)?;
            if let (Fields::Body(old), Fields::Body(new)) = (&c.fields, &mut parsed) {
                new.move_position = old.move_position;
                new.force = old.force;
                if patch.contains_key("velocity") || patch.contains_key("angular_velocity") {
                    new.sleeping = false;
                }
            }
            c.fields = parsed;
        }
        if let Some(e) = enabled {
            if c.is_transform() && !e {
                return Err("Transform cannot be disabled".into());
            }
            c.enabled = e;
        }
        Ok(c.snapshot())
    }
    fn clone_nodes(
        &mut self,
        source: &[u64],
        remap: &mut BTreeMap<u64, u64>,
    ) -> Result<(), String> {
        for old in source {
            let new = self.allocate_object();
            remap.insert(*old, new);
            let old_i = self.object_index(*old)?;
            let new_i = self.object_index(new)?;
            self.objects[new_i] = self.objects[old_i];
            let mut m = self.metadata[old].clone();
            m.parent = None;
            self.metadata.insert(new, m);
            let old_transform = self
                .components
                .iter()
                .flatten()
                .find(|c| c.object == *old && c.is_transform())
                .unwrap()
                .handle;
            let new_transform = self
                .components
                .iter()
                .flatten()
                .find(|c| c.object == new && c.is_transform())
                .unwrap()
                .handle;
            let old_rect = self
                .components
                .iter()
                .flatten()
                .find(|c| c.handle == old_transform && c.kind == "RectTransform")
                .map(|c| c.fields.value());
            if let Some(fields) = old_rect {
                self.add_component(new, "RectTransform".into(), fields)?;
            }
            remap.insert(old_transform, new_transform);
        }
        Ok(())
    }
    pub(crate) fn clone_object(&mut self, root: u64, parent: Option<u64>) -> Result<Value, String> {
        self.object_index(root)?;
        if let Some(p) = parent {
            self.object_index(p)?;
        }
        let mut source = vec![root];
        let mut index = 0;
        while index < source.len() {
            source.extend(self.children(source[index]));
            index += 1;
        }
        let mut remap = BTreeMap::new();
        self.clone_nodes(&source, &mut remap)?;
        let old_components = self
            .components
            .iter()
            .flatten()
            .filter(|c| source.contains(&c.object) && !c.is_transform())
            .cloned()
            .collect::<Vec<_>>();
        for c in &old_components {
            let h = self.add_component(remap[&c.object], c.kind.clone(), c.fields.value())?;
            remap.insert(c.handle, h);
            self.components[(h & 0xffffffff) as usize]
                .as_mut()
                .unwrap()
                .enabled = c.enabled;
        }
        for old in &source {
            let p = if *old == root {
                parent
            } else {
                self.metadata[old].parent.map(|p| remap[&p])
            };
            self.metadata.get_mut(&remap[old]).unwrap().parent = p;
        }
        for c in old_components {
            let h = remap[&c.handle];
            let mut value = c.fields.value();
            remap_references(&mut value, &remap);
            self.components[(h & 0xffffffff) as usize]
                .as_mut()
                .unwrap()
                .fields = Fields::parse(&c.kind, value)?;
        }
        Ok(json!({"object":self.object_snapshot(remap[&root])?,"remap":remap}))
    }
}
fn remap_references(v: &mut Value, remap: &BTreeMap<u64, u64>) {
    match v {
        Value::Object(map) => {
            if let Some(id) = map.get("$ref").and_then(Value::as_u64)
                && let Some(new) = remap.get(&id)
            {
                map.insert("$ref".into(), json!(new));
            }
            for v in map.values_mut() {
                remap_references(v, remap);
            }
        }
        Value::Array(a) => {
            for v in a {
                remap_references(v, remap);
            }
        }
        _ => {}
    }
}
