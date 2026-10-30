//! Opt-in record of what changed in a world between two drains.

use std::collections::BTreeMap;

use crate::{ComponentDesc, ObjectHandle, World};

/// The kind of a component, without its payload.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ComponentKind {
    Transform,
    Sprite,
    Tilemap,
    Camera,
    Light,
    GlobalLight2d,
    ShadowCaster2d,
    UniversalAdditionalCameraData,
    AudioListener,
    GuiLayer,
    FlareLayer,
    #[cfg(feature = "ui")]
    RectTransform,
    #[cfg(feature = "ui")]
    Canvas,
    #[cfg(feature = "ui")]
    CanvasGroup,
    #[cfg(feature = "ui")]
    CanvasRenderer,
    #[cfg(feature = "ui")]
    Text,
    Body2d,
    Collider2d,
    MeshInstance,
    Renderer,
    Script,
}

impl ComponentDesc {
    pub fn kind(&self) -> ComponentKind {
        match self {
            Self::Transform(_) => ComponentKind::Transform,
            Self::Sprite(_) => ComponentKind::Sprite,
            Self::Tilemap(_) => ComponentKind::Tilemap,
            Self::Camera(_) => ComponentKind::Camera,
            Self::Light(_) => ComponentKind::Light,
            Self::GlobalLight2d(_) => ComponentKind::GlobalLight2d,
            Self::ShadowCaster2d => ComponentKind::ShadowCaster2d,
            Self::UniversalAdditionalCameraData(_) => ComponentKind::UniversalAdditionalCameraData,
            Self::AudioListener { .. } => ComponentKind::AudioListener,
            Self::GUILayer { .. } => ComponentKind::GuiLayer,
            Self::FlareLayer { .. } => ComponentKind::FlareLayer,
            #[cfg(feature = "ui")]
            Self::RectTransform(_) => ComponentKind::RectTransform,
            #[cfg(feature = "ui")]
            Self::Canvas(_) => ComponentKind::Canvas,
            #[cfg(feature = "ui")]
            Self::CanvasGroup(_) => ComponentKind::CanvasGroup,
            #[cfg(feature = "ui")]
            Self::CanvasRenderer(_) => ComponentKind::CanvasRenderer,
            #[cfg(feature = "ui")]
            Self::Text(_) => ComponentKind::Text,
            Self::Body2d(_) => ComponentKind::Body2d,
            Self::Collider2d(_) => ComponentKind::Collider2d,
            Self::MeshInstance(_) => ComponentKind::MeshInstance,
            Self::Renderer { .. } => ComponentKind::Renderer,
            Self::Script { .. } => ComponentKind::Script,
        }
    }
}

/// How a component of one kind changed on an object during an interval.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ComponentChange {
    Added,
    Removed,
    Changed,
}

/// One component-kind change on one object.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ComponentChangeEntry {
    pub object: ObjectHandle,
    pub kind: ComponentKind,
    pub change: ComponentChange,
}

/// Coalesced changes since the previous drain. Every list is in object handle
/// order (and kind order within `components`).
///
/// An object created and destroyed within one interval appears nowhere. A destroyed object
/// appears only in `destroyed`. A created object reports `created` plus any later transform,
/// parent, property or component changes. The `Transform` component is part of every object
/// and is reported through `transforms`, never through `components`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WorldChanges {
    pub created: Vec<ObjectHandle>,
    pub destroyed: Vec<ObjectHandle>,
    /// Local transform changed.
    pub transforms: Vec<ObjectHandle>,
    /// Parent changed.
    pub parents: Vec<ObjectHandle>,
    /// Name, tag, layer or active flag changed.
    pub properties: Vec<ObjectHandle>,
    pub components: Vec<ComponentChangeEntry>,
}

