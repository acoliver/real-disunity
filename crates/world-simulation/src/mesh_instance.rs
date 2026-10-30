//! 3D mesh instances: what an object draws, as opaque ids a host resolves to render handles.

use crate::{ComponentDesc, ComponentError, ObjectHandle, World, WorldError};

/// Opaque identity of a mesh. The host that renders the world decides what it names.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MeshId(pub u64);

/// Opaque identity of a material. The host that renders the world decides what it names.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MaterialId(pub u64);

/// Most materials one instance can name.
pub const MAX_INSTANCE_MATERIALS: usize = 64;
/// Most lower-detail meshes one instance can name after its primary mesh.
pub const MAX_INSTANCE_LODS: usize = 8;
/// Largest linear tint channel, matching the brightest multiplier a scene accepts.
pub const MAX_TINT_CHANNEL: f32 = 4.0;

/// A drawn 3D mesh at the object's world transform.
///
/// The world stores only ids. Mapping them to meshes, LOD tables and materials is the host's
/// job, so this crate loads no assets.
#[derive(Clone, Debug, PartialEq)]
pub struct MeshInstance {
    /// Highest-detail mesh.
    pub mesh: MeshId,
    /// Lower-detail meshes in order of decreasing detail. Empty when the mesh has no LODs.
    pub lods: Vec<MeshId>,
    /// One material per submesh slot, at least one.
    pub materials: Vec<MaterialId>,
    /// Linear RGB multiplier, each channel in `0..=MAX_TINT_CHANNEL`.
    pub tint: [f32; 3],
}

impl MeshInstance {
    /// An instance of `mesh` drawn with `material`, without LODs and with a white tint.
    pub fn new(mesh: MeshId, material: MaterialId) -> Self {
        Self {
            mesh,
            lods: Vec::new(),
            materials: vec![material],
            tint: [1.0; 3],
        }
    }

    pub(crate) fn validate(&self) -> Result<(), WorldError> {
        let tint_ok = self
            .tint
            .iter()
            .all(|channel| channel.is_finite() && (0.0..=MAX_TINT_CHANNEL).contains(channel));
        if self.materials.is_empty()
            || self.materials.len() > MAX_INSTANCE_MATERIALS
            || self.lods.len() > MAX_INSTANCE_LODS
            || !tint_ok
        {
            return Err(WorldError::InvalidComponent(
                ComponentError::InvalidMeshInstance,
            ));
        }
        Ok(())
    }
}

impl World {
    /// The object's live mesh instance, or `None` when it has none.
    pub fn mesh_instance(&self, object: ObjectHandle) -> Result<Option<&MeshInstance>, WorldError> {
        let Some(handle) = self.live_object(object)?.mesh_instance else {
            return Ok(None);
        };
        Ok(match &self.component(handle)?.desc {
            ComponentDesc::MeshInstance(instance) => Some(instance),
            _ => None,
        })
    }

    /// Every live object that has a mesh instance, in handle order.
    pub fn mesh_instance_objects(&self) -> impl Iterator<Item = ObjectHandle> + '_ {
        self.objects
            .iter(self.identity)
            .filter(|(_, object)| object.mesh_instance.is_some())
            .map(|(id, _)| ObjectHandle(id))
    }
}
