//! Float textures that cannot be filtered, in sets the runtime builds.
//!
//! WGSL cannot say that a `texture_2d<f32>` is unfilterable, so the shader's own binding reads as
//! filterable. A declaration of `Float { filterable: false }` takes the slot instead, and a
//! texture of a format such as `R32Float` can be bound to it. The shader samples it with a
//! sampler declared non-filtering. The target is `R32Float` too, so every texel read back is the
//! exact `f32` the shader returned.

use std::sync::Arc;

use wgpu::util::DeviceExt;
use worldrunner_render_program::binding::{
    BindingAddress, BindingSource, EngineSemantic, ResourceBinding, ResourceKind, ResourceLayout,
};
use worldrunner_render_program::error::ProgramError;
use worldrunner_render_program::layout::LayoutError;
use worldrunner_render_program::module::{
    StageDeclarations, StageKind, StageModule, StageProgram, StageSource,
};
use worldrunner_render_program::runtime::{
    GraphicsProgram, Import, ImportedDraw, ImportedGeometry, PreparedProgramDraw, ProgramRuntime,
    TargetSignature,
};

use crate::common::{SIZE, no_cull};
use crate::support::{Attachment, Gpu};

const WGSL: &str = "
@group(0) @binding(0) var ambient: texture_2d<f32>;
@group(0) @binding(1) var point: sampler;
@vertex fn vs(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    var xs = array<f32, 6>(-1.0, 1.0, 1.0, -1.0, 1.0, -1.0);
    var ys = array<f32, 6>(-1.0, -1.0, 1.0, -1.0, 1.0, 1.0);
    return vec4<f32>(xs[index], ys[index], 0.0, 1.0);
}
@fragment fn fs_sample(@builtin(position) at: vec4<f32>) -> @location(0) vec4<f32> {
    return vec4<f32>(textureSample(ambient, point, at.xy / 4.0).r, 0.0, 0.0, 1.0);
}
@fragment fn fs_load(@builtin(position) at: vec4<f32>) -> @location(0) vec4<f32> {
    return vec4<f32>(textureLoad(ambient, vec2<i32>(at.xy), 0).r, 0.0, 0.0, 1.0);
}";

const UNFILTERABLE: wgpu::TextureSampleType = wgpu::TextureSampleType::Float { filterable: false };
const FILTERABLE: wgpu::TextureSampleType = wgpu::TextureSampleType::Float { filterable: true };
const R32: wgpu::TextureFormat = wgpu::TextureFormat::R32Float;

fn ambient() -> EngineSemantic {
    EngineSemantic::named("unfilterable.ambient")
}

fn stage(entry: &str, kind: StageKind, resources: Vec<ResourceBinding>) -> StageProgram {
    StageProgram::new(
        StageModule {
            source: StageSource::Wgsl(Arc::from(WGSL)),
            entry_point: entry.to_owned(),
            kind,
        },
        StageDeclarations {
            resources: ResourceLayout::new(resources),
            constants: Vec::new(),
        },
    )
    .unwrap()
}

/// The texture of set 0 as `sample` and, when the entry point samples, the sampler as `sampler`.
fn declarations(
    sample: wgpu::TextureSampleType,
    sampler: Option<wgpu::SamplerBindingType>,
) -> Vec<ResourceBinding> {
    let at = |binding| BindingAddress { set: 0, binding };
    let mut declared = vec![ResourceBinding {
        at: at(0),
        source: BindingSource::Engine(ambient()),
        visibility: wgpu::ShaderStages::FRAGMENT,
        kind: ResourceKind::Texture {
            dimension: wgpu::TextureViewDimension::D2,
            sample,
            multisampled: false,
        },
    }];
    declared.extend(sampler.map(|kind| ResourceBinding {
        at: at(1),
        source: BindingSource::Unnamed,
        visibility: wgpu::ShaderStages::FRAGMENT,
        kind: ResourceKind::Sampler(kind),
    }));
    declared
}

fn program(
    runtime: &ProgramRuntime,
    entry: &str,
    resources: Vec<ResourceBinding>,
) -> Result<GraphicsProgram, ProgramError> {
    runtime.graphics_program(
        &stage("vs", StageKind::Vertex, Vec::new()),
        &stage(entry, StageKind::Fragment, resources),
    )
}

/// The texel at column `x` and row `y` of the 4x4 test texture: distinct multiples of 1/64, which
/// every float format used here holds exactly.
fn texel(x: u32, y: u32) -> f32 {
    f32::from(u8::try_from(y * SIZE + x + 1).unwrap()) / 64.0
}

