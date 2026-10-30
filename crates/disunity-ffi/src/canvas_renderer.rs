//! Canvas renderer component settings. The rules live in world-simulation; this file converts
//! records.
use crate::{DisunityResult, callbacks::mutate, errors::AbiError, physics::component};
use worldrunner_world_simulation::{
    ComponentDesc, ComponentHandle, WorldError,
    ui::{CanvasRenderer, CanvasRendererFlags, inherited_alpha},
};

/// A canvas renderer's settings; booleans are 0 or 1. `mesh`, `texture` and `alpha_texture` are
/// managed instance ids (0 for none). The clip rectangle is x, y, width, height and only counts
/// when `has_rect_clipping` is 1. The materials have their own calls. A write rejects a NaN
/// alpha, a count above 8 or a boolean above 1, leaving the renderer unchanged, and clears the
/// material slots past the new counts.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct DisunityCanvasRenderer {
    pub mesh: u64,
    pub texture: u64,
    pub alpha_texture: u64,
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
    pub alpha: f32,
    pub cull: u32,
    pub cull_transparent_mesh: u32,
    pub has_pop_instruction: u32,
    pub has_rect_clipping: u32,
    pub clip_x: f32,
    pub clip_y: f32,
    pub clip_width: f32,
    pub clip_height: f32,
    pub softness_x: f32,
    pub softness_y: f32,
    pub material_count: u32,
    pub pop_material_count: u32,
}

impl Default for DisunityCanvasRenderer {
    /// The settings of a new canvas renderer.
    fn default() -> Self {
        Self::from_native(CanvasRenderer::default())
    }
}

impl DisunityCanvasRenderer {
    fn from_native(renderer: CanvasRenderer) -> Self {
        let [r, g, b, a] = renderer.color;
        let [clip_x, clip_y, clip_width, clip_height] = renderer.clip_rect.unwrap_or([0.; 4]);
        let [softness_x, softness_y] = renderer.clipping_softness;
        Self {
            mesh: renderer.mesh,
            texture: renderer.texture,
            alpha_texture: renderer.alpha_texture,
            r,
            g,
            b,
            a,
            alpha: renderer.alpha,
            cull: u32::from(renderer.flags.cull),
            cull_transparent_mesh: u32::from(renderer.flags.cull_transparent_mesh),
            has_pop_instruction: u32::from(renderer.flags.has_pop_instruction),
            has_rect_clipping: u32::from(renderer.clip_rect.is_some()),
            clip_x,
            clip_y,
            clip_width,
            clip_height,
            softness_x,
            softness_y,
            material_count: renderer.material_count,
            pop_material_count: renderer.pop_material_count,
        }
    }

    /// The renderer `current` becomes: every setting of the record, and the materials of
    /// `current` that still fit the counts.
    fn to_native(self, current: CanvasRenderer) -> Result<CanvasRenderer, AbiError> {
        let flags = [
            self.cull,
            self.cull_transparent_mesh,
            self.has_pop_instruction,
            self.has_rect_clipping,
        ];
        if flags.iter().any(|flag| *flag > 1) {
            return Err("canvas renderer boolean settings must be 0 or 1".into());
        }
        let renderer = CanvasRenderer {
            color: [self.r, self.g, self.b, self.a],
            alpha: self.alpha,
            flags: CanvasRendererFlags {
                cull: self.cull == 1,
                cull_transparent_mesh: self.cull_transparent_mesh == 1,
                has_pop_instruction: self.has_pop_instruction == 1,
            },
            clip_rect: (self.has_rect_clipping == 1).then_some([
                self.clip_x,
                self.clip_y,
                self.clip_width,
                self.clip_height,
            ]),
            clipping_softness: [self.softness_x, self.softness_y],
            material_count: self.material_count,
            pop_material_count: self.pop_material_count,
            mesh: self.mesh,
            texture: self.texture,
            alpha_texture: self.alpha_texture,
            ..current
        };
        Ok(renderer.normalized().map_err(WorldError::from)?)
    }
}

