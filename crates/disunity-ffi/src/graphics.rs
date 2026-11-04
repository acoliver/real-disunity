//! Shaders, materials and render textures of a world, and the world materials of a mesh
//! renderer's slots. The rules live in world-simulation; this file validates strings and records
//! and forwards.
use crate::{
    DisunityResult,
    callbacks::mutate,
    errors::AbiError,
    mesh_renderer::{renderer, set_renderer},
};
use worldrunner_world_simulation::{
    MAX_RENDERER_MATERIALS,
    graphics::{
        MaterialEdit, MaterialId, RenderTextureId, ShaderId, ShaderSource, WorldRenderTexture,
        WorldShader,
    },
};

/// Longest string accepted, in bytes. The world's own limits are tighter; this bounds what is read.
const MAX_TEXT_BYTES: u32 = 4096;

/// Reads a name or file name the caller passes as UTF-8 bytes.
///
/// # Safety
/// `bytes` spans `length` readable bytes when it is not null.
pub(crate) unsafe fn text(bytes: *const u8, length: u32) -> Result<String, AbiError> {
    if bytes.is_null() || length == 0 || length > MAX_TEXT_BYTES {
        return Err("a name is 1 to 4096 bytes of UTF-8".into());
    }
    std::str::from_utf8(unsafe { std::slice::from_raw_parts(bytes, length as usize) })
        .map(str::to_owned)
        .map_err(|_| "a name must be UTF-8".into())
}

/// Registers a shader by name and writes its id. A shader starts without a source.
///
/// # Safety
/// `name` spans `length` readable bytes; `output` is writable for one id.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_shader_create(
    world: u64,
    name: *const u8,
    length: u32,
    output: *mut u64,
) -> DisunityResult {
    if output.is_null() {
        return DisunityResult::invalid_argument();
    }
    mutate(world, |s| {
        let name = unsafe { text(name, length) }?;
        let id = s.native.create_shader(WorldShader { name, source: None })?;
        unsafe { *output = id.to_bits() };
        Ok(())
    })
}

/// Sets where a shader's programs are: the serialized file the shader object lives in and its
/// path id there. A zero `length` removes the source.
///
/// # Safety
/// `file` spans `length` readable bytes when `length` is not zero.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_shader_source_write(
    world: u64,
    shader: u64,
    file: *const u8,
    length: u32,
    path_id: i64,
) -> DisunityResult {
    mutate(world, |s| {
        let id = ShaderId::from_bits(shader);
        let current = s.native.shader(id)?.clone();
        let source = if length == 0 {
            None
        } else {
            let file = unsafe { text(file, length) }?;
            Some(ShaderSource { file, path_id })
        };
        s.native
            .update_shader(id, WorldShader { source, ..current })?;
        Ok(())
    })
}

/// Removes a shader. Materials that name it keep the stale id.
#[unsafe(no_mangle)]
pub extern "C" fn disunity_shader_destroy(world: u64, shader: u64) -> DisunityResult {
    mutate(world, |s| {
        s.native.destroy_shader(ShaderId::from_bits(shader))?;
        Ok(())
    })
}

/// Creates a material of a live shader and writes its id.
///
/// # Safety
/// `output` is writable for one id.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_material_create(
    world: u64,
    shader: u64,
    output: *mut u64,
) -> DisunityResult {
    if output.is_null() {
        return DisunityResult::invalid_argument();
    }
    mutate(world, |s| {
        let id = s.native.create_material(ShaderId::from_bits(shader))?;
        unsafe { *output = id.to_bits() };
        Ok(())
    })
}

/// Removes a material. Renderer slots that name it read as having none.
#[unsafe(no_mangle)]
pub extern "C" fn disunity_material_destroy(world: u64, material: u64) -> DisunityResult {
    mutate(world, |s| {
        s.native.destroy_material(MaterialId::from_bits(material))?;
        Ok(())
    })
}