fn texture(gpu: &Gpu, format: wgpu::TextureFormat) -> wgpu::Texture {
    let values = (0..SIZE).flat_map(|y| (0..SIZE).map(move |x| texel(x, y)));
    let bytes: Vec<u8> = match format {
        R32 => values.flat_map(f32::to_le_bytes).collect(),
        // Half-precision codes of k/64 for k in 1..=16: exponent bias 15, ten fraction bits.
        wgpu::TextureFormat::R16Float => values
            .flat_map(|value| half_bits(value).to_le_bytes())
            .collect(),
        other => panic!("{other:?} is not built here"),
    };
    gpu.device.create_texture_with_data(
        gpu.authority.queue(),
        &wgpu::TextureDescriptor {
            label: Some("ambient"),
            size: wgpu::Extent3d {
                width: SIZE,
                height: SIZE,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        },
        wgpu::util::TextureDataOrder::LayerMajor,
        &bytes,
    )
}

/// The half-precision code of a positive multiple of 1/64 up to 1.
fn half_bits(value: f32) -> u16 {
    let bits = value.to_bits();
    let exponent = u16::try_from((bits >> 23) & 0xff).unwrap() + 15 - 127;
    let fraction = u16::try_from((bits >> 13) & 0x3ff).unwrap();
    assert_eq!(bits & 0x1fff, 0, "{value} is not a half-precision value");
    (exponent << 10) | fraction
}

fn imports(gpu: &Gpu, format: wgpu::TextureFormat) -> Vec<Import> {
    let texture = texture(gpu, format);
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    vec![Import::texture(ambient(), texture, view)]
}

fn prepare(
    runtime: &mut ProgramRuntime,
    program: &GraphicsProgram,
    imports: &[Import],
) -> Result<PreparedProgramDraw, ProgramError> {
    runtime.prepare_imported_draw(
        &ImportedDraw {
            program,
            state: &no_cull(),
            vertex_inputs: &[],
            bindings: &[],
            imports,
            sets: &[],
            geometry: &ImportedGeometry::Vertices { first: 0, count: 6 },
            instances: 0..1,
            owners: &[],
        },
        TargetSignature::attachments(&[R32], None, 1).unwrap(),
    )
}

/// The red channel of every texel of one pass of `draw` into a cleared `R32Float` target, row by
/// row.
fn drawn(gpu: &Gpu, draw: &PreparedProgramDraw) -> Vec<f32> {
    let target = gpu.texture(SIZE, R32, 1);
    let view = target.create_view(&wgpu::TextureViewDescriptor::default());
    gpu.pass(
        &[Attachment {
            view: &view,
            resolve: None,
            clear: wgpu::Color::TRANSPARENT,
        }],
        &[draw],
    );
    gpu.read(&target)
        .chunks_exact(4)
        .map(|bytes| f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
        .collect()
}

fn expected() -> Vec<f32> {
    (0..SIZE)
        .flat_map(|y| (0..SIZE).map(move |x| texel(x, y)))
        .collect()
}

#[test]
fn an_r32float_texture_sampled_without_filtering_returns_exact_values() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime();
    let sampled = program(
        &runtime,
        "fs_sample",
        declarations(UNFILTERABLE, Some(wgpu::SamplerBindingType::NonFiltering)),
    )
    .unwrap();
    // The slot takes the declaration's sample type, not the filterable one WGSL reads as.
    let slot = sampled.layout().slot(0, 0).unwrap();
    assert!(matches!(
        slot.ty,
        wgpu::BindingType::Texture { sample_type, .. } if sample_type == UNFILTERABLE
    ));
    assert!(matches!(
        sampled.layout().slot(0, 1).unwrap().ty,
        wgpu::BindingType::Sampler(wgpu::SamplerBindingType::NonFiltering)
    ));

    // `R32Float` is not filterable without a device feature, and the runtime's non-filtering
    // sampler fills the sampler binding.
    let draw = prepare(&mut runtime, &sampled, &imports(&gpu, R32)).unwrap();
    assert_eq!(drawn(&gpu, &draw), expected());

    // The same draw with a sampler the owner lends, which `point` reads like any other.
    let nearest = gpu
        .device
        .create_sampler(&wgpu::SamplerDescriptor::default());
    let lent = program(
        &runtime,
        "fs_sample",
        vec![
            declarations(UNFILTERABLE, None).remove(0),
            ResourceBinding {
                source: BindingSource::Engine(EngineSemantic::named("unfilterable.point")),
                ..declarations(UNFILTERABLE, Some(wgpu::SamplerBindingType::NonFiltering)).remove(1)
            },
        ],
    )
    .unwrap();
    let mut both = imports(&gpu, R32);
    both.push(Import::sampler(
        EngineSemantic::named("unfilterable.point"),
        nearest,
    ));
    let draw = prepare(&mut runtime, &lent, &both).unwrap();
    assert_eq!(drawn(&gpu, &draw), expected());

    // A filterable format suits the same slot: it is only a promise not to filter.
    let draw = prepare(
        &mut runtime,
        &sampled,
        &imports(&gpu, wgpu::TextureFormat::R16Float),
    )
    .unwrap();
    assert_eq!(drawn(&gpu, &draw), expected());
}

#[test]
fn a_filtering_sampler_with_an_unfilterable_texture_is_refused_by_name() {
    let gpu = Gpu::new();
    let runtime = gpu.runtime();
    // WGSL reads its sampler as filtering, and so does a declaration that says so.
    let refused = program(
        &runtime,
        "fs_sample",
        declarations(UNFILTERABLE, Some(wgpu::SamplerBindingType::Filtering)),
    );
    assert_eq!(
        refused.err(),
        Some(ProgramError::Layout(LayoutError::FilteringSampler {
            texture_set: 0,
            texture: 0,
            sampler_set: 0,
            sampler: 1,
        }))
    );
    // A shader that loads texels pairs the texture with no sampler, so nothing is refused.
    assert!(
        program(
            &runtime,
            "fs_load",
            declarations(UNFILTERABLE, Some(wgpu::SamplerBindingType::Filtering)),
        )
        .is_ok()
    );
    // A filterable texture takes a filtering sampler, as it always has.
    assert!(
        program(
            &runtime,
            "fs_sample",
            declarations(FILTERABLE, Some(wgpu::SamplerBindingType::Filtering)),
        )
        .is_ok()
    );
}

#[test]
fn a_declaration_narrows_only_the_filterable_flag_of_a_float_texture() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime();
    let filterable = program(&runtime, "fs_load", declarations(FILTERABLE, None)).unwrap();
    // The import is the binding's `texture_2d<f32>` only if its format can be filtered.
    let error = prepare(&mut runtime, &filterable, &imports(&gpu, R32)).unwrap_err();
    assert!(
        matches!(
            &error,
            ProgramError::WrongResource {
                set: 0,
                binding: 0,
                ..
            }
        ),
        "{error:?}"
    );
    assert!(
        prepare(
            &mut runtime,
            &filterable,
            &imports(&gpu, wgpu::TextureFormat::R16Float)
        )
        .is_ok()
    );

    // Unfilterable is the one sample type the shader's float binding can be declared as besides
    // its own: an unsigned or depth declaration is another class of texture.
    for sample in [
        wgpu::TextureSampleType::Uint,
        wgpu::TextureSampleType::Depth,
    ] {
        let mismatched = program(&runtime, "fs_load", declarations(sample, None));
        assert!(
            matches!(
                mismatched.err(),
                Some(ProgramError::Layout(LayoutError::Mismatch {
                    set: 0,
                    binding: 0,
                    ..
                }))
            ),
            "{sample:?}"
        );
    }
}

#[test]
fn a_filtering_sampler_lent_to_a_non_filtering_slot_is_wgpus_to_refuse() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime();
    let lent = program(
        &runtime,
        "fs_sample",
        vec![
            declarations(UNFILTERABLE, None).remove(0),
            ResourceBinding {
                source: BindingSource::Engine(EngineSemantic::named("unfilterable.point")),
                ..declarations(UNFILTERABLE, Some(wgpu::SamplerBindingType::NonFiltering)).remove(1)
            },
        ],
    )
    .unwrap();
    let linear = gpu.device.create_sampler(&wgpu::SamplerDescriptor {
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..wgpu::SamplerDescriptor::default()
    });
    let mut both = imports(&gpu, R32);
    both.push(Import::sampler(
        EngineSemantic::named("unfilterable.point"),
        linear,
    ));
    let error = prepare(&mut runtime, &lent, &both).unwrap_err();
    assert!(matches!(error, ProgramError::Gpu(_)), "{error:?}");
}