fn renderer(
    session: &crate::store::Session,
    handle: u64,
) -> Result<(ComponentHandle, CanvasRenderer), AbiError> {
    let id = component(session, handle)?;
    match session.native.component(id)?.desc {
        ComponentDesc::CanvasRenderer(renderer) => Ok((id, renderer)),
        _ => Err("component is not a CanvasRenderer".into()),
    }
}

/// Creates a canvas renderer with the engine's default settings on a live object.
/// # Safety
/// Output is writable for one component identity.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_canvas_renderer_create(
    world: u64,
    object: u64,
    output: *mut u64,
) -> DisunityResult {
    if output.is_null() {
        return DisunityResult::invalid_argument();
    }
    let desc = ComponentDesc::CanvasRenderer(CanvasRenderer::default());
    match crate::physics::create(world, object, desc) {
        Ok(id) => {
            unsafe { *output = id };
            DisunityResult::ok()
        }
        Err(e) => crate::abi::translate_error(Some(world), &e),
    }
}

/// # Safety
/// Output is writable for one CanvasRenderer record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_canvas_renderer_read(
    world: u64,
    handle: u64,
    output: *mut DisunityCanvasRenderer,
) -> DisunityResult {
    if output.is_null() {
        return DisunityResult::invalid_argument();
    }
    mutate(world, |s| {
        let (_, value) = renderer(s, handle)?;
        unsafe { *output = DisunityCanvasRenderer::from_native(value) };
        Ok(())
    })
}

/// Validates every setting before changing the renderer; a rejected write changes nothing.
/// # Safety
/// Value is readable for one CanvasRenderer record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_canvas_renderer_write(
    world: u64,
    handle: u64,
    value: *const DisunityCanvasRenderer,
) -> DisunityResult {
    if value.is_null() {
        return DisunityResult::invalid_argument();
    }
    let value = unsafe { *value };
    mutate(world, |s| {
        let (id, current) = renderer(s, handle)?;
        let desc = ComponentDesc::CanvasRenderer(value.to_native(current)?);
        s.native.set_component(id, desc)?;
        Ok(())
    })
}

fn list(pop: u32) -> Result<bool, AbiError> {
    match pop {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err("canvas renderer material list must be 0 (materials) or 1 (pop materials)".into()),
    }
}

/// Reads one material identity; `pop` is 0 for the materials and 1 for the pop materials. An
/// `index` at or past the count of that list is rejected.
/// # Safety
/// Output is writable for one instance id.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_canvas_renderer_material_read(
    world: u64,
    handle: u64,
    pop: u32,
    index: u32,
    output: *mut u64,
) -> DisunityResult {
    if output.is_null() {
        return DisunityResult::invalid_argument();
    }
    mutate(world, |s| {
        let (_, value) = renderer(s, handle)?;
        let (count, slots) = if list(pop)? {
            (value.pop_material_count, value.pop_materials)
        } else {
            (value.material_count, value.materials)
        };
        if index >= count {
            return Err("canvas renderer material index is past the count".into());
        }
        unsafe { *output = slots[index as usize] };
        Ok(())
    })
}

/// Writes one material identity (0 for none); `pop` and `index` are as for the read.
#[unsafe(no_mangle)]
pub extern "C" fn disunity_canvas_renderer_material_write(
    world: u64,
    handle: u64,
    pop: u32,
    index: u32,
    instance: u64,
) -> DisunityResult {
    mutate(world, |s| {
        let (id, current) = renderer(s, handle)?;
        let next = current
            .with_material(list(pop)?, index, instance)
            .map_err(WorldError::from)?;
        s.native
            .set_component(id, ComponentDesc::CanvasRenderer(next))?;
        Ok(())
    })
}

/// Reads the alpha the renderer's element is drawn with: the renderer's alpha times the alpha
/// of the enabled canvas groups above it.
/// # Safety
/// Output is writable for one f32.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_canvas_renderer_inherited_alpha(
    world: u64,
    handle: u64,
    output: *mut f32,
) -> DisunityResult {
    if output.is_null() {
        return DisunityResult::invalid_argument();
    }
    mutate(world, |s| {
        let (id, _) = renderer(s, handle)?;
        unsafe { *output = inherited_alpha(&s.native, id)? };
        Ok(())
    })
}
