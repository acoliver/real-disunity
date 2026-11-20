//! The mesh renderers a frame shows, for a host that draws 3D objects next to the sprites.
use std::collections::HashMap;

use super::extraction::{selected_camera, view_basis};
use crate::graphics::{
    BASE_COLOR, MAIN_COLOR, MAIN_TEXTURE, MaterialId, ShaderProperties, TextureRef,
};
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
    /// What each material slot draws with, one per material the renderer holds. A slot with a
    /// live material draws with that material's current colour and main texture, as the
    /// renderer's property block overrides them; any other slot draws with the renderer's
    /// `drawn` record.
    pub slots: Vec<DrawnMaterial>,
    /// The layer of the renderer's object, which a camera's culling mask tests.
    pub layer: u32,
    /// The world material of each slot, one per slot in the order of `slots`. A slot has none
    /// when it never had one or its material was destroyed.
    pub materials: Vec<Option<MaterialId>>,
    /// The render queue that replaces the materials' queues for this renderer, when it has one.
    pub render_queue: Option<i32>,
    /// The shader values that override the materials' for this renderer, as a property block
    /// does.
    pub property_block: Option<ShaderProperties>,
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
        let layer = world.object(component.object)?.layer;
        if !world.active_in_hierarchy(component.object)?
            || camera.is_some_and(|camera| !camera.sees_layer(layer))
        {
            continue;
        }
        let materials: Vec<Option<MaterialId>> = (0..renderer.material_count as usize)
            .map(|slot| live_material(world, renderer.slot_materials.get(slot).copied().flatten()))
            .collect();
        let slots = materials
            .iter()
            .enumerate()
            .map(|(slot, material)| {
                let fallback = renderer.drawn.get(slot).copied().unwrap_or_default();
                drawn_slot(world, *material, renderer.property_block.as_ref(), fallback)
            })
            .collect();
        objects.push(MeshObject {
            object: component.object,
            local_to_world: (basis * world.presented_matrix(component.object)?).to_cols_array(),
            mesh,
            slots,
            layer: u32::from(layer),
            materials,
            render_queue: renderer.render_queue,
            property_block: renderer.property_block.clone(),
        });
    }
    Ok(objects)
}

fn live_material(world: &World, material: Option<MaterialId>) -> Option<MaterialId> {
    material.filter(|id| world.material(*id).is_ok())
}

/// What a slot draws with now. A slot with a live world material takes the colour and main
/// texture the material and the renderer's property block hold, the way the managed
/// `Renderer.DrawnOf` reads them when the shared materials are set (csharp/Disunity/RendererDrawn.cs):
/// `_Color` if there is one, else `_BaseColor`, else white, with negative channels as 0; and the
/// main texture's asset identity, 0 for a texture that is not an asset. A slot with no live
/// material draws `fallback`, the record the managed side sent. The world never holds a colour
/// that is not finite, so none reaches the slot.
fn drawn_slot(
    world: &World,
    material: Option<MaterialId>,
    block: Option<&ShaderProperties>,
    fallback: DrawnMaterial,
) -> DrawnMaterial {
    let Some(material) = material.and_then(|id| world.material(id).ok()) else {
        return fallback;
    };
    let sets = [block, Some(&material.properties)];
    let color = sets
        .iter()
        .flatten()
        .find_map(|set| set.vector(MAIN_COLOR))
        .or_else(|| sets.iter().flatten().find_map(|set| set.vector(BASE_COLOR)))
        .unwrap_or([1.0; 4])
        .map(|channel| channel.max(0.0));
    let texture = match sets
        .iter()
        .flatten()
        .find_map(|set| set.texture(MAIN_TEXTURE))
        .map(|slot| slot.texture)
    {
        Some(TextureRef::Asset(identity)) => identity,
        _ => 0,
    };
    DrawnMaterial { color, texture }
}
