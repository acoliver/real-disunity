//! The resources a parsed module's entry point uses, read from the module itself.

use naga::valid::ModuleInfo;
use naga::{
    AddressSpace, GlobalVariable, ImageClass, ImageDimension, Module, ScalarKind, StorageAccess,
    TypeInner,
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
        ImageClass::Storage { .. } => return Err(unsupported(global, "storage texture")),
        ImageClass::External => return Err(unsupported(global, "external texture")),
    };
    Ok(wgpu::BindingType::Texture {
        sample_type,
        view_dimension,
        multisampled,
    })
}
