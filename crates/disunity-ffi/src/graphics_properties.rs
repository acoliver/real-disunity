//! Shader property values and keywords on a material, in the global shader state or in a
//! renderer's property block. A scope number and a target pick the holder: scope 0 is the
//! material whose id is `target`, scope 1 the global state (`target` is ignored) and scope 2 the
//! property block of the mesh renderer component `target`.
use crate::{
    DisunityResult,
    callbacks::mutate,
    errors::AbiError,
    graphics::text,
    mesh_renderer::{renderer, set_renderer},
    store::Session,
    textures::FIRST_PLAYER_TEXTURE,
};
use worldrunner_world_simulation::{
    ComponentHandle,
    graphics::{
        MAX_PROPERTY_ARRAY_ELEMENTS, MaterialEdit, MaterialId, PropertyValue, RenderTextureId,
        ShaderProperties, TextureRef, TextureSlot,
    },
};

const SCOPE_MATERIAL: u32 = 0;
const SCOPE_GLOBAL: u32 = 1;
const SCOPE_BLOCK: u32 = 2;

const TEXTURE_NONE: u32 = 0;
const TEXTURE_ASSET: u32 = 1;
const TEXTURE_RENDER: u32 = 2;
/// A global texture, which only compute bindings name; the name travels beside the record.
pub(crate) const TEXTURE_GLOBAL: u32 = 3;

/// A texture property. `kind` is 0 for no texture (`texture` must be 0), 1 for an asset texture
/// by the identity the host knows it by (a player texture identity) and 2 for a world render
/// texture by its id.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DisunityTextureSlot {
    pub kind: u32,
    pub texture: u64,
    pub scale_x: f32,
    pub scale_y: f32,
    pub offset_x: f32,
    pub offset_y: f32,
}

/// Converts a texture kind and identity to the world's reference.
pub(crate) fn texture_ref(kind: u32, texture: u64) -> Result<TextureRef, AbiError> {
    match kind {
        TEXTURE_NONE if texture == 0 => Ok(TextureRef::None),
        TEXTURE_ASSET if texture >= FIRST_PLAYER_TEXTURE => Ok(TextureRef::Asset(texture)),
        TEXTURE_RENDER if texture != 0 => {
            Ok(TextureRef::Render(RenderTextureId::from_bits(texture)))
        }
        _ => Err(
            "a texture is none (0), a player texture identity (1) or a render texture (2)".into(),
        ),
    }
}

/// The kind and identity a texture reference is written as.
pub(crate) fn texture_parts(texture: &TextureRef) -> (u32, u64) {
    match texture {
        TextureRef::None => (TEXTURE_NONE, 0),
        TextureRef::Asset(identity) => (TEXTURE_ASSET, *identity),
        TextureRef::Render(id) => (TEXTURE_RENDER, id.to_bits()),
        TextureRef::Global(_) => (TEXTURE_GLOBAL, 0),
    }
}

impl DisunityTextureSlot {
    pub(crate) fn to_native(self) -> Result<TextureSlot, AbiError> {
        Ok(TextureSlot {
            texture: texture_ref(self.kind, self.texture)?,
            scale: [self.scale_x, self.scale_y],
            offset: [self.offset_x, self.offset_y],
        })
    }
}

#[derive(Clone, Copy)]
enum Scope {
    Material(MaterialId),
    Global,
    Block(ComponentHandle),
}

fn scope_of(session: &Session, scope: u32, target: u64) -> Result<Scope, AbiError> {
    match scope {
        SCOPE_MATERIAL => Ok(Scope::Material(MaterialId::from_bits(target))),
        SCOPE_GLOBAL => Ok(Scope::Global),
        SCOPE_BLOCK => Ok(Scope::Block(renderer(session, target)?.0)),
        _ => Err("a shader scope is 0 (material), 1 (global) or 2 (property block)".into()),
    }
}

