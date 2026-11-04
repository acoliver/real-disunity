//! Mesh renderer, skinned mesh renderer and mesh filter component settings. The rules live in
//! world-simulation; this file converts records.
use crate::{
    DisunityResult, callbacks::mutate, errors::AbiError, meshes::FIRST_PLAYER_MESH,
    physics::component,
};
use worldrunner_world_simulation::{
    ComponentDesc, ComponentHandle, DrawnMaterial, MAX_RENDERER_MATERIALS, MeshFilter,
    MeshRenderer, RendererKind, RendererSwitches,
};

/// A renderer's settings; booleans are 0 or 1. `kind` is 0 for a mesh renderer, 1 for a
/// skinned mesh renderer and 2 for a particle system renderer, and is fixed when the component is created. The mode fields hold the
/// numeric value of the script enum: shadow casting 0 to 3, motion vectors 0 to 2, light probe
/// usage 0, 1, 2 or 4, reflection probe usage 0 to 3. `material_count` is at most 64. A write
/// rejects an out-of-range value, a boolean above 1 or a different kind, and changes nothing.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct DisunityMeshRenderer {
    pub kind: u32,
    pub enabled: u32,
    pub force_rendering_off: u32,
    pub receive_shadows: u32,
    pub allow_occlusion_when_dynamic: u32,
    pub static_shadow_caster: u32,
    pub shadow_casting_mode: u32,
    pub motion_vector_mode: u32,
    pub light_probe_usage: u32,
    pub reflection_probe_usage: u32,
    pub rendering_layer_mask: u32,
    pub renderer_priority: i32,
    pub sorting_layer: i32,
    pub sorting_order: i32,
    pub material_count: u32,
}

impl DisunityMeshRenderer {
    fn from_native(renderer: MeshRenderer) -> Self {
        Self {
            kind: match renderer.kind {
                RendererKind::Mesh => 0,
                RendererKind::Skinned => 1,
                RendererKind::Particle => 2,
            },
            enabled: u32::from(renderer.switches.enabled),
            force_rendering_off: u32::from(renderer.switches.force_rendering_off),
            receive_shadows: u32::from(renderer.switches.receive_shadows),
            allow_occlusion_when_dynamic: u32::from(renderer.allow_occlusion_when_dynamic),
            static_shadow_caster: u32::from(renderer.static_shadow_caster),
            shadow_casting_mode: renderer.shadow_casting_mode,
            motion_vector_mode: renderer.motion_vector_mode,
            light_probe_usage: renderer.light_probe_usage,
            reflection_probe_usage: renderer.reflection_probe_usage,
            rendering_layer_mask: renderer.rendering_layer_mask,
            renderer_priority: renderer.renderer_priority,
            sorting_layer: renderer.sorting_layer,
            sorting_order: renderer.sorting_order,
            material_count: renderer.material_count,
        }
    }

    fn to_native(self, current: MeshRenderer) -> Result<MeshRenderer, AbiError> {
        let flags = [
            self.enabled,
            self.force_rendering_off,
            self.receive_shadows,
            self.allow_occlusion_when_dynamic,
            self.static_shadow_caster,
        ];
        if flags.iter().any(|flag| *flag > 1) {
            return Err("renderer boolean settings must be 0 or 1".into());
        }
        if self.kind != Self::from_native(current.clone()).kind {
            return Err("a renderer's kind cannot be changed".into());
        }
        Ok(MeshRenderer {
            kind: current.kind,
            switches: RendererSwitches {
                enabled: self.enabled == 1,
                force_rendering_off: self.force_rendering_off == 1,
                receive_shadows: self.receive_shadows == 1,
            },
            allow_occlusion_when_dynamic: self.allow_occlusion_when_dynamic == 1,
            static_shadow_caster: self.static_shadow_caster == 1,
            shadow_casting_mode: self.shadow_casting_mode,
            motion_vector_mode: self.motion_vector_mode,
            light_probe_usage: self.light_probe_usage,
            reflection_probe_usage: self.reflection_probe_usage,
            rendering_layer_mask: self.rendering_layer_mask,
            renderer_priority: self.renderer_priority,
            sorting_layer: self.sorting_layer,
            sorting_order: self.sorting_order,
            material_count: self.material_count,
            // Only `disunity_mesh_renderer_slots_write` sets what the slots draw with; the ones
            // past a lowered material count go with the materials.
            drawn: current
                .drawn
                .into_iter()
                .take(self.material_count as usize)
                .collect(),
            slot_materials: current
                .slot_materials
                .into_iter()
                .take(self.material_count as usize)
                .collect(),
            property_block: current.property_block,
            render_queue: current.render_queue,
        })
    }
}

