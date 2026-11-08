//! The resources a parsed module's entry point uses, read from the module itself.

use naga::valid::ModuleInfo;
use naga::{
    AddressSpace, GlobalVariable, ImageClass, ImageDimension, Module, ScalarKind, StorageAccess,
    StorageFormat, TypeInner,
};

use crate::module::ModuleError;

/// A resource the entry point uses, with the binding type its declaration implies.
#[derive(Clone, Debug, PartialEq)]
pub struct Reflected {
    pub set: u32,
    pub binding: u32,
    pub name: String,
    pub ty: wgpu::BindingType,
    /// Bytes the shader needs from a buffer binding; zero for textures and samplers.
    pub min_size: u64,
}

impl Reflected {
    /// The view dimension and multisampling of the resource, if it is a texture.
    #[must_use]
    pub fn texture(&self) -> Option<(wgpu::TextureViewDimension, bool)> {
        match self.ty {
            wgpu::BindingType::Texture {
                view_dimension,
                multisampled,
                ..
            } => Some((view_dimension, multisampled)),
            _ => None,
        }
    }
}

/// Every resource of `module` that entry point `entry` uses.
pub(crate) fn reflect(
    module: &Module,
    info: &ModuleInfo,
    entry: usize,
) -> Result<Vec<Reflected>, ModuleError> {
    let usage = info.get_entry_point(entry);
    let mut resources = Vec::new();
    for (handle, global) in module.global_variables.iter() {
        if usage[handle].is_empty() {
            continue;
        }
        if let Some(resource) = describe(module, global)? {
            resources.push(resource);
        }
    }
    resources.sort_by_key(|r| (r.set, r.binding));
    Ok(resources)
}

fn unsupported(global: &GlobalVariable, what: &str) -> ModuleError {
    ModuleError::UnsupportedResource {
        name: global.name.clone().unwrap_or_default(),
        what: what.to_owned(),
    }
}

/// `None` for a variable that is not a resource: private and workgroup memory.
fn describe(module: &Module, global: &GlobalVariable) -> Result<Option<Reflected>, ModuleError> {
    let ty = &module.types[global.ty].inner;
    let (binding_type, min_size) = match (&global.space, ty) {
        (AddressSpace::Private | AddressSpace::WorkGroup | AddressSpace::Function, _) => {
            return Ok(None);
        }
        (AddressSpace::Uniform, _) => (buffer(wgpu::BufferBindingType::Uniform), size(module, ty)),
        (AddressSpace::Storage { access }, _) => {
            let read_only = !access.contains(StorageAccess::STORE);
            (
                buffer(wgpu::BufferBindingType::Storage { read_only }),
                size(module, ty),
            )
        }
        (AddressSpace::Handle, TypeInner::Sampler { comparison }) => (
            wgpu::BindingType::Sampler(if *comparison {
                wgpu::SamplerBindingType::Comparison
            } else {
                wgpu::SamplerBindingType::Filtering
            }),
            0,
        ),
        (
            AddressSpace::Handle,
            TypeInner::Image {
                dim,
                arrayed,
                class,
            },
        ) => (texture(global, *dim, *arrayed, *class)?, 0),
        (AddressSpace::Handle, TypeInner::BindingArray { .. }) => {
            return Err(unsupported(global, "binding array"));
        }
        _ => return Err(unsupported(global, "address space")),
    };
    let Some(binding) = &global.binding else {
        return Err(unsupported(global, "resource without a binding"));
    };
    Ok(Some(Reflected {
        set: binding.group,
        binding: binding.binding,
        name: global.name.clone().unwrap_or_default(),
        ty: binding_type,
        min_size,
    }))
}

fn buffer(ty: wgpu::BufferBindingType) -> wgpu::BindingType {
    wgpu::BindingType::Buffer {
        ty,
        has_dynamic_offset: false,
        min_binding_size: None,
    }
}

fn size(module: &Module, ty: &TypeInner) -> u64 {
    u64::from(ty.size(module.to_ctx()))
}

fn texture(
    global: &GlobalVariable,
    dim: ImageDimension,
    arrayed: bool,
    class: ImageClass,
) -> Result<wgpu::BindingType, ModuleError> {
    let view_dimension = match (dim, arrayed) {
        (ImageDimension::D1, false) => wgpu::TextureViewDimension::D1,
        (ImageDimension::D2, false) => wgpu::TextureViewDimension::D2,
        (ImageDimension::D2, true) => wgpu::TextureViewDimension::D2Array,
        (ImageDimension::D3, false) => wgpu::TextureViewDimension::D3,
        (ImageDimension::Cube, false) => wgpu::TextureViewDimension::Cube,
        (ImageDimension::Cube, true) => wgpu::TextureViewDimension::CubeArray,
        _ => return Err(unsupported(global, "image dimension")),
    };
    let (sample_type, multisampled) = match class {
        ImageClass::Sampled { kind, multi } => (
            match kind {
                ScalarKind::Float => wgpu::TextureSampleType::Float { filterable: !multi },
                ScalarKind::Sint => wgpu::TextureSampleType::Sint,
                ScalarKind::Uint => wgpu::TextureSampleType::Uint,
                _ => return Err(unsupported(global, "image sample kind")),
            },
            multi,
        ),
        ImageClass::Depth { multi } => (wgpu::TextureSampleType::Depth, multi),
        ImageClass::Storage { format, access } => {
            return storage_texture(global, (format, access), view_dimension);
        }
        ImageClass::External => return Err(unsupported(global, "external texture")),
    };
    Ok(wgpu::BindingType::Texture {
        sample_type,
        view_dimension,
        multisampled,
    })
}