fn apply(session: &mut Session, scope: Scope, value: PropertyValue) -> Result<(), AbiError> {
    match scope {
        Scope::Material(id) => session
            .native
            .edit_material(id, MaterialEdit::Property(value))?,
        Scope::Global => session.native.set_global_property(value)?,
        Scope::Block(handle) => {
            session.native.check_property(&value)?;
            let mut current = crate::mesh_renderer::renderer_at(session, handle)?;
            current
                .property_block
                .get_or_insert_with(ShaderProperties::default)
                .set(value)
                .map_err(worldrunner_world_simulation::WorldError::from)?;
            set_renderer(session, handle, current)?;
        }
    }
    Ok(())
}

fn write(
    world: u64,
    scope: u32,
    target: u64,
    make: impl FnOnce() -> Result<PropertyValue, AbiError>,
) -> DisunityResult {
    mutate(world, |s| {
        let scope = scope_of(s, scope, target)?;
        apply(s, scope, make()?)
    })
}

/// Sets a float property, replacing the value of the same name.
///
/// # Safety
/// `name` spans `length` readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_shader_float_write(
    world: u64,
    scope: u32,
    target: u64,
    name: *const u8,
    length: u32,
    value: f32,
) -> DisunityResult {
    write(world, scope, target, || {
        Ok(PropertyValue::Float(unsafe { text(name, length) }?, value))
    })
}

/// Sets an integer property.
///
/// # Safety
/// `name` spans `length` readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_shader_int_write(
    world: u64,
    scope: u32,
    target: u64,
    name: *const u8,
    length: u32,
    value: i32,
) -> DisunityResult {
    write(world, scope, target, || {
        Ok(PropertyValue::Int(unsafe { text(name, length) }?, value))
    })
}

/// Sets a vector or colour property from 4 floats.
///
/// # Safety
/// `name` spans `length` readable bytes; `values` is readable for 4 floats.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_shader_vector_write(
    world: u64,
    scope: u32,
    target: u64,
    name: *const u8,
    length: u32,
    values: *const f32,
) -> DisunityResult {
    write(world, scope, target, || {
        if values.is_null() {
            return Err("a vector property needs 4 floats".into());
        }
        let mut vector = [0.0; 4];
        unsafe { std::ptr::copy_nonoverlapping(values, vector.as_mut_ptr(), vector.len()) };
        Ok(PropertyValue::Vector(
            unsafe { text(name, length) }?,
            vector,
        ))
    })
}

/// Sets a matrix property from 16 floats in column-major order.
///
/// # Safety
/// `name` spans `length` readable bytes; `values` is readable for 16 floats.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_shader_matrix_write(
    world: u64,
    scope: u32,
    target: u64,
    name: *const u8,
    length: u32,
    values: *const f32,
) -> DisunityResult {
    write(world, scope, target, || {
        if values.is_null() {
            return Err("a matrix property needs 16 floats".into());
        }
        let mut matrix = [0.0; 16];
        unsafe { std::ptr::copy_nonoverlapping(values, matrix.as_mut_ptr(), matrix.len()) };
        Ok(PropertyValue::Matrix(
            unsafe { text(name, length) }?,
            matrix,
        ))
    })
}

/// Copies `count` elements of `N` floats out of `values`, or reports why the call is invalid.
/// Nothing is read unless the pointer is non-null and aligned and the count is 1 to 1023 with a
/// byte size that fits an allocation.
///
/// # Safety
/// `values` is readable for `count * N` floats when it passes those checks.
pub(crate) unsafe fn copy_elements<const N: usize>(
    values: *const f32,
    count: u32,
) -> Result<Vec<[f32; N]>, DisunityResult> {
    let count = usize::try_from(count).map_err(|_| DisunityResult::invalid_argument())?;
    let bytes = count
        .checked_mul(N)
        .and_then(|scalars| scalars.checked_mul(std::mem::size_of::<f32>()))
        .filter(|bytes| isize::try_from(*bytes).is_ok());
    let Some(bytes) = bytes else {
        return Err(DisunityResult::invalid_argument());
    };
    if values.is_null()
        || !values.is_aligned()
        || count == 0
        || count > MAX_PROPERTY_ARRAY_ELEMENTS
        || bytes == 0
    {
        return Err(DisunityResult::invalid_argument());
    }
    let source = unsafe { std::slice::from_raw_parts(values, bytes / std::mem::size_of::<f32>()) };
    let mut elements = Vec::new();
    if elements.try_reserve_exact(count).is_err() {
        return Err(DisunityResult::error(
            crate::abi::DisunityErrorReason::Capacity as u64,
        ));
    }
    for chunk in source.chunks_exact(N) {
        let element =
            <[f32; N]>::try_from(chunk).map_err(|_| DisunityResult::invalid_argument())?;
        elements.push(element);
    }
    Ok(elements)
}

