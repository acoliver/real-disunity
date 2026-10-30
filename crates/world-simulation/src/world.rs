use crate::handles::{Arena, next_world};
use crate::journal::Change;
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

#[derive(Clone)]
pub(crate) struct Object {
    name: String,
    tag: String,
    layer: u8,
    pub(crate) active: bool,
    pub(crate) parent: Option<ObjectHandle>,
    /// Live objects whose `parent` is this object, kept equal to a scan of all objects.
    /// Handle order is slot order within a world, which is the order `children` reports.
    pub(crate) children: std::collections::BTreeSet<ObjectHandle>,
    /// Every stored component owned by this object, including the transform.
    pub(crate) components: Vec<ComponentHandle>,
    transform: ComponentHandle,
    pub(crate) renderer: Option<ComponentHandle>,
    pub(crate) mesh_instance: Option<ComponentHandle>,
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
    pub(crate) assets: crate::assets::Assets,
    pub(crate) assets_limit: u32,
    pub(crate) clock: crate::Clock,
    pub(crate) input: crate::input::Input,
    pub(crate) coroutines: crate::coroutines::Coroutines,
    pub(crate) scheduler: crate::timeline::Scheduler,
    pub(crate) physics: crate::physics2d::Physics,
    pub(crate) identity: u64,
    pub(crate) objects: Arena<Object>,
    pub(crate) lifetime: crate::lifetime::Lifetime,
    pub(crate) lifecycle: crate::lifecycle::Lifecycle,
    pub(crate) components: Arena<ComponentSnapshot>,
    pub(crate) journal: crate::journal::Journal,
    pub(crate) spatial: Option<crate::spatial::SpatialIndex>,
    pub(crate) staging: Option<crate::edits::Staging>,
}

impl World {
    pub fn new(settings: WorldSettings) -> Result<Self, WorldError> {
        Ok(Self {
            assets: crate::assets::Assets::default(),
            assets_limit: settings.max_objects,
            clock: crate::Clock::default(),
            input: crate::input::Input::default(),
            coroutines: crate::coroutines::Coroutines::default(),
            scheduler: crate::timeline::Scheduler::default(),
            physics: crate::physics2d::Physics::default(),
            identity: next_world()?,
            lifetime: crate::lifetime::Lifetime::default(),
            lifecycle: crate::lifecycle::Lifecycle::default(),
            objects: Arena::new(settings.max_objects, LimitKind::Objects),
            components: Arena::new(settings.max_components, LimitKind::Components),
            journal: crate::journal::Journal::default(),
            spatial: None,
            staging: None,
        })
    }

