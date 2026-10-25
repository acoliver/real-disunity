use glam::Mat4;

use crate::handles::{Arena, next_world};
use crate::{
    ComponentDesc, ComponentError, ComponentHandle, LimitKind, LocalTransform, ObjectHandle,
    WorldError,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorldSettings {
    pub max_objects: u32,
    pub max_components: u32,
}

impl Default for WorldSettings {
    fn default() -> Self {
        Self {
            max_objects: 1 << 20,
            max_components: 1 << 22,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ObjectDesc {
    pub name: String,
    pub tag: String,
    pub layer: u8,
    pub active: bool,
    pub local_transform: LocalTransform,
}

impl Default for ObjectDesc {
    fn default() -> Self {
        Self {
            name: "GameObject".into(),
            tag: "Untagged".into(),
            layer: 0,
            active: true,
            local_transform: LocalTransform::default(),
        }
    }
}

struct Object {
    name: String,
    tag: String,
    layer: u8,
    active: bool,
    transform: ComponentHandle,
    renderer: Option<ComponentHandle>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ObjectSnapshot {
    pub handle: ObjectHandle,
    pub name: String,
    pub tag: String,
    pub layer: u8,
    pub active: bool,
    pub transform: ComponentHandle,
    pub local_transform: LocalTransform,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ComponentSnapshot {
    pub handle: ComponentHandle,
    pub object: ObjectHandle,
    pub desc: ComponentDesc,
}

/// Detached read-only observations. Editing these values cannot change a world.
#[derive(Clone, Debug, PartialEq)]
pub struct WorldSnapshot {
    pub objects: Vec<ObjectSnapshot>,
    pub components: Vec<ComponentSnapshot>,
}

/// The sole owner of live object and component state for a simulation session.
pub struct World {
    pub(crate) identity: u64,
    objects: Arena<Object>,
    pub(crate) components: Arena<ComponentSnapshot>,
}

impl World {
    pub fn new(settings: WorldSettings) -> Result<Self, WorldError> {
        Ok(Self {
            identity: next_world()?,
            objects: Arena::new(settings.max_objects, LimitKind::Objects),
            components: Arena::new(settings.max_components, LimitKind::Components),
        })
    }

    pub fn create_object(&mut self, desc: ObjectDesc) -> Result<ObjectHandle, WorldError> {
        if desc.name.len() > 1024 || desc.tag.len() > 1024 || desc.layer > 31 {
            return Err(WorldError::InvalidObject);
        }
        desc.local_transform.validate()?;
        let object = ObjectHandle(self.objects.vacancy(self.identity)?);
        let transform = ComponentHandle(self.components.vacancy(self.identity)?);
        self.components.insert(
            transform.0,
            ComponentSnapshot {
                handle: transform,
                object,
                desc: ComponentDesc::Transform(desc.local_transform),
            },
        );
        self.objects.insert(
            object.0,
            Object {
                name: desc.name,
                tag: desc.tag,
                layer: desc.layer,
                active: desc.active,
                transform,
                renderer: None,
            },
        );
        Ok(object)
    }

    fn live_object(&self, handle: ObjectHandle) -> Result<&Object, WorldError> {
        if handle.0.world != self.identity {
            return Err(WorldError::WrongWorld);
        }
        self.objects
            .get(handle.0)
            .ok_or(WorldError::StaleObject(handle))
    }

    pub fn object(&self, handle: ObjectHandle) -> Result<ObjectSnapshot, WorldError> {
        let object = self.live_object(handle)?;
        Ok(ObjectSnapshot {
            handle,
            name: object.name.clone(),
            tag: object.tag.clone(),
            layer: object.layer,
            active: object.active,
            transform: object.transform,
            local_transform: self.local_transform(handle)?,
        })
    }

    pub fn component(&self, handle: ComponentHandle) -> Result<&ComponentSnapshot, WorldError> {
        if handle.0.world != self.identity {
            return Err(WorldError::WrongWorld);
        }
        self.components
            .get(handle.0)
            .ok_or(WorldError::StaleComponent(handle))
    }

    pub fn add_component(
        &mut self,
        object: ObjectHandle,
        desc: ComponentDesc,
    ) -> Result<ComponentHandle, WorldError> {
        let owner = self.live_object(object)?;
        desc.validate()?;
        let duplicate = match desc {
            ComponentDesc::Transform(_) => true,
            ComponentDesc::Renderer { .. } => owner.renderer.is_some(),
            ComponentDesc::Script { .. } => false,
        };
        if duplicate {
            return Err(WorldError::InvalidComponent(ComponentError::Duplicate));
        }
        let handle = ComponentHandle(self.components.vacancy(self.identity)?);
        if matches!(desc, ComponentDesc::Renderer { .. }) {
            self.objects
                .get_mut(object.0)
                .expect("validated object")
                .renderer = Some(handle);
        }
        self.components.insert(
            handle.0,
            ComponentSnapshot {
                handle,
                object,
                desc,
            },
        );
        Ok(handle)
    }

    pub fn set_component(
        &mut self,
        handle: ComponentHandle,
        desc: ComponentDesc,
    ) -> Result<(), WorldError> {
        let old = self.component(handle)?;
        desc.validate()?;
        if std::mem::discriminant(&old.desc) != std::mem::discriminant(&desc) {
            return Err(WorldError::InvalidComponent(ComponentError::KindMismatch));
        }
        self.components
            .get_mut(handle.0)
            .expect("validated component")
            .desc = desc;
        Ok(())
    }

    pub fn remove_component(&mut self, handle: ComponentHandle) -> Result<(), WorldError> {
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
            ComponentDesc::Script { .. } => {}
        }
        self.components.remove(handle.0);
        Ok(())
    }

    pub fn renderer(&self, object: ObjectHandle) -> Result<Option<ComponentHandle>, WorldError> {
        Ok(self.live_object(object)?.renderer)
    }

    pub fn destroy_object(&mut self, handle: ObjectHandle) -> Result<(), WorldError> {
        self.live_object(handle)?;
        let components: Vec<_> = self
            .components
            .iter(self.identity)
            .filter_map(|(id, c)| (c.object == handle).then_some(id))
            .collect();
        for id in components {
            self.components.remove(id);
        }
        self.objects.remove(handle.0);
        Ok(())
    }

    pub fn local_transform(&self, object: ObjectHandle) -> Result<LocalTransform, WorldError> {
        let handle = self.live_object(object)?.transform;
        let ComponentDesc::Transform(value) = self.component(handle)?.desc else {
            unreachable!("object transform kind is invariant")
        };
        Ok(value)
    }

    pub fn set_local_transform(
        &mut self,
        object: ObjectHandle,
        value: LocalTransform,
    ) -> Result<(), WorldError> {
        let handle = self.live_object(object)?.transform;
        self.set_component(handle, ComponentDesc::Transform(value))
    }

    pub fn world_matrix(&self, object: ObjectHandle) -> Result<Mat4, WorldError> {
        Ok(self.local_transform(object)?.matrix())
    }

    pub fn snapshot(&self) -> WorldSnapshot {
        WorldSnapshot {
            objects: self
                .objects
                .iter(self.identity)
                .map(|(id, _)| self.object(ObjectHandle(id)).expect("live arena entry"))
                .collect(),
            components: self
                .components
                .iter(self.identity)
                .map(|(_, value)| value.clone())
                .collect(),
        }
    }
}
