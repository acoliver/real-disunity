//! Comparison samplers in sets the runtime builds, used with depth textures.
//!
//! A comparison sampler compares the reference a shader passes with the depth it reads and
//! returns 1 or 0, so the expected image is a table of ones and zeros. The depth texture is a
//! `Depth16Unorm` one uploaded with codes `c` for the depths `c / 65535`.

use std::sync::Arc;

use wgpu::util::DeviceExt;
use worldrunner_render_program::binding::{
    BindingAddress, BindingSource, EngineSemantic, ResourceBinding, ResourceKind, ResourceLayout,
};
use worldrunner_render_program::error::ProgramError;
use worldrunner_render_program::module::{
    StageDeclarations, StageKind, StageModule, StageProgram, StageSource,
};
use worldrunner_render_program::runtime::{
    GraphicsProgram, Import, ImportTable, ImportedDraw, ImportedGeometry, PreparedProgramDraw,
    PreparedSet, ProgramRuntime,
};

use crate::common::{SIZE, assert_pixel, no_cull, render, signature};
use crate::support::Gpu;

const WGSL: &str = "
@group(0) @binding(0) var shadow: texture_depth_2d;
@group(0) @binding(1) var compare: sampler_comparison;
@vertex fn vs(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    var xs = array<f32, 6>(-1.0, 1.0, 1.0, -1.0, 1.0, -1.0);
    var ys = array<f32, 6>(-1.0, -1.0, 1.0, -1.0, 1.0, 1.0);
    return vec4<f32>(xs[index], ys[index], 0.0, 1.0);
}
@fragment fn fs(@builtin(position) at: vec4<f32>) -> @location(0) vec4<f32> {
    let lit = textureSampleCompare(shadow, compare, at.xy / 4.0, 0.5);
    return vec4<f32>(lit, lit, lit, 1.0);
}";

/// The depth of each column, as 16-bit codes: two below 0.5, a code one below 0.5 and the code
/// one above it. The codes 32767 and 32768 are the depths 0.49999237 and 0.50000763.
const CODES: [u16; 4] = [16384, 32767, 32768, 49151];

fn shadow() -> EngineSemantic {
    EngineSemantic::named("comparison.shadow")
}

fn sampler() -> EngineSemantic {
    EngineSemantic::named("comparison.sampler")
}

fn declarations(sampler_source: BindingSource) -> StageDeclarations {
    let at = |binding| BindingAddress { set: 0, binding };
    StageDeclarations {
        resources: ResourceLayout::new(vec![
            ResourceBinding {
                at: at(0),
                source: BindingSource::Engine(shadow()),
                visibility: wgpu::ShaderStages::FRAGMENT,
                kind: ResourceKind::Texture {
                    dimension: wgpu::TextureViewDimension::D2,
                    sample: wgpu::TextureSampleType::Depth,
                    multisampled: false,
                },
            },
            ResourceBinding {
                at: at(1),
                source: sampler_source,
                visibility: wgpu::ShaderStages::FRAGMENT,
                kind: ResourceKind::Sampler(wgpu::SamplerBindingType::Comparison),
            },
        ]),
        constants: Vec::new(),
    }
}

fn program(
    runtime: &ProgramRuntime,
    sampler_source: BindingSource,
) -> Result<GraphicsProgram, ProgramError> {
    let stage = |entry: &str, kind, declarations| {
        StageProgram::new(
            StageModule {
                source: StageSource::Wgsl(Arc::from(WGSL)),
                entry_point: entry.to_owned(),
                kind,
            },
            declarations,
        )
        .unwrap()
    };
    runtime.graphics_program(
        &stage("vs", StageKind::Vertex, StageDeclarations::default()),
        &stage("fs", StageKind::Fragment, declarations(sampler_source)),
    )
}

fn comparison_program(runtime: &ProgramRuntime) -> GraphicsProgram {
    program(runtime, BindingSource::Engine(sampler())).unwrap()
}

/// A depth texture whose column `x` holds `CODES[x]` in every row.
fn depth_texture(gpu: &Gpu) -> wgpu::Texture {
    let bytes: Vec<u8> = (0..SIZE)
        .flat_map(|_| CODES.iter().flat_map(|code| code.to_le_bytes()))
        .collect();
    gpu.device.create_texture_with_data(
        gpu.authority.queue(),
        &wgpu::TextureDescriptor {
            label: Some("shadow"),
            size: wgpu::Extent3d {
                width: SIZE,
                height: SIZE,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth16Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        },
        wgpu::util::TextureDataOrder::LayerMajor,
        &bytes,
    )
}

fn depth_import(gpu: &Gpu) -> Import {
    let texture = depth_texture(gpu);
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    Import::texture(shadow(), texture, view)
}

fn compare_sampler(gpu: &Gpu, function: wgpu::CompareFunction) -> wgpu::Sampler {
    gpu.device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("comparison"),
        compare: Some(function),
        ..wgpu::SamplerDescriptor::default()
    })
}

