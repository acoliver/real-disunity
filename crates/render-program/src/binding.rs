//! Where a program's resources are bound and what kind each one is.
//!
//! These descriptors belong to no shader format. A frontend declares them for each stage, and
//! [`ProgramLayout`](crate::layout::ProgramLayout) checks them against the resources the stage's
//! entry point reads.

use wgpu::{BindingType, BufferBindingType, SamplerBindingType, ShaderStages};

/// A bind group index and a binding within it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BindingAddress {
    pub set: u32,
    pub binding: u32,
}

/// The name a property is looked up by.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PropertyId(String);

impl PropertyId {
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for PropertyId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// A value the engine supplies to a binding, named by the host that owns it.
///
/// The name is a contract between a program's declarations and the draw that supplies the
/// value, so a host can bind its own frame uniform or texture without renaming anything in
/// the shader. The runtime gives a name no meaning of its own.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum EngineSemantic {
    /// A value or resource the host names, such as `scene.frame`.
    Named(String),
}

impl EngineSemantic {
    #[must_use]
    pub fn named(name: impl Into<String>) -> Self {
        Self::Named(name.into())
    }

    #[must_use]
    pub fn name(&self) -> &str {
        let Self::Named(name) = self;
        name
    }
}

impl std::fmt::Display for EngineSemantic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

/// Where the value of a binding or constant member comes from.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum BindingSource {
    /// A property looked up by name in the caller's layers.
    Property(PropertyId),
    /// A buffer, texture view or sampler the draw imports from its owner, by semantic.
    Engine(EngineSemantic),
    /// A binding the runtime fills itself, such as a sampler.
    Unnamed,
}

/// Whether a storage buffer is written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StorageAccess {
    ReadOnly,
    ReadWrite,
}

/// What a binding holds. A declaration must equal what the shader declares at its address.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResourceKind {
    /// `min_size` bytes at least, or the shader's own need when that is larger.
    Uniform { min_size: u64 },
    Storage {
        access: StorageAccess,
        min_size: u64,
    },
    Texture {
        dimension: wgpu::TextureViewDimension,
        sample: wgpu::TextureSampleType,
        multisampled: bool,
    },
    /// Filtering, non-filtering and comparison samplers are distinct kinds.
    Sampler(SamplerBindingType),
}

impl ResourceKind {
    /// The kind a shader's own binding type declares, or `None` for a binding type this crate
    /// has no kind for.
    #[must_use]
    pub fn of_binding_type(ty: &BindingType, min_size: u64) -> Option<Self> {
        match ty {
            BindingType::Buffer {
                ty: BufferBindingType::Uniform,
                ..
            } => Some(Self::Uniform { min_size }),
            BindingType::Buffer {
                ty: BufferBindingType::Storage { read_only },
                ..
            } => Some(Self::Storage {
                access: if *read_only {
                    StorageAccess::ReadOnly
                } else {
                    StorageAccess::ReadWrite
                },
                min_size,
            }),
            BindingType::Texture {
                sample_type,
                view_dimension,
                multisampled,
            } => Some(Self::Texture {
                dimension: *view_dimension,
                sample: *sample_type,
                multisampled: *multisampled,
            }),
            BindingType::Sampler(kind) => Some(Self::Sampler(*kind)),
            _ => None,
        }
    }

    /// Bytes a buffer binding must hold at least; zero for textures and samplers.
    #[must_use]
    pub fn min_size(&self) -> u64 {
        match self {
            Self::Uniform { min_size } | Self::Storage { min_size, .. } => *min_size,
            Self::Texture { .. } | Self::Sampler(_) => 0,
        }
    }

    /// Whether `ty`, read from a shader, is the same kind of resource. Buffer sizes are bounds
    /// rather than part of the kind.
    #[must_use]
    pub fn matches(&self, ty: &BindingType) -> bool {
        Self::of_binding_type(ty, self.min_size()).is_some_and(|found| found == *self)
    }
}

/// One declared binding.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResourceBinding {
    pub at: BindingAddress,
    pub source: BindingSource,
    /// The stages that can reach the binding, which must include every stage that uses it.
    pub visibility: ShaderStages,
    pub kind: ResourceKind,
}

/// The bindings a stage declares.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ResourceLayout {
    pub bindings: Vec<ResourceBinding>,
}

impl ResourceLayout {
    #[must_use]
    pub fn new(bindings: Vec<ResourceBinding>) -> Self {
        Self { bindings }
    }

    /// The first declaration at `at`.
    #[must_use]
    pub fn find(&self, at: BindingAddress) -> Option<&ResourceBinding> {
        self.bindings.iter().find(|binding| binding.at == at)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn uniform() -> BindingType {
        BindingType::Buffer {
            ty: BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        }
    }

    #[test]
    fn a_kind_matches_the_shader_binding_of_its_own_kind_whatever_the_buffer_size() {
        let kind = ResourceKind::Uniform { min_size: 64 };
        assert!(kind.matches(&uniform()));
        assert!(!kind.matches(&BindingType::Sampler(SamplerBindingType::Filtering)));
    }

    #[test]
    fn storage_access_and_sampler_kinds_are_distinct() {
        let read_only = BindingType::Buffer {
            ty: BufferBindingType::Storage { read_only: true },
            has_dynamic_offset: false,
            min_binding_size: None,
        };
        let writable = BindingType::Buffer {
            ty: BufferBindingType::Storage { read_only: false },
            has_dynamic_offset: false,
            min_binding_size: None,
        };
        let kind = ResourceKind::Storage {
            access: StorageAccess::ReadOnly,
            min_size: 0,
        };
        assert!(kind.matches(&read_only));
        assert!(!kind.matches(&writable));
        let filtering = ResourceKind::Sampler(SamplerBindingType::Filtering);
        assert!(!filtering.matches(&BindingType::Sampler(SamplerBindingType::Comparison)));
    }

    #[test]
    fn a_texture_kind_names_dimension_sample_type_and_sample_count() {
        let ty = BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        };
        let kind = ResourceKind::of_binding_type(&ty, 0).unwrap();
        assert!(kind.matches(&ty));
        let cube = BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: wgpu::TextureViewDimension::Cube,
            multisampled: false,
        };
        assert!(!kind.matches(&cube));
    }

    #[test]
    fn the_first_declaration_at_an_address_is_found() {
        let at = BindingAddress { set: 1, binding: 2 };
        let layout = ResourceLayout::new(vec![ResourceBinding {
            at,
            source: BindingSource::Unnamed,
            visibility: ShaderStages::FRAGMENT,
            kind: ResourceKind::Sampler(SamplerBindingType::Filtering),
        }]);
        assert!(layout.find(at).is_some());
        assert!(layout.find(BindingAddress { set: 1, binding: 3 }).is_none());
    }
}