fn edit(world: u64, material: u64, edit: MaterialEdit) -> DisunityResult {
    mutate(world, |s| {
        s.native
            .edit_material(MaterialId::from_bits(material), edit)?;
        Ok(())
    })
}

/// Points a material at another live shader.
#[unsafe(no_mangle)]
pub extern "C" fn disunity_material_shader_write(
    world: u64,
    material: u64,
    shader: u64,
) -> DisunityResult {
    edit(
        world,
        material,
        MaterialEdit::Shader(ShaderId::from_bits(shader)),
    )
}

/// Sets the queue a material renders in; -1 leaves the choice to the shader.
#[unsafe(no_mangle)]
pub extern "C" fn disunity_material_render_queue_write(
    world: u64,
    material: u64,
    queue: i32,
) -> DisunityResult {
    edit(world, material, MaterialEdit::RenderQueue(queue))
}

/// Switches a pass, named by its `LightMode`, on (`enabled` 1) or off (0).
///
/// # Safety
/// `name` spans `length` readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_material_pass_write(
    world: u64,
    material: u64,
    name: *const u8,
    length: u32,
    enabled: u32,
) -> DisunityResult {
    mutate(world, |s| {
        if enabled > 1 {
            return Err("a pass is enabled with 0 or 1".into());
        }
        let name = unsafe { text(name, length) }?;
        let change = MaterialEdit::Pass {
            name,
            enabled: enabled == 1,
        };
        s.native
            .edit_material(MaterialId::from_bits(material), change)?;
        Ok(())
    })
}

/// What a material holds besides its properties. `generation` rises with every edit.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DisunityMaterialInfo {
    pub shader: u64,
    pub render_queue: i32,
    pub keyword_count: u32,
    pub disabled_pass_count: u32,
    pub generation: u64,
}

/// # Safety
/// `output` is writable for one record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_material_read(
    world: u64,
    material: u64,
    output: *mut DisunityMaterialInfo,
) -> DisunityResult {
    if output.is_null() {
        return DisunityResult::invalid_argument();
    }
    mutate(world, |s| {
        let held = s.native.material(MaterialId::from_bits(material))?;
        let count = |len: usize| u32::try_from(len).map_err(|_| AbiError::from("count overflow"));
        unsafe {
            *output = DisunityMaterialInfo {
                shader: held.shader.to_bits(),
                render_queue: held.render_queue,
                keyword_count: count(held.keywords.len())?,
                disabled_pass_count: count(held.disabled_passes.len())?,
                generation: held.generation,
            };
        }
        Ok(())
    })
}

/// A render texture's description; booleans are 0 or 1. `format` is the engine's
/// `GraphicsFormat` value. `generation` is ignored on input and starts at 1.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DisunityRenderTexture {
    pub width: u32,
    pub height: u32,
    pub format: u32,
    pub depth_bits: u32,
    pub mip_count: u32,
    pub srgb: u32,
    pub generation: u64,
}

impl DisunityRenderTexture {
    fn to_native(self) -> Result<WorldRenderTexture, AbiError> {
        if self.srgb > 1 {
            return Err("a render texture's sRGB flag is 0 or 1".into());
        }
        Ok(WorldRenderTexture {
            width: self.width,
            height: self.height,
            format: self.format,
            depth_bits: self.depth_bits,
            mip_count: self.mip_count,
            srgb: self.srgb == 1,
            generation: 0,
        })
    }
}

/// Creates a render texture and writes its id.
///
/// # Safety
/// `desc` is readable for one record; `output` is writable for one id.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_render_texture_create(
    world: u64,
    desc: *const DisunityRenderTexture,
    output: *mut u64,
) -> DisunityResult {
    if desc.is_null() || output.is_null() {
        return DisunityResult::invalid_argument();
    }
    let desc = unsafe { *desc };
    mutate(world, |s| {
        let id = s.native.create_render_texture(desc.to_native()?)?;
        unsafe { *output = id.to_bits() };
        Ok(())
    })
}