    pub fn create_object(&mut self, desc: ObjectDesc) -> Result<ObjectHandle, WorldError> {
        self.ensure_allocation_allowed()?;
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
                parent: None,
                children: std::collections::BTreeSet::new(),
                components: vec![transform],
                transform,
                renderer: None,
                mesh_instance: None,
            },
        );
        self.record_change(Change::Created(object));
        Ok(object)
    }

    pub(crate) fn live_object(&self, handle: ObjectHandle) -> Result<&Object, WorldError> {
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

    /// Moves the object to `layer`, one of Unity's 32 layers.
    pub fn set_layer(&mut self, handle: ObjectHandle, layer: u8) -> Result<(), WorldError> {
        self.live_object(handle)?;
        if layer > 31 {
            return Err(WorldError::InvalidObject);
        }
        self.objects
            .get_mut(handle.0)
            .expect("validated object")
            .layer = layer;
        self.record_change(Change::Properties(handle));
        Ok(())
    }

    pub fn set_object_text(
        &mut self,
        handle: ObjectHandle,
        tag: bool,
        text: String,
    ) -> Result<(), WorldError> {
        self.live_object(handle)?;
        if text.len() > 1024 {
            return Err(WorldError::InvalidObject);
        }
        let object = self.objects.get_mut(handle.0).expect("validated object");
        if tag {
            object.tag = text;
        } else {
            object.name = text;
        }
        self.record_change(Change::Properties(handle));
        Ok(())
    }

    pub fn component(&self, handle: ComponentHandle) -> Result<&ComponentSnapshot, WorldError> {
        if handle.0.world != self.identity {
            return Err(WorldError::WrongWorld);
        }
        self.components
            .get(handle.0)
            .ok_or(WorldError::StaleComponent(handle))
    }

    fn duplicate_component(
        &self,
        object: ObjectHandle,
        desc: &ComponentDesc,
    ) -> Result<bool, WorldError> {
        let owner = self.live_object(object)?;
        Ok(match desc {
            ComponentDesc::Transform(_) => true,
            ComponentDesc::Sprite(_)
            | ComponentDesc::Tilemap(_)
            | ComponentDesc::Camera(_)
            | ComponentDesc::Light(_)
            | ComponentDesc::GlobalLight2d(_)
            | ComponentDesc::UniversalAdditionalCameraData(_)
            | ComponentDesc::AudioListener { .. }
            | ComponentDesc::GUILayer { .. }
            | ComponentDesc::FlareLayer { .. } => self.owns_unretired(owner, desc),
            #[cfg(feature = "ui")]
            ComponentDesc::RectTransform(_)
            | ComponentDesc::Canvas(_)
            | ComponentDesc::CanvasGroup(_)
            | ComponentDesc::CanvasRenderer(_)
            | ComponentDesc::Text(_) => self.owns_unretired(owner, desc),
            ComponentDesc::Renderer { .. } => owner.renderer.is_some(),
            ComponentDesc::MeshInstance(_) => owner.mesh_instance.is_some(),
            ComponentDesc::ShadowCaster2d
            | ComponentDesc::Script { .. }
            | ComponentDesc::Collider2d(_) => false,
            ComponentDesc::Body2d(_) => self.owns_unretired(owner, desc),
        })
    }

    /// Whether `owner` has a component of `desc`'s kind that is not already retiring. Walks the
    /// owner's own components, so the cost does not depend on the size of the world.
    fn owns_unretired(&self, owner: &Object, desc: &ComponentDesc) -> bool {
        owner.components.iter().any(|handle| {
            !self.lifetime.components.contains(handle)
                && self.components.get(handle.0).is_some_and(|c| {
                    std::mem::discriminant(&c.desc) == std::mem::discriminant(desc)
                })
        })
    }

    pub fn add_component(
        &mut self,
        object: ObjectHandle,
        mut desc: ComponentDesc,
    ) -> Result<ComponentHandle, WorldError> {
        self.ensure_allocation_allowed()?;
        self.live_object(object)?;
        desc.validate()?;
        self.validate_physics_material(&desc)?;
        desc.register_presentation()?;
        let duplicate = self.duplicate_component(object, &desc)?;
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
        if matches!(desc, ComponentDesc::MeshInstance(_)) {
            self.objects
                .get_mut(object.0)
                .expect("validated object")
                .mesh_instance = Some(handle);
        }
        let script = matches!(desc, ComponentDesc::Script { .. });
        self.objects
            .get_mut(object.0)
            .expect("validated object")
            .components
            .push(handle);
        self.components.insert(
            handle.0,
            ComponentSnapshot {
                handle,
                object,
                desc,
            },
        );
        self.register_component_lifetime(handle, object, script);
        self.note_component_added(handle);
        Ok(handle)
    }

    fn register_component_lifetime(
        &mut self,
        handle: ComponentHandle,
        object: ObjectHandle,
        script: bool,
    ) {
        if script {
            self.lifecycle
                .scripts
                .insert(handle, crate::lifecycle::ScriptState::default());
        }
        if self.lifetime.objects.contains(&object) {
            self.lifetime.components.insert(handle);
        }
    }

    pub fn set_component(
        &mut self,
        handle: ComponentHandle,
        mut desc: ComponentDesc,
    ) -> Result<(), WorldError> {
        let old = self.component(handle)?;
        desc.validate()?;
        self.validate_physics_material(&desc)?;
        desc.register_presentation()?;
        if std::mem::discriminant(&old.desc) != std::mem::discriminant(&desc) {
            return Err(WorldError::InvalidComponent(ComponentError::KindMismatch));
        }
        let slot = self
            .components
            .get_mut(handle.0)
            .expect("validated component");
        let previous = std::mem::replace(&mut slot.desc, desc);
        self.note_component_replaced(handle, previous);
        Ok(())
    }

    /// Removes and retires only the requested component.
    pub fn remove_component(&mut self, handle: ComponentHandle) -> Result<(), WorldError> {
        self.destroy_component(handle)?;
        self.retire_component(handle);
        Ok(())
    }

    pub fn renderer(&self, object: ObjectHandle) -> Result<Option<ComponentHandle>, WorldError> {
        Ok(self.live_object(object)?.renderer)
    }

    /// Destroys and retires only the requested subtree.
    pub fn destroy_object(&mut self, handle: ObjectHandle) -> Result<(), WorldError> {
        self.destroy(handle, crate::DestroyWhen::EndOfFrame)?;
        self.retire_subtree(handle);
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

    pub fn snapshot(&self) -> WorldSnapshot {
        WorldSnapshot {
            objects: self
                .objects
                .iter(self.identity)
                .filter(|(id, _)| !self.is_unpublished(ObjectHandle(*id)))
                .map(|(id, _)| self.object(ObjectHandle(id)).expect("live arena entry"))
                .collect(),
            components: self
                .components
                .iter(self.identity)
                .filter(|(_, value)| !self.is_unpublished(value.object))
                .map(|(_, value)| value.clone())
                .collect(),
        }
    }
}