/// A storage image binding: the access the declaration allows, the format it names and the
/// dimension of the views it takes.
fn storage_texture(
    global: &GlobalVariable,
    (format, access): (StorageFormat, StorageAccess),
    view_dimension: wgpu::TextureViewDimension,
) -> Result<wgpu::BindingType, ModuleError> {
    if matches!(
        view_dimension,
        wgpu::TextureViewDimension::Cube | wgpu::TextureViewDimension::CubeArray
    ) {
        return Err(unsupported(global, "cube storage image"));
    }
    let load_store = StorageAccess::LOAD | StorageAccess::STORE;
    let access = match access {
        StorageAccess::LOAD => wgpu::StorageTextureAccess::ReadOnly,
        StorageAccess::STORE => wgpu::StorageTextureAccess::WriteOnly,
        found if found == load_store => wgpu::StorageTextureAccess::ReadWrite,
        found if found.contains(StorageAccess::ATOMIC) => {
            return Err(unsupported(global, "atomic storage image"));
        }
        _ => return Err(unsupported(global, "storage image without access")),
    };
    Ok(wgpu::BindingType::StorageTexture {
        access,
        format: storage_format(format),
        view_dimension,
    })
}

/// The texture format a shader's storage format names.
fn storage_format(format: StorageFormat) -> wgpu::TextureFormat {
    use wgpu::TextureFormat as F;
    match format {
        StorageFormat::R8Unorm => F::R8Unorm,
        StorageFormat::R8Snorm => F::R8Snorm,
        StorageFormat::R8Uint => F::R8Uint,
        StorageFormat::R8Sint => F::R8Sint,
        StorageFormat::R16Uint => F::R16Uint,
        StorageFormat::R16Sint => F::R16Sint,
        StorageFormat::R16Float => F::R16Float,
        StorageFormat::Rg8Unorm => F::Rg8Unorm,
        StorageFormat::Rg8Snorm => F::Rg8Snorm,
        StorageFormat::Rg8Uint => F::Rg8Uint,
        StorageFormat::Rg8Sint => F::Rg8Sint,
        StorageFormat::R32Uint => F::R32Uint,
        StorageFormat::R32Sint => F::R32Sint,
        StorageFormat::R32Float => F::R32Float,
        StorageFormat::Rg16Uint => F::Rg16Uint,
        StorageFormat::Rg16Sint => F::Rg16Sint,
        StorageFormat::Rg16Float => F::Rg16Float,
        StorageFormat::Rgba8Unorm => F::Rgba8Unorm,
        StorageFormat::Rgba8Snorm => F::Rgba8Snorm,
        StorageFormat::Rgba8Uint => F::Rgba8Uint,
        StorageFormat::Rgba8Sint => F::Rgba8Sint,
        StorageFormat::Bgra8Unorm => F::Bgra8Unorm,
        StorageFormat::Rgb10a2Uint => F::Rgb10a2Uint,
        StorageFormat::Rgb10a2Unorm => F::Rgb10a2Unorm,
        StorageFormat::Rg11b10Ufloat => F::Rg11b10Ufloat,
        StorageFormat::R64Uint => F::R64Uint,
        StorageFormat::Rg32Uint => F::Rg32Uint,
        StorageFormat::Rg32Sint => F::Rg32Sint,
        StorageFormat::Rg32Float => F::Rg32Float,
        StorageFormat::Rgba16Uint => F::Rgba16Uint,
        StorageFormat::Rgba16Sint => F::Rgba16Sint,
        StorageFormat::Rgba16Float => F::Rgba16Float,
        StorageFormat::Rgba32Uint => F::Rgba32Uint,
        StorageFormat::Rgba32Sint => F::Rgba32Sint,
        StorageFormat::Rgba32Float => F::Rgba32Float,
        StorageFormat::R16Unorm => F::R16Unorm,
        StorageFormat::R16Snorm => F::R16Snorm,
        StorageFormat::Rg16Unorm => F::Rg16Unorm,
        StorageFormat::Rg16Snorm => F::Rg16Snorm,
        StorageFormat::Rgba16Unorm => F::Rgba16Unorm,
        StorageFormat::Rgba16Snorm => F::Rgba16Snorm,
    }
}
