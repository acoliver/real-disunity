use crate::{ComponentHandle, ObjectHandle, World, WorldError};
use std::collections::BTreeMap;

/// Bounded serialized script data with distinct native reference types.
#[derive(Clone, Debug, PartialEq)]
pub enum FieldValue {
    Null,
    Bool(bool),
    Integer(i64),
    Number(f64),
    Text(String),
    Object(ObjectHandle),
    Component(ComponentHandle),
    Array(Vec<FieldValue>),
    Record(Vec<(String, FieldValue)>),
}

impl FieldValue {
    fn validate(&self, world: u64, depth: usize, remaining: &mut usize) -> Result<(), WorldError> {
        if depth > 32 || *remaining == 0 {
            return Err(WorldError::InvalidFields);
        }
        *remaining -= 1;
        match self {
            Self::Object(handle) if handle.0.world != world => Err(WorldError::WrongWorld),
            Self::Component(handle) if handle.0.world != world => Err(WorldError::WrongWorld),
            Self::Number(value) if !value.is_finite() => Err(WorldError::InvalidFields),
            Self::Text(value) if value.len() > 65536 => Err(WorldError::InvalidFields),
            Self::Array(values) => {
                for value in values {
                    value.validate(world, depth + 1, remaining)?;
                }
                Ok(())
            }
            Self::Record(fields) => validate_fields(fields, world, depth + 1, remaining),
            _ => Ok(()),
        }
    }

    pub(crate) fn remap(
        &mut self,
        objects: &BTreeMap<ObjectHandle, ObjectHandle>,
        components: &BTreeMap<ComponentHandle, ComponentHandle>,
    ) {
        match self {
            Self::Object(handle) => {
                if let Some(new) = objects.get(handle) {
                    *handle = *new;
                }
            }
            Self::Component(handle) => {
                if let Some(new) = components.get(handle) {
                    *handle = *new;
                }
            }
            Self::Array(values) => {
                for value in values {
                    value.remap(objects, components);
                }
            }
            Self::Record(fields) => {
                for (_, value) in fields {
                    value.remap(objects, components);
                }
            }
            _ => {}
        }
    }
}

fn validate_fields(
    fields: &[(String, FieldValue)],
    world: u64,
    depth: usize,
    remaining: &mut usize,
) -> Result<(), WorldError> {
    let mut names = std::collections::BTreeSet::new();
    for (name, value) in fields {
        if name.is_empty() || name.len() > 1024 || !names.insert(name) {
            return Err(WorldError::InvalidFields);
        }
        value.validate(world, depth, remaining)?;
    }
    Ok(())
}

impl World {
    pub fn set_script_fields(
        &mut self,
        script: ComponentHandle,
        fields: Vec<(String, FieldValue)>,
    ) -> Result<(), WorldError> {
        self.component(script)?;
        validate_fields(&fields, self.identity, 0, &mut 4096)?;
        let state = self
            .lifecycle
            .scripts
            .get_mut(&script)
            .ok_or(WorldError::NotScript)?;
        state.fields = fields;
        Ok(())
    }

    pub fn script_fields(
        &self,
        script: ComponentHandle,
    ) -> Result<&[(String, FieldValue)], WorldError> {
        self.component(script)?;
        Ok(&self
            .lifecycle
            .scripts
            .get(&script)
            .ok_or(WorldError::NotScript)?
            .fields)
    }
}
