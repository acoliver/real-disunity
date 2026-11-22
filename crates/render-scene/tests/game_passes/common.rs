//! Helpers the test games share: program construction and what a game reports to its test.
use std::sync::Arc;
use worldrunner_render_post::{FrameError, PassEntry};
use worldrunner_render_program::binding::{
    BindingAddress, BindingSource, EngineSemantic, ResourceBinding, ResourceKind,
};
use worldrunner_render_program::error::ProgramError;
use worldrunner_render_program::module::{
    StageDeclarations, StageKind, StageModule, StageProgram, StageSource,
};
use worldrunner_render_program::runtime::{GraphicsProgram, ProgramRuntime};
use worldrunner_render_scene::wgpu::{self, util::DeviceExt};

/// Counts a game keeps for its test.
#[derive(Default)]
pub struct Counts {
    pub builds: u32,
    pub prepares: u32,
    /// The entry of every pass the game recorded into, in order.
    pub recorded: Vec<PassEntry>,
}

pub fn graphics(
    runtime: &ProgramRuntime,
    wgsl: &str,
    vertex: &str,
    fragment: &str,
    vertex_declarations: StageDeclarations,
    fragment_declarations: StageDeclarations,
) -> GraphicsProgram {
    let stage = |entry: &str, kind, declarations| {
        StageProgram::new(
            StageModule {
                source: StageSource::Wgsl(Arc::from(wgsl)),
                entry_point: entry.to_owned(),
                kind,
            },
            declarations,
        )
        .unwrap()
    };
    runtime
        .graphics_program(
            &stage(vertex, StageKind::Vertex, vertex_declarations),
            &stage(fragment, StageKind::Fragment, fragment_declarations),
        )
        .unwrap()
}

fn fragment_binding(binding: u32, name: &str, kind: ResourceKind) -> ResourceBinding {
    ResourceBinding {
        at: BindingAddress { set: 0, binding },
        source: BindingSource::Engine(EngineSemantic::named(name)),
        visibility: wgpu::ShaderStages::FRAGMENT,
        kind,
    }
}

pub fn texture_binding(
    binding: u32,
    name: &str,
    sample: wgpu::TextureSampleType,
) -> ResourceBinding {
    fragment_binding(
        binding,
        name,
        ResourceKind::Texture {
            dimension: wgpu::TextureViewDimension::D2,
            sample,
            multisampled: false,
        },
    )
}

pub fn array_binding(binding: u32, name: &str) -> ResourceBinding {
    texture_array_binding(
        binding,
        name,
        wgpu::TextureSampleType::Float { filterable: true },
    )
}

pub fn texture_array_binding(
    binding: u32,
    name: &str,
    sample: wgpu::TextureSampleType,
) -> ResourceBinding {
    fragment_binding(
        binding,
        name,
        ResourceKind::Texture {
            dimension: wgpu::TextureViewDimension::D2Array,
            sample,
            multisampled: false,
        },
    )
}

/// A texture of `dimension` loaded as unfilterable float.
pub fn unfilterable_binding(
    binding: u32,
    name: &str,
    dimension: wgpu::TextureViewDimension,
) -> ResourceBinding {
    fragment_binding(
        binding,
        name,
        ResourceKind::Texture {
            dimension,
            sample: wgpu::TextureSampleType::Float { filterable: false },
            multisampled: false,
        },
    )
}

pub fn uniform_binding(binding: u32, name: &str) -> ResourceBinding {
    fragment_binding(binding, name, ResourceKind::Uniform { min_size: 16 })
}

pub fn vertex_buffer(device: &wgpu::Device, floats: &[f32]) -> wgpu::Buffer {
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("test game vertices"),
        contents: &floats
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .collect::<Vec<_>>(),
        usage: wgpu::BufferUsages::VERTEX,
    })
}

pub fn program_error(game: &'static str, error: &ProgramError) -> FrameError {
    FrameError::FeaturePreparation {
        feature: game,
        reason: error.to_string(),
    }
}
