//! The mesh renderers a frame shows, for a host that draws 3D objects next to the sprites.
use std::collections::HashMap;

use super::extraction::{camera_admits, selected_camera, view_basis};
use crate::{ComponentDesc, DrawnMaterial, ObjectHandle, RendererKind, World, WorldError};

/// One mesh renderer a frame shows.
#[derive(Clone, Debug, PartialEq)]
pub struct MeshObject {
    pub object: ObjectHandle,
    /// Column-major matrix from the mesh's own coordinates to the frame's view basis, the basis
    /// the camera pose is in.
    pub local_to_world: [f32; 16],
    /// The renderer identity of the mesh contents, the one the host reads them by.
    pub mesh: u64,
    /// What each material slot draws with, one per material the renderer holds.
    pub slots: Vec<DrawnMaterial>,
}

/// Lists, in component order, every enabled mesh renderer on an active object whose mesh filter
/// names a mesh the host has contents for and whose layer the presented camera's culling mask
/// admits (every layer when no camera is presented). A renderer with `force_rendering_off` is
/// left out, and so is every renderer that is not a plain mesh renderer: skinned and particle
/// renderers draw from data a mesh filter does not hold.
pub fn extract_mesh_objects(world: &World) -> Result<Vec<MeshObject>, WorldError> {
    let camera = selected_camera(world, world.published_components())?.map(|(_, camera)| camera);
    let mut meshes: HashMap<ObjectHandle, u64> = HashMap::new();
    for component in world.published_components() {
        if let ComponentDesc::MeshFilter(filter) = &component.desc
            && filter.drawn_mesh != 0
        {
            meshes.insert(component.object, filter.drawn_mesh);
        }
    }
    let basis = view_basis();
    let mut objects = Vec::new();
    for component in world.published_components() {
        let ComponentDesc::MeshRenderer(renderer) = &component.desc else {
            continue;
        };
        let shown = renderer.kind == RendererKind::Mesh
            && renderer.switches.enabled
            && !renderer.switches.force_rendering_off;
        let Some(mesh) = meshes.get(&component.object).copied().filter(|_| shown) else {
            continue;
        };
        if !world.active_in_hierarchy(component.object)?
            || !camera_admits(world, camera, component.object)?
        {
            continue;
        }
        let slots = (0..renderer.material_count as usize)
            .map(|slot| renderer.drawn.get(slot).copied().unwrap_or_default())
            .collect();
        objects.push(MeshObject {
            object: component.object,
            local_to_world: (basis * world.presented_matrix(component.object)?).to_cols_array(),
            mesh,
            slots,
        });
    }
    Ok(objects)
}