impl WorldChanges {
    pub fn is_empty(&self) -> bool {
        self.created.is_empty()
            && self.destroyed.is_empty()
            && self.transforms.is_empty()
            && self.parents.is_empty()
            && self.properties.is_empty()
            && self.components.is_empty()
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum Change {
    Created(ObjectHandle),
    Destroyed(ObjectHandle),
    Transform(ObjectHandle),
    Parent(ObjectHandle),
    Properties(ObjectHandle),
    Component(ObjectHandle, ComponentKind, ComponentChange),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Presence {
    #[default]
    Existing,
    Created,
    Destroyed,
}

#[derive(Clone, Debug, Default)]
struct Pending {
    presence: Presence,
    transform: bool,
    parent: bool,
    properties: bool,
    components: BTreeMap<ComponentKind, ComponentChange>,
}

impl Pending {
    fn is_empty(&self) -> bool {
        self.presence == Presence::Existing
            && !self.transform
            && !self.parent
            && !self.properties
            && self.components.is_empty()
    }
}

fn merge(old: Option<ComponentChange>, new: ComponentChange) -> Option<ComponentChange> {
    use ComponentChange::{Added, Changed, Removed};
    match (old, new) {
        (None, new) => Some(new),
        (Some(Added), Removed) => None,
        (Some(Added), _) => Some(Added),
        (Some(Changed | Removed), Added) | (Some(Changed), Changed) => Some(Changed),
        (Some(Changed | Removed), _) => Some(Removed),
    }
}

#[derive(Clone, Default)]
pub(crate) struct Journal {
    enabled: bool,
    pending: BTreeMap<ObjectHandle, Pending>,
    /// While set, changes wait here so a failed batch can discard them.
    staged: Option<Vec<Change>>,
}

impl Journal {
    pub(crate) fn enabled(&self) -> bool {
        self.enabled
    }

    pub(crate) fn enable(&mut self) {
        self.enabled = true;
    }

    pub(crate) fn record(&mut self, change: Change) {
        if !self.enabled {
            return;
        }
        match &mut self.staged {
            Some(staged) => staged.push(change),
            None => self.apply(change),
        }
    }

    pub(crate) fn begin_stage(&mut self) {
        if self.enabled {
            self.staged = Some(Vec::new());
        }
    }

    pub(crate) fn commit_stage(&mut self) {
        for change in self.staged.take().unwrap_or_default() {
            self.apply(change);
        }
    }

    pub(crate) fn discard_stage(&mut self) {
        self.staged = None;
    }

    fn apply(&mut self, change: Change) {
        match change {
            Change::Created(object) => {
                self.pending.entry(object).or_default().presence = Presence::Created;
            }
            Change::Destroyed(object) => self.destroyed(object),
            Change::Transform(object) => self.touch(object, |p| p.transform = true),
            Change::Parent(object) => self.touch(object, |p| p.parent = true),
            Change::Properties(object) => self.touch(object, |p| p.properties = true),
            Change::Component(object, kind, change) => self.touch(object, |p| {
                match merge(p.components.get(&kind).copied(), change) {
                    Some(merged) => p.components.insert(kind, merged),
                    None => p.components.remove(&kind),
                };
            }),
        }
    }

    fn destroyed(&mut self, object: ObjectHandle) {
        if self
            .pending
            .get(&object)
            .is_some_and(|p| p.presence == Presence::Created)
        {
            self.pending.remove(&object);
        } else {
            self.pending.insert(
                object,
                Pending {
                    presence: Presence::Destroyed,
                    ..Pending::default()
                },
            );
        }
    }

    fn touch(&mut self, object: ObjectHandle, edit: impl FnOnce(&mut Pending)) {
        let pending = self.pending.entry(object).or_default();
        if pending.presence == Presence::Destroyed {
            return;
        }
        edit(pending);
        if pending.is_empty() {
            self.pending.remove(&object);
        }
    }

    fn drain(&mut self) -> WorldChanges {
        let mut changes = WorldChanges::default();
        for (object, pending) in std::mem::take(&mut self.pending) {
            match pending.presence {
                Presence::Created => changes.created.push(object),
                Presence::Destroyed => changes.destroyed.push(object),
                Presence::Existing => {}
            }
            if pending.transform {
                changes.transforms.push(object);
            }
            if pending.parent {
                changes.parents.push(object);
            }
            if pending.properties {
                changes.properties.push(object);
            }
            changes
                .components
                .extend(pending.components.into_iter().map(|(kind, change)| {
                    ComponentChangeEntry {
                        object,
                        kind,
                        change,
                    }
                }));
        }
        changes
    }
}

impl World {
    /// Starts recording changes. Tracking is off by default and costs one flag test per
    /// mutation while off. Changes made before this call are never reported.
    pub fn track_changes(&mut self) {
        self.journal.enable();
    }

    pub fn is_tracking_changes(&self) -> bool {
        self.journal.enabled()
    }

    /// Returns the coalesced changes since the previous drain and starts a new interval.
    /// Returns an empty set while tracking is off.
    pub fn drain_changes(&mut self) -> WorldChanges {
        self.journal.drain()
    }

    pub(crate) fn record_change(&mut self, change: Change) {
        self.journal.record(change);
    }
}