fn prepare(
    runtime: &mut ProgramRuntime,
    program: &GraphicsProgram,
    (imports, sets): (&[Import], &[&PreparedSet]),
) -> Result<PreparedProgramDraw, ProgramError> {
    runtime.prepare_imported_draw(
        &ImportedDraw {
            program,
            state: &no_cull(),
            vertex_inputs: &[],
            bindings: &[],
            imports,
            sets,
            geometry: &ImportedGeometry::Vertices { first: 0, count: 6 },
            instances: 0..1,
            owners: &[],
        },
        signature(),
    )
}

/// Asserts that column `x` of every row is white where `lit[x]` and black elsewhere.
fn assert_columns(image: &[u8], lit: [bool; 4]) {
    for y in 0..SIZE {
        for (x, lit) in (0..SIZE).zip(lit) {
            let value = if lit { 1.0 } else { 0.0 };
            assert_pixel(image, (x, y), [value, value, value, 1.0]);
        }
    }
}

#[test]
fn a_comparison_sampler_returns_zero_or_one_at_known_depths() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime();
    let program = comparison_program(&runtime);
    assert!(matches!(
        program.layout().slot(0, 1).unwrap().ty,
        wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison)
    ));

    // `LessEqual` lights a texel when the reference 0.5 is at most the stored depth: the two
    // texels at or above 0.5, including the one code above it, and not the one code below it.
    let less_equal = compare_sampler(&gpu, wgpu::CompareFunction::LessEqual);
    let draw = prepare(
        &mut runtime,
        &program,
        (
            &[
                depth_import(&gpu),
                Import::sampler(sampler(), less_equal.clone()),
            ],
            &[],
        ),
    )
    .unwrap();
    assert_columns(&render(&gpu, &[], &[&draw]), [false, false, true, true]);

    // `Greater` is the complement, and the sampler is a lent object: the same program draws
    // with it through a set prepared once.
    let greater = compare_sampler(&gpu, wgpu::CompareFunction::Greater);
    let table = ImportTable::new()
        .with(depth_import(&gpu))
        .with(Import::sampler(sampler(), greater));
    let set = runtime
        .prepare_set(&program, 0, &table, Vec::new())
        .unwrap();
    let draw = prepare(&mut runtime, &program, (&[], &[&set])).unwrap();
    assert_columns(&render(&gpu, &[], &[&draw]), [true, true, false, false]);

    // `Always` and `Never` are the extremes, whatever the depths.
    for (function, lit) in [
        (wgpu::CompareFunction::Always, true),
        (wgpu::CompareFunction::Never, false),
    ] {
        let draw = prepare(
            &mut runtime,
            &program,
            (
                &[
                    depth_import(&gpu),
                    Import::sampler(sampler(), compare_sampler(&gpu, function)),
                ],
                &[],
            ),
        )
        .unwrap();
        assert_columns(&render(&gpu, &[], &[&draw]), [lit; 4]);
    }
    // One program, one pipeline for all four draws.
    assert_eq!(runtime.stats().pipelines_built, 1);
}

#[test]
fn a_comparison_sampler_the_runtime_would_have_to_create_is_refused_by_name() {
    let gpu = Gpu::new();
    let runtime = gpu.runtime();
    for source in [
        BindingSource::Unnamed,
        BindingSource::Property(worldrunner_render_program::binding::PropertyId::new(
            "shadow",
        )),
    ] {
        assert_eq!(
            program(&runtime, source).err(),
            Some(ProgramError::UnsupportedSampler { set: 0, binding: 1 })
        );
    }
}

#[test]
fn a_plain_sampler_lent_to_a_comparison_slot_is_wgpus_to_refuse() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime();
    let program = comparison_program(&runtime);
    let plain = gpu
        .device
        .create_sampler(&wgpu::SamplerDescriptor::default());
    let imports = [depth_import(&gpu), Import::sampler(sampler(), plain)];
    let error = prepare(&mut runtime, &program, (&imports, &[])).unwrap_err();
    assert!(matches!(error, ProgramError::Gpu(_)), "{error:?}");
}