pub(crate) fn renderer(
    session: &crate::store::Session,
    handle: u64,
) -> Result<(ComponentHandle, MeshRenderer), AbiError> {
    let id = component(session, handle)?;
    Ok((id, renderer_at(session, id)?))
}

/// The renderer a native component handle names.
pub(crate) fn renderer_at(
    session: &crate::store::Session,
    id: ComponentHandle,
) -> Result<MeshRenderer, AbiError> {
    match &session.native.component(id)?.desc {
        ComponentDesc::MeshRenderer(renderer) => Ok(renderer.clone()),
        _ => Err("component is not a MeshRenderer or SkinnedMeshRenderer".into()),
    }
}

/// Stores a changed renderer, validating it as a whole.
pub(crate) fn set_renderer(
    session: &mut crate::store::Session,
    id: ComponentHandle,
    renderer: MeshRenderer,
) -> Result<(), AbiError> {
    session
        .native
        .set_component(id, ComponentDesc::MeshRenderer(renderer))?;
    Ok(())
}

fn filter(
    session: &crate::store::Session,
    handle: u64,
) -> Result<(ComponentHandle, MeshFilter), AbiError> {
    let id = component(session, handle)?;
    match session.native.component(id)?.desc {
        ComponentDesc::MeshFilter(filter) => Ok((id, filter)),
        _ => Err("component is not a MeshFilter".into()),
    }
}

fn created(world: u64, object: u64, desc: ComponentDesc, output: *mut u64) -> DisunityResult {
    match crate::physics::create(world, object, desc) {
        Ok(id) => {
            unsafe { *output = id };
            DisunityResult::ok()
        }
        Err(e) => crate::abi::translate_error(Some(world), &e),
    }
}

/// Creates a renderer with the engine's default settings on a live object. `kind` is 0 for a
/// mesh renderer, 1 for a skinned mesh renderer and 2 for a particle system renderer.
/// # Safety
/// Output is writable for one component identity.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_mesh_renderer_create(
    world: u64,
    object: u64,
    kind: u32,
    output: *mut u64,
) -> DisunityResult {
    let kind = match kind {
        0 => RendererKind::Mesh,
        1 => RendererKind::Skinned,
        2 => RendererKind::Particle,
        _ => return DisunityResult::invalid_argument(),
    };
    if output.is_null() {
        return DisunityResult::invalid_argument();
    }
    created(
        world,
        object,
        ComponentDesc::MeshRenderer(MeshRenderer::of_kind(kind)),
        output,
    )
}

/// # Safety
/// Output is writable for one MeshRenderer record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_mesh_renderer_read(
    world: u64,
    handle: u64,
    output: *mut DisunityMeshRenderer,
) -> DisunityResult {
    if output.is_null() {
        return DisunityResult::invalid_argument();
    }
    mutate(world, |s| {
        let (_, value) = renderer(s, handle)?;
        unsafe { *output = DisunityMeshRenderer::from_native(value) };
        Ok(())
    })
}

/// Validates every setting before changing the renderer; a rejected write changes nothing.
/// # Safety
/// Value is readable for one MeshRenderer record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_mesh_renderer_write(
    world: u64,
    handle: u64,
    value: *const DisunityMeshRenderer,
) -> DisunityResult {
    if value.is_null() {
        return DisunityResult::invalid_argument();
    }
    let value = unsafe { *value };
    mutate(world, |s| {
        let (id, current) = renderer(s, handle)?;
        let desc = ComponentDesc::MeshRenderer(value.to_native(current)?);
        s.native.set_component(id, desc)?;
        Ok(())
    })
}

/// Creates a mesh filter with no mesh on a live object.
/// # Safety
/// Output is writable for one component identity.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_mesh_filter_create(
    world: u64,
    object: u64,
    output: *mut u64,
) -> DisunityResult {
    if output.is_null() {
        return DisunityResult::invalid_argument();
    }
    created(
        world,
        object,
        ComponentDesc::MeshFilter(MeshFilter::default()),
        output,
    )
}

/// Reads the managed instance id of the filter's mesh, 0 for none.
/// # Safety
/// Output is writable for one instance id.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_mesh_filter_read(
    world: u64,
    handle: u64,
    output: *mut u64,
) -> DisunityResult {
    if output.is_null() {
        return DisunityResult::invalid_argument();
    }
    mutate(world, |s| {
        let (_, value) = filter(s, handle)?;
        unsafe { *output = value.mesh };
        Ok(())
    })
}

