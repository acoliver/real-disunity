//! The resources a stage's Unity parameters declare, placed through its binding table.

use worldrunner_unity_serialized::shader_program::{Binding, ProgramParameters, TextureParam};

use crate::layout::LayoutError;
use crate::module::ResourceBinding;

/// What kind of resource a parameter declares.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Class {
    ConstantBuffer,
    Texture {
        dimension: wgpu::TextureViewDimension,
        multisampled: bool,
    },
    Sampler,
    StructuredBuffer,
    UnorderedAccess,
}

/// A declared resource at its prepared set and binding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Declared {
    pub(crate) set: u32,
    pub(crate) binding: u32,
    pub(crate) class: Class,
}

fn unsupported(kind: impl Into<String>) -> LayoutError {
    LayoutError::UnsupportedParameter { kind: kind.into() }
}

/// The table entry for a Unity binding, which must be unique across descriptor sets.
pub(crate) fn entry(
    table: &[ResourceBinding],
    unity_binding: i32,
) -> Result<&ResourceBinding, LayoutError> {
    let wanted = u32::try_from(unity_binding).map_err(|_| LayoutError::UnboundResource {
        unity_binding: i64::from(unity_binding),
    })?;
    let mut matches = table.iter().filter(|e| e.unity_binding == wanted);
    match (matches.next(), matches.next()) {
        (Some(found), None) => Ok(found),
        (Some(_), Some(_)) => Err(LayoutError::AmbiguousBinding {
            unity_binding: wanted,
        }),
        _ => Err(LayoutError::UnboundResource {
            unity_binding: i64::from(unity_binding),
        }),
    }
}

/// Unity's `TextureDimension`, with the values of the managed engine's
/// `Disunity/Enums/Rendering.cs`, as a view dimension; `Any`, `None` and unknown have none.
fn view_dimension(code: i8) -> Result<wgpu::TextureViewDimension, LayoutError> {
    match code {
        2 => Ok(wgpu::TextureViewDimension::D2),
        3 => Ok(wgpu::TextureViewDimension::D3),
        4 => Ok(wgpu::TextureViewDimension::Cube),
        5 => Ok(wgpu::TextureViewDimension::D2Array),
        6 => Ok(wgpu::TextureViewDimension::CubeArray),
        other => Err(unsupported(format!("texture dimension {other}"))),
    }
}

/// Every resource `parameters` declares, with duplicates merged.
pub(crate) fn declarations(
    parameters: &ProgramParameters,
    table: &[ResourceBinding],
) -> Result<Vec<Declared>, LayoutError> {
    if !parameters.vectors.is_empty() {
        return Err(unsupported("loose vector"));
    }
    if !parameters.matrices.is_empty() {
        return Err(unsupported("loose matrix"));
    }
    let mut out = Vec::new();
    for binding in &parameters.constant_buffer_bindings {
        out.push(simple(
            table,
            binding,
            "constant buffer",
            Class::ConstantBuffer,
        )?);
    }
    for binding in &parameters.buffers {
        out.push(simple(
            table,
            binding,
            "structured buffer",
            Class::StructuredBuffer,
        )?);
    }
    for uav in &parameters.uavs {
        let placed = entry(table, uav.index)?;
        out.push(declared(placed.set, placed.binding, Class::UnorderedAccess));
    }
    for texture in &parameters.textures {
        out.extend(texture_declarations(table, texture)?);
    }
    for sampler in &parameters.samplers {
        let placed = entry(table, sampler.bind_point)?;
        let binding = placed.sampler_binding.unwrap_or(placed.binding);
        out.push(declared(placed.set, binding, Class::Sampler));
    }
    out.sort_by_key(|d| (d.set, d.binding));
    out.dedup();
    Ok(out)
}

fn declared(set: u32, binding: u32, class: Class) -> Declared {
    Declared {
        set,
        binding,
        class,
    }
}

fn simple(
    table: &[ResourceBinding],
    binding: &Binding,
    what: &str,
    class: Class,
) -> Result<Declared, LayoutError> {
    if binding.array_size > 1 {
        return Err(unsupported(format!("{what} array")));
    }
    let placed = entry(table, binding.index)?;
    Ok(declared(placed.set, placed.binding, class))
}

/// A texture, and the sampler of a combined image sampler that was split.
fn texture_declarations(
    table: &[ResourceBinding],
    texture: &TextureParam,
) -> Result<Vec<Declared>, LayoutError> {
    let placed = entry(table, texture.index)?;
    let class = Class::Texture {
        dimension: view_dimension(texture.dimension)?,
        multisampled: texture.multisampled,
    };
    let mut out = vec![declared(placed.set, placed.binding, class)];
    if let Some(sampler) = placed.sampler_binding {
        out.push(declared(placed.set, sampler, Class::Sampler));
    }
    Ok(out)
}
