//! How many storage images a scene device lets one shader stage bind: the adapter's limit, up to
//! what the engine's kernels bind. A single-pass downsampler binds seven (its source and six
//! levels), past wgpu's default of four, so a device that kept the default refused it by name.
//!
//! The devices here are wgpu's no-op device made with the scene's request against an adapter's
//! limits; it validates layouts against those limits as a real device does.
use super::requirements;
use std::sync::Arc;
use worldrunner_render_gpu::configuration::AllocationBudget;
use worldrunner_render_gpu::submission::SubmissionAuthority;
use worldrunner_render_post::TimingMode;
use worldrunner_render_post::contracts::TextureRange;
use worldrunner_render_program::binding::{
    BindingAddress, BindingSource, EngineSemantic, ResourceBinding, ResourceKind, ResourceLayout,
};
use worldrunner_render_program::error::ProgramError;
use worldrunner_render_program::module::{
    StageDeclarations, StageKind, StageModule, StageProgram, StageSource,
};
use worldrunner_render_program::runtime::{Import, ImportedDispatch, ProgramRuntime};

/// Images of one 64 x 64 `R32Float` texture of seven mips: the kernel writes mip `n` of the
/// texture through image `n`, where the invocation's texel is inside it.
const LEVELS: u32 = 7;
const WGSL: &str = "
@group(0) @binding(0) var m0: texture_storage_2d<r32float, write>;
@group(0) @binding(1) var m1: texture_storage_2d<r32float, write>;
@group(0) @binding(2) var m2: texture_storage_2d<r32float, write>;
@group(0) @binding(3) var m3: texture_storage_2d<r32float, write>;
@group(0) @binding(4) var m4: texture_storage_2d<r32float, write>;
@group(0) @binding(5) var m5: texture_storage_2d<r32float, write>;
@group(0) @binding(6) var m6: texture_storage_2d<r32float, write>;
@compute @workgroup_size(8, 8) fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let p = vec2<i32>(id.xy);
    let v = vec4<f32>(f32(id.x + 64u * id.y), 0.0, 0.0, 0.0);
    textureStore(m0, p, v);
    if (all(id.xy < vec2<u32>(32u))) { textureStore(m1, p, v + 1.0); }
    if (all(id.xy < vec2<u32>(16u))) { textureStore(m2, p, v + 2.0); }
    if (all(id.xy < vec2<u32>(8u))) { textureStore(m3, p, v + 3.0); }
    if (all(id.xy < vec2<u32>(4u))) { textureStore(m4, p, v + 4.0); }
    if (all(id.xy < vec2<u32>(2u))) { textureStore(m5, p, v + 5.0); }
    if (all(id.xy < vec2<u32>(1u))) { textureStore(m6, p, v + 6.0); }
}";

fn semantic(level: u32) -> EngineSemantic {
    EngineSemantic::named(["m0", "m1", "m2", "m3", "m4", "m5", "m6"][level as usize])
}

fn stage() -> StageProgram {
    let images = (0..LEVELS)
        .map(|level| ResourceBinding {
            at: BindingAddress {
                set: 0,
                binding: level,
            },
            source: BindingSource::Engine(semantic(level)),
            visibility: wgpu::ShaderStages::COMPUTE,
            kind: ResourceKind::StorageImage {
                access: wgpu::StorageTextureAccess::WriteOnly,
                format: wgpu::TextureFormat::R32Float,
                dimension: wgpu::TextureViewDimension::D2,
            },
        })
        .collect();
    StageProgram::new(
        StageModule {
            source: StageSource::Wgsl(Arc::from(WGSL)),
            entry_point: "main".to_owned(),
            kind: StageKind::Compute,
        },
        StageDeclarations {
            resources: ResourceLayout::new(images),
            constants: Vec::new(),
        },
    )
    .unwrap()
}

/// The storage images per stage the scene asks an adapter offering `offered` for.
fn asked(offered: u32) -> wgpu::Limits {
    let available = wgpu::Limits {
        max_storage_textures_per_shader_stage: offered,
        ..wgpu::Limits::default()
    };
    requirements(&available, wgpu::Features::empty(), TimingMode::Disabled).required_limits
}