/// Sets the managed instance id of the filter's mesh, 0 for none.
#[unsafe(no_mangle)]
pub extern "C" fn disunity_mesh_filter_write(world: u64, handle: u64, mesh: u64) -> DisunityResult {
    mutate(world, |s| {
        let (id, current) = filter(s, handle)?;
        let drawn_mesh = if mesh == current.mesh {
            current.drawn_mesh
        } else {
            0
        };
        s.native.set_component(
            id,
            ComponentDesc::MeshFilter(MeshFilter { mesh, drawn_mesh }),
        )?;
        Ok(())
    })
}

/// Sets the renderer identity the host knows the filter's mesh contents by; 0 for none. A
/// nonzero identity must be a player mesh identity. Naming another mesh with
/// `disunity_mesh_filter_write` resets it.
#[unsafe(no_mangle)]
pub extern "C" fn disunity_mesh_filter_drawn_write(
    world: u64,
    handle: u64,
    identity: u64,
) -> DisunityResult {
    mutate(world, |s| {
        if identity != 0 && identity < FIRST_PLAYER_MESH {
            return Err("a mesh filter draws a player mesh identity or none".into());
        }
        let (id, current) = filter(s, handle)?;
        s.native.set_component(
            id,
            ComponentDesc::MeshFilter(MeshFilter {
                drawn_mesh: identity,
                ..current
            }),
        )?;
        Ok(())
    })
}

/// Reads the renderer identity the host knows the filter's mesh contents by, 0 for none.
/// # Safety
/// Output is writable for one identity.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_mesh_filter_drawn_read(
    world: u64,
    handle: u64,
    output: *mut u64,
) -> DisunityResult {
    if output.is_null() {
        return DisunityResult::invalid_argument();
    }
    mutate(world, |s| {
        let (_, value) = filter(s, handle)?;
        unsafe { *output = value.drawn_mesh };
        Ok(())
    })
}

/// What the host draws one material slot of a renderer with: a colour multiplier as the
/// material holds it and the renderer identity of a texture, 0 for none.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DisunityDrawnMaterial {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
    pub texture: u64,
}

/// Replaces what the host draws each material slot with. `count` is at most the renderer's
/// `material_count`; slots past it draw white and untextured. Colour channels must be finite and
/// not negative, and a nonzero texture must be a player texture identity. A rejected call changes
/// nothing.
///
/// # Safety
/// `slots` is readable for `count` records, or null when `count` is zero.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_mesh_renderer_slots_write(
    world: u64,
    handle: u64,
    slots: *const DisunityDrawnMaterial,
    count: u32,
) -> DisunityResult {
    if count > MAX_RENDERER_MATERIALS || (count > 0 && slots.is_null()) {
        return DisunityResult::invalid_argument();
    }
    let given: &[DisunityDrawnMaterial] = if count == 0 {
        &[]
    } else {
        unsafe { std::slice::from_raw_parts(slots, count as usize) }
    };
    mutate(world, |s| {
        let (id, mut current) = renderer(s, handle)?;
        if given
            .iter()
            .any(|slot| slot.texture != 0 && slot.texture < crate::textures::FIRST_PLAYER_TEXTURE)
        {
            return Err("a renderer slot draws a player texture identity or none".into());
        }
        current.drawn = given
            .iter()
            .map(|slot| DrawnMaterial {
                color: [slot.r, slot.g, slot.b, slot.a],
                texture: slot.texture,
            })
            .collect();
        s.native
            .set_component(id, ComponentDesc::MeshRenderer(current))?;
        Ok(())
    })
}

/// Reads the slots the host draws with into `slots` (up to `capacity` records) and writes how
/// many the renderer holds to `count`.
///
/// # Safety
/// `slots` is writable for `capacity` records, or null when `capacity` is zero; `count` is
/// writable for one value.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_mesh_renderer_slots_read(
    world: u64,
    handle: u64,
    slots: *mut DisunityDrawnMaterial,
    capacity: u32,
    count: *mut u32,
) -> DisunityResult {
    if count.is_null() || (capacity > 0 && slots.is_null()) {
        return DisunityResult::invalid_argument();
    }
    mutate(world, |s| {
        let (_, current) = renderer(s, handle)?;
        let held = u32::try_from(current.drawn.len()).map_err(|_| "slot count overflow")?;
        unsafe { *count = held };
        if capacity == 0 {
            return Ok(());
        }
        if capacity < held {
            return Err("renderer slot capacity is too small".into());
        }
        for (index, slot) in current.drawn.iter().enumerate() {
            let [r, g, b, a] = slot.color;
            let texture = slot.texture;
            unsafe {
                slots.add(index).write(DisunityDrawnMaterial {
                    r,
                    g,
                    b,
                    a,
                    texture,
                });
            }
        }
        Ok(())
    })
}