/// Replaces a render texture's description and raises its generation.
///
/// # Safety
/// `desc` is readable for one record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_render_texture_write(
    world: u64,
    texture: u64,
    desc: *const DisunityRenderTexture,
) -> DisunityResult {
    if desc.is_null() {
        return DisunityResult::invalid_argument();
    }
    let desc = unsafe { *desc };
    mutate(world, |s| {
        s.native
            .update_render_texture(RenderTextureId::from_bits(texture), desc.to_native()?)?;
        Ok(())
    })
}

/// # Safety
/// `output` is writable for one record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_render_texture_read(
    world: u64,
    texture: u64,
    output: *mut DisunityRenderTexture,
) -> DisunityResult {
    if output.is_null() {
        return DisunityResult::invalid_argument();
    }
    mutate(world, |s| {
        let held = s
            .native
            .render_texture(RenderTextureId::from_bits(texture))?;
        unsafe {
            *output = DisunityRenderTexture {
                width: held.width,
                height: held.height,
                format: held.format,
                depth_bits: held.depth_bits,
                mip_count: held.mip_count,
                srgb: u32::from(held.srgb),
                generation: held.generation,
            };
        }
        Ok(())
    })
}

/// Removes a render texture. Whatever names it goes stale.
#[unsafe(no_mangle)]
pub extern "C" fn disunity_render_texture_destroy(world: u64, texture: u64) -> DisunityResult {
    mutate(world, |s| {
        s.native
            .destroy_render_texture(RenderTextureId::from_bits(texture))?;
        Ok(())
    })
}

/// Sets the world material of each slot of a renderer; an id of 0 leaves a slot without one.
/// `count` is at most the renderer's `material_count`, and slots past it have none. Every
/// nonzero id must name a live material. A rejected call changes nothing.
///
/// # Safety
/// `materials` is readable for `count` ids, or null when `count` is zero.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_mesh_renderer_materials_write(
    world: u64,
    handle: u64,
    materials: *const u64,
    count: u32,
) -> DisunityResult {
    if count > MAX_RENDERER_MATERIALS || (count > 0 && materials.is_null()) {
        return DisunityResult::invalid_argument();
    }
    let given: &[u64] = if count == 0 {
        &[]
    } else {
        unsafe { std::slice::from_raw_parts(materials, count as usize) }
    };
    mutate(world, |s| {
        let (id, mut current) = renderer(s, handle)?;
        let mut slots = Vec::with_capacity(given.len());
        for &bits in given {
            slots.push(if bits == 0 {
                None
            } else {
                let material = MaterialId::from_bits(bits);
                s.native.material(material)?;
                Some(material)
            });
        }
        current.slot_materials = slots;
        set_renderer(s, id, current)
    })
}

/// Reads the world material ids of a renderer's slots into `materials` (up to `capacity` ids,
/// 0 for a slot without one) and writes how many slots are set to `count`.
///
/// # Safety
/// `materials` is writable for `capacity` ids, or null when `capacity` is zero; `count` is
/// writable for one value.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_mesh_renderer_materials_read(
    world: u64,
    handle: u64,
    materials: *mut u64,
    capacity: u32,
    count: *mut u32,
) -> DisunityResult {
    if count.is_null() || (capacity > 0 && materials.is_null()) {
        return DisunityResult::invalid_argument();
    }
    mutate(world, |s| {
        let (_, current) = renderer(s, handle)?;
        let held =
            u32::try_from(current.slot_materials.len()).map_err(|_| "slot count overflow")?;
        unsafe { *count = held };
        if capacity == 0 {
            return Ok(());
        }
        if capacity < held {
            return Err("renderer material capacity is too small".into());
        }
        for (index, slot) in current.slot_materials.iter().enumerate() {
            let bits = slot.map_or(0, MaterialId::to_bits);
            unsafe { materials.add(index).write(bits) };
        }
        Ok(())
    })
}