/// Sets a global vector array from `count` vectors of 4 floats (x, y, z, w) at `values`, in
/// element order. The count is 1 to 1023 and every float must be finite. The floats are copied
/// before the call returns.
///
/// # Safety
/// `name` spans `length` readable bytes; `values` is readable for `count * 4` floats.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_shader_global_vector_array_write(
    world: u64,
    name: *const u8,
    length: u32,
    values: *const f32,
    count: u32,
) -> DisunityResult {
    let elements = match unsafe { copy_elements::<4>(values, count) } {
        Ok(elements) => elements,
        Err(refused) => return refused,
    };
    write(world, SCOPE_GLOBAL, 0, || {
        Ok(PropertyValue::VectorArray(
            unsafe { text(name, length) }?,
            elements,
        ))
    })
}

/// Sets a global matrix array from `count` matrices of 16 floats at `values`, each in
/// column-major order, in element order. The count is 1 to 1023 and every float must be finite.
/// The floats are copied before the call returns.
///
/// # Safety
/// `name` spans `length` readable bytes; `values` is readable for `count * 16` floats.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_shader_global_matrix_array_write(
    world: u64,
    name: *const u8,
    length: u32,
    values: *const f32,
    count: u32,
) -> DisunityResult {
    let elements = match unsafe { copy_elements::<16>(values, count) } {
        Ok(elements) => elements,
        Err(refused) => return refused,
    };
    write(world, SCOPE_GLOBAL, 0, || {
        Ok(PropertyValue::MatrixArray(
            unsafe { text(name, length) }?,
            elements,
        ))
    })
}

/// Sets a texture property with its scale and offset.
///
/// # Safety
/// `name` spans `length` readable bytes; `slot` is readable for one record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_shader_texture_write(
    world: u64,
    scope: u32,
    target: u64,
    name: *const u8,
    length: u32,
    slot: *const DisunityTextureSlot,
) -> DisunityResult {
    write(world, scope, target, || {
        if slot.is_null() {
            return Err("a texture property needs a slot record".into());
        }
        let slot = unsafe { *slot }.to_native()?;
        Ok(PropertyValue::Texture(unsafe { text(name, length) }?, slot))
    })
}

/// Removes every property value from a material, the global state or a renderer's property
/// block. Keywords stay.
#[unsafe(no_mangle)]
pub extern "C" fn disunity_shader_properties_clear(
    world: u64,
    scope: u32,
    target: u64,
) -> DisunityResult {
    mutate(world, |s| {
        match scope_of(s, scope, target)? {
            Scope::Material(id) => s.native.edit_material(id, MaterialEdit::ClearProperties)?,
            Scope::Global => s.native.clear_global_properties(),
            Scope::Block(handle) => {
                let mut current = crate::mesh_renderer::renderer_at(s, handle)?;
                current.property_block = None;
                set_renderer(s, handle, current)?;
            }
        }
        Ok(())
    })
}