/// A runtime on a no-op device made with the scene's request to an adapter offering `offered`.
fn runtime(offered: u32) -> ProgramRuntime {
    let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor {
        required_limits: asked(offered),
        ..wgpu::DeviceDescriptor::default()
    });
    ProgramRuntime::new(
        device,
        SubmissionAuthority::new(queue),
        AllocationBudget(1 << 20),
    )
}

#[test]
fn the_scene_asks_for_the_adapters_storage_images_up_to_what_the_engine_binds() {
    for (offered, expected) in [(32, 16), (16, 16), (8, 8), (4, 4)] {
        assert_eq!(
            asked(offered).max_storage_textures_per_shader_stage,
            expected,
            "an adapter offering {offered}"
        );
    }
}

#[test]
fn a_kernel_of_seven_storage_images_prepares_where_the_adapter_offers_them() {
    let mut runtime = runtime(32);
    let program = runtime.compute_program(&stage()).unwrap();
    let texture = runtime.device().create_texture(&wgpu::TextureDescriptor {
        label: Some("seven mips"),
        size: wgpu::Extent3d {
            width: 64,
            height: 64,
            depth_or_array_layers: 1,
        },
        mip_level_count: LEVELS,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::R32Float,
        usage: wgpu::TextureUsages::STORAGE_BINDING,
        view_formats: &[],
    });
    let imports: Vec<_> = (0..LEVELS)
        .map(|level| {
            let view = texture.create_view(&wgpu::TextureViewDescriptor {
                base_mip_level: level,
                mip_level_count: Some(1),
                ..wgpu::TextureViewDescriptor::default()
            });
            Import::texture_range(
                semantic(level),
                texture.clone(),
                view,
                wgpu::TextureViewDimension::D2,
                TextureRange::single(level, 0, wgpu::TextureAspect::All),
            )
        })
        .collect();
    let dispatch = runtime
        .prepare_imported_dispatch(&ImportedDispatch {
            program: &program,
            bindings: &[],
            imports: &imports,
            groups: [8, 8, 1],
            owners: &[],
        })
        .unwrap();
    assert_eq!(dispatch.writes().len(), LEVELS as usize);
}

#[test]
fn an_adapter_that_offers_fewer_storage_images_refuses_the_kernel_by_name() {
    let runtime = runtime(4);
    let Err(error) = runtime.compute_program(&stage()) else {
        panic!("a device that allows 4 storage images made a program of 7");
    };
    assert_eq!(
        error,
        ProgramError::StorageImageLimit {
            stage: StageKind::Compute,
            count: LEVELS,
            limit: 4,
        }
    );
}

/// What a device made for an adapter offering `offered` storage images is granted through the
/// negotiation every scene device goes through: the scene's request combined with a caller that
/// asks for nothing beyond wgpu's defaults.
fn granted(offered: u32) -> Result<u32, worldrunner_render_gpu::device::DeviceError> {
    let available = wgpu::Limits {
        max_storage_textures_per_shader_stage: offered,
        ..wgpu::Limits::default()
    };
    let wanted = requirements(&available, wgpu::Features::empty(), TimingMode::Disabled)
        .union(&worldrunner_render_gpu::device::DeviceRequirements::default());
    worldrunner_render_gpu::device::negotiate(wgpu::Features::empty(), &available, &wanted)
        .map(|device| device.limits.max_storage_textures_per_shader_stage)
}

#[test]
fn a_device_is_granted_the_adapters_storage_images_up_to_sixteen_through_the_negotiation() {
    for (offered, expected) in [
        (4, 4),
        (5, 5),
        (8, 8),
        (12, 12),
        (16, 16),
        (17, 16),
        (1 << 20, 16),
    ] {
        assert_eq!(
            granted(offered).ok(),
            Some(expected),
            "an adapter offering {offered}"
        );
    }
}

#[test]
fn an_adapter_offering_fewer_storage_images_than_wgpus_default_is_refused_by_name() {
    for offered in [0, 2, 3] {
        let Err(worldrunner_render_gpu::device::DeviceError::LimitsExceeded { failures }) =
            granted(offered)
        else {
            panic!("a device was made for an adapter offering {offered} storage images");
        };
        assert!(
            failures
                .iter()
                .any(|failure| failure.contains("max_storage_textures_per_shader_stage")),
            "{failures:?}"
        );
    }
}