/// Enables (`enabled` 1) or disables (0) a keyword on a material or in the global state. A
/// property block has no keywords.
///
/// # Safety
/// `name` spans `length` readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_shader_keyword_write(
    world: u64,
    scope: u32,
    target: u64,
    name: *const u8,
    length: u32,
    enabled: u32,
) -> DisunityResult {
    mutate(world, |s| {
        if enabled > 1 {
            return Err("a keyword is enabled with 0 or 1".into());
        }
        let name = unsafe { text(name, length) }?;
        let enabled = enabled == 1;
        match scope_of(s, scope, target)? {
            Scope::Material(id) => s
                .native
                .edit_material(id, MaterialEdit::Keyword { name, enabled })?,
            Scope::Global => s.native.set_global_keyword(&name, enabled)?,
            Scope::Block(_) => return Err("a property block has no keywords".into()),
        }
        Ok(())
    })
}

/// Writes 1 to `enabled` when the keyword is enabled on the material or in the global state.
///
/// # Safety
/// `name` spans `length` readable bytes; `enabled` is writable for one value.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_shader_keyword_read(
    world: u64,
    scope: u32,
    target: u64,
    name: *const u8,
    length: u32,
    enabled: *mut u32,
) -> DisunityResult {
    if enabled.is_null() {
        return DisunityResult::invalid_argument();
    }
    mutate(world, |s| {
        let name = unsafe { text(name, length) }?;
        let on = match scope_of(s, scope, target)? {
            Scope::Material(id) => s.native.material(id)?.keywords.contains(&name),
            Scope::Global => s.native.global_shader_state().keywords.contains(&name),
            Scope::Block(_) => return Err("a property block has no keywords".into()),
        };
        unsafe { *enabled = u32::from(on) };
        Ok(())
    })
}

/// The values a property name holds, one per kind. `present` has bit 0 set for a float, bit 1
/// for an integer, bit 2 for a vector, bit 3 for a matrix and bit 4 for a texture; the fields of
/// a kind that is not present are zero.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DisunityPropertyRead {
    pub present: u32,
    pub float_value: f32,
    pub int_value: i32,
    pub vector_x: f32,
    pub vector_y: f32,
    pub vector_z: f32,
    pub vector_w: f32,
    pub texture_kind: u32,
    pub texture: u64,
    pub scale_x: f32,
    pub scale_y: f32,
    pub offset_x: f32,
    pub offset_y: f32,
}

fn read_values(properties: &ShaderProperties, name: &str) -> DisunityPropertyRead {
    let mut read = DisunityPropertyRead::default();
    if let Some(value) = properties.float(name) {
        read.present |= 1;
        read.float_value = value;
    }
    if let Some(value) = properties.int(name) {
        read.present |= 2;
        read.int_value = value;
    }
    if let Some([x, y, z, w]) = properties.vector(name) {
        read.present |= 4;
        (read.vector_x, read.vector_y, read.vector_z, read.vector_w) = (x, y, z, w);
    }
    if properties.matrix(name).is_some() {
        read.present |= 8;
    }
    if let Some(slot) = properties.texture(name) {
        read.present |= 16;
        (read.texture_kind, read.texture) = texture_parts(&slot.texture);
        [read.scale_x, read.scale_y] = slot.scale;
        [read.offset_x, read.offset_y] = slot.offset;
    }
    read
}

/// Reads every kind of value a property name holds in a material, the global state or a
/// renderer's property block.
///
/// # Safety
/// `name` spans `length` readable bytes; `output` is writable for one record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_shader_property_read(
    world: u64,
    scope: u32,
    target: u64,
    name: *const u8,
    length: u32,
    output: *mut DisunityPropertyRead,
) -> DisunityResult {
    if output.is_null() {
        return DisunityResult::invalid_argument();
    }
    mutate(world, |s| {
        let name = unsafe { text(name, length) }?;
        let read = match scope_of(s, scope, target)? {
            Scope::Material(id) => read_values(&s.native.material(id)?.properties, &name),
            Scope::Global => read_values(&s.native.global_shader_state().properties, &name),
            Scope::Block(handle) => {
                let current = crate::mesh_renderer::renderer_at(s, handle)?;
                read_values(&current.property_block.unwrap_or_default(), &name)
            }
        };
        unsafe { *output = read };
        Ok(())
    })
}
