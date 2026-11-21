//! A set written once: the engine declares the bindings of a set it binds itself, builds the
//! `wgpu::BindGroupLayout` from the declaration, builds its groups against that layout, and lends
//! the same declaration to the programs that read the set.
//!
//! The set holds every kind of binding the declarations can say: a uniform buffer with a minimum
//! size, a read-only storage buffer, an unfilterable `R32Float` texture with a non-filtering
//! sampler, and a depth texture with a comparison sampler. The target is `Rgba32Float`, so each
//! channel read back is the exact `f32` one binding contributed.

use std::sync::Arc;

use wgpu::ShaderStages;
use wgpu::util::DeviceExt;
use worldrunner_render_program::binding::{
    BindingAddress, BindingSource, EngineSemantic, ResourceBinding, ResourceKind, ResourceLayout,
    StorageAccess,
};
use worldrunner_render_program::error::ProgramError;
use worldrunner_render_program::layout::LayoutError;
use worldrunner_render_program::module::{
    StageDeclarations, StageKind, StageModule, StageProgram, StageSource,
};
use worldrunner_render_program::runtime::{
    Import, ImportTable, ImportedDraw, ImportedGeometry, PreparedProgramDraw, ProgramRuntime,
    TargetSignature,
};

use crate::common::{SIZE, no_cull};
use crate::support::{Attachment, Gpu};

const WGSL: &str = "
struct Frame { tint: vec4<f32> }
@group(0) @binding(0) var<uniform> frame: Frame;
@group(0) @binding(1) var<storage, read> table: array<vec4<f32>>;
@group(0) @binding(2) var ambient: texture_2d<f32>;
@group(0) @binding(3) var point: sampler;
@group(0) @binding(4) var shadow: texture_depth_2d;
@group(0) @binding(5) var compare: sampler_comparison;
@vertex fn vs(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    var xs = array<f32, 6>(-1.0, 1.0, 1.0, -1.0, 1.0, -1.0);
    var ys = array<f32, 6>(-1.0, -1.0, 1.0, -1.0, 1.0, 1.0);
    return vec4<f32>(xs[index], ys[index], 0.0, 1.0);
}
@fragment fn fs(@builtin(position) at: vec4<f32>) -> @location(0) vec4<f32> {
    let uv = at.xy / 4.0;
    return vec4<f32>(
        textureSample(ambient, point, uv).r,
        textureSampleCompare(shadow, compare, uv, 0.5),
        frame.tint.x,
        table[1].x,
    );
}";

const RGBA32: wgpu::TextureFormat = wgpu::TextureFormat::Rgba32Float;
/// The values each binding contributes to the channel it is read into.
const AMBIENT: f32 = 0.0625;
const TINT: f32 = 2.0;
const TABLE: f32 = 8.0;
/// A depth above the reference 0.5 of the shader, which a `LessEqual` comparison lights.
const DEPTH_CODE: u16 = 49151;

fn at(binding: u32) -> BindingAddress {
    BindingAddress { set: 0, binding }
}

fn declare(binding: u32, name: &str, kind: ResourceKind) -> ResourceBinding {
    ResourceBinding {
        at: at(binding),
        source: BindingSource::Engine(EngineSemantic::named(name)),
        visibility: ShaderStages::FRAGMENT,
        kind,
    }
}

/// Set 0 as the engine declares it, once.
fn frame_set() -> ResourceLayout {
    let texture = |sample| ResourceKind::Texture {
        dimension: wgpu::TextureViewDimension::D2,
        sample,
        multisampled: false,
    };
    ResourceLayout::new(vec![
        declare(0, "frame.tint", ResourceKind::Uniform { min_size: 16 }),
        declare(
            1,
            "frame.table",
            ResourceKind::Storage {
                access: StorageAccess::ReadOnly,
                min_size: 32,
            },
        ),
        declare(
            2,
            "frame.ambient",
            texture(wgpu::TextureSampleType::Float { filterable: false }),
        ),
        declare(
            3,
            "frame.point",
            ResourceKind::Sampler(wgpu::SamplerBindingType::NonFiltering),
        ),
        declare(4, "frame.shadow", texture(wgpu::TextureSampleType::Depth)),
        declare(
            5,
            "frame.compare",
            ResourceKind::Sampler(wgpu::SamplerBindingType::Comparison),
        ),
    ])
}

/// What an engine owns for the set: one resource for each binding.
struct Frame {
    tint: wgpu::Buffer,
    table: wgpu::Buffer,
    ambient: wgpu::TextureView,
    point: wgpu::Sampler,
    shadow: wgpu::TextureView,
    compare: wgpu::Sampler,
    _textures: [wgpu::Texture; 2],
}

fn texture_with(gpu: &Gpu, format: wgpu::TextureFormat, texel: &[u8]) -> wgpu::Texture {
    gpu.device.create_texture_with_data(
        gpu.authority.queue(),
        &wgpu::TextureDescriptor {
            label: Some("frame"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
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
        texel,
    )
}

fn frame(gpu: &Gpu) -> Frame {
    let ambient = texture_with(gpu, wgpu::TextureFormat::R32Float, &AMBIENT.to_le_bytes());
    let shadow = texture_with(
        gpu,
        wgpu::TextureFormat::Depth16Unorm,
        &DEPTH_CODE.to_le_bytes(),
    );
    let view =
        |texture: &wgpu::Texture| texture.create_view(&wgpu::TextureViewDescriptor::default());
    let table: Vec<f32> = [[0.0; 4], [TABLE, 0.0, 0.0, 0.0]].concat();
    Frame {
        tint: gpu.floats(wgpu::BufferUsages::UNIFORM, &[TINT, 0.0, 0.0, 0.0]),
        table: gpu.floats(wgpu::BufferUsages::STORAGE, &table),
        ambient: view(&ambient),
        point: gpu
            .device
            .create_sampler(&wgpu::SamplerDescriptor::default()),
        shadow: view(&shadow),
        compare: gpu.device.create_sampler(&wgpu::SamplerDescriptor {
            compare: Some(wgpu::CompareFunction::LessEqual),
            ..wgpu::SamplerDescriptor::default()
        }),
        _textures: [ambient, shadow],
    }
}

/// The group an engine builds for its set against `layout`.
fn group(gpu: &Gpu, layout: &wgpu::BindGroupLayout, frame: &Frame) -> wgpu::BindGroup {
    let entry = |binding, resource| wgpu::BindGroupEntry { binding, resource };
    gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("frame"),
        layout,
        entries: &[
            entry(0, frame.tint.as_entire_binding()),
            entry(1, frame.table.as_entire_binding()),
            entry(2, wgpu::BindingResource::TextureView(&frame.ambient)),
            entry(3, wgpu::BindingResource::Sampler(&frame.point)),
            entry(4, wgpu::BindingResource::TextureView(&frame.shadow)),
            entry(5, wgpu::BindingResource::Sampler(&frame.compare)),
        ],
    })
}

/// The validation error, if any, that `work` makes wgpu report.
fn validation_error<T>(gpu: &Gpu, work: impl FnOnce() -> T) -> Option<String> {
    let scope = gpu.device.push_error_scope(wgpu::ErrorFilter::Validation);
    let _made = work();
    pollster::block_on(scope.pop()).map(|error| error.to_string())
}

fn stage(entry: &str, kind: StageKind) -> StageProgram {
    StageProgram::new(
        StageModule {
            source: StageSource::Wgsl(Arc::from(WGSL)),
            entry_point: entry.to_owned(),
            kind,
        },
        StageDeclarations::default(),
    )
    .unwrap()
}

fn draw_with(
    runtime: &mut ProgramRuntime,
    lent: &[worldrunner_render_program::runtime::ExternalSet],
) -> PreparedProgramDraw {
    let program = runtime
        .graphics_program_in(
            &stage("vs", StageKind::Vertex),
            &stage("fs", StageKind::Fragment),
            lent,
        )
        .unwrap();
    runtime
        .prepare_imported_draw(
            &ImportedDraw {
                program: &program,
                state: &no_cull(),
                vertex_inputs: &[],
                bindings: &[],
                imports: &[],
                sets: &[],
                topology: wgpu::PrimitiveTopology::TriangleList,
                geometry: &ImportedGeometry::Vertices { first: 0, count: 6 },
                instances: 0..1,
                owners: &[],
            },
            TargetSignature::attachments(&[RGBA32], None, 1).unwrap(),
        )
        .unwrap()
}

#[test]
fn a_layout_built_from_declarations_accepts_the_groups_an_engine_builds_against_it() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime();
    let declared = frame_set();
    let layout = runtime.set_layout(&declared).unwrap();
    let engine = frame(&gpu);

    // The engine's group is valid against the layout built from the declaration.
    assert_eq!(
        validation_error(&gpu, || group(&gpu, &layout, &engine)),
        None
    );

    // The layout is not vacuous: it holds the uniform buffer to its declared minimum size, and a
    // plain sampler is not a comparison sampler.
    let short = gpu.floats(wgpu::BufferUsages::UNIFORM, &[TINT, 0.0]);
    let too_small = Frame {
        tint: short,
        ..frame(&gpu)
    };
    assert!(validation_error(&gpu, || group(&gpu, &layout, &too_small)).is_some());
    let plain = Frame {
        compare: gpu
            .device
            .create_sampler(&wgpu::SamplerDescriptor::default()),
        ..frame(&gpu)
    };
    assert!(validation_error(&gpu, || group(&gpu, &layout, &plain)).is_some());

    // A program lends the same declaration with its own layout, and the pipeline accepts a group
    // built against that layout: the channels read back are what each binding holds.
    let lent = runtime.external_set(declared).unwrap();
    assert_eq!(lent.set, 0);
    let built = group(&gpu, &lent.layout, &engine);
    let draw = draw_with(&mut runtime, &[lent]);
    assert_eq!(draw.external_sets(), [0]);
    let target = gpu.texture(SIZE, RGBA32, 1);
    let view = target.create_view(&wgpu::TextureViewDescriptor::default());
    gpu.pass_with(
        &[Attachment {
            view: &view,
            resolve: None,
            clear: wgpu::Color::TRANSPARENT,
        }],
        &[(0, &built)],
        &[&draw],
    );
    let texels: Vec<f32> = gpu
        .read(&target)
        .chunks_exact(4)
        .map(|bytes| f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
        .collect();
    assert_eq!(texels.len(), (SIZE * SIZE * 4) as usize);
    for texel in texels.chunks_exact(4) {
        assert_eq!(texel, [AMBIENT, 1.0, TINT, TABLE]);
    }
}

#[test]
fn a_declaration_that_is_not_one_set_is_refused_by_name() {
    let gpu = Gpu::new();
    let runtime = gpu.runtime();
    let uniform = || ResourceKind::Uniform { min_size: 16 };

    let mut across = frame_set();
    across.bindings[1].at = BindingAddress { set: 1, binding: 1 };
    assert!(matches!(
        runtime.set_layout(&across).err(),
        Some(ProgramError::Layout(LayoutError::Declaration { .. }))
    ));
    let mut twice = frame_set();
    twice.bindings.push(declare(0, "frame.again", uniform()));
    assert_eq!(
        runtime.set_layout(&twice).err(),
        Some(ProgramError::Layout(LayoutError::DuplicateDeclaration {
            set: 0,
            binding: 0
        }))
    );
    assert!(matches!(
        runtime.set_layout(&ResourceLayout::default()).err(),
        Some(ProgramError::Layout(LayoutError::Declaration { .. }))
    ));
    // The set an engine lends is the set it declares.
    let mut elsewhere = ResourceLayout::default();
    elsewhere.bindings.push(ResourceBinding {
        at: BindingAddress { set: 2, binding: 0 },
        ..declare(0, "frame.tint", uniform())
    });
    assert_eq!(runtime.external_set(elsewhere).unwrap().set, 2);
}

/// The imports an engine lends for its set: the same resources as [`group`] builds by hand.
fn imports(frame: &Frame) -> ImportTable {
    let named = |name: &str| EngineSemantic::named(name);
    let texture = |name: &str, view: &wgpu::TextureView| {
        Import::texture(named(name), view.texture().clone(), view.clone())
    };
    ImportTable::new()
        .with(Import::buffer(named("frame.tint"), frame.tint.clone()))
        .with(Import::buffer(named("frame.table"), frame.table.clone()))
        .with(texture("frame.ambient", &frame.ambient))
        .with(Import::sampler(named("frame.point"), frame.point.clone()))
        .with(texture("frame.shadow", &frame.shadow))
        .with(Import::sampler(
            named("frame.compare"),
            frame.compare.clone(),
        ))
}

#[test]
fn the_runtime_builds_an_external_group_from_imports_and_the_draw_reads_it() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime();
    let engine = frame(&gpu);
    let lent = runtime.external_set(frame_set()).unwrap();
    let group = runtime
        .prepare_external_set(&lent, &imports(&engine))
        .unwrap();
    assert_eq!(group.set(), 0);
    let draw = draw_with(&mut runtime, std::slice::from_ref(&lent));
    let target = gpu.texture(SIZE, RGBA32, 1);
    let view = target.create_view(&wgpu::TextureViewDescriptor::default());
    gpu.pass_binding(
        &[Attachment {
            view: &view,
            resolve: None,
            clear: wgpu::Color::TRANSPARENT,
        }],
        &|pass| group.bind(pass),
        &[&draw],
    );
    let texels: Vec<f32> = gpu
        .read(&target)
        .chunks_exact(4)
        .map(|bytes| f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
        .collect();
    for texel in texels.chunks_exact(4) {
        assert_eq!(texel, [AMBIENT, 1.0, TINT, TABLE]);
    }
}

#[test]
fn an_external_group_is_refused_by_name_for_a_missing_or_unsuitable_import() {
    let gpu = Gpu::new();
    let runtime = gpu.runtime();
    let engine = frame(&gpu);
    let lent = runtime.external_set(frame_set()).unwrap();
    let missing = ImportTable::new().with(Import::buffer(
        EngineSemantic::named("frame.tint"),
        engine.tint.clone(),
    ));
    assert_eq!(
        runtime.prepare_external_set(&lent, &missing).err(),
        Some(ProgramError::MissingImport {
            semantic: "frame.table".to_owned(),
            set: 0,
            binding: 1,
        })
    );
    // A buffer where the set declares a texture.
    let wrong = imports(&engine).with(Import::buffer(
        EngineSemantic::named("frame.ambient"),
        engine.tint.clone(),
    ));
    assert!(matches!(
        runtime.prepare_external_set(&lent, &wrong).err(),
        Some(ProgramError::WrongResource {
            set: 0,
            binding: 2,
            ..
        })
    ));
    // The uniform held to its declared minimum size.
    let short = imports(&engine).with(Import::buffer(
        EngineSemantic::named("frame.tint"),
        gpu.floats(wgpu::BufferUsages::UNIFORM, &[TINT, 0.0]),
    ));
    assert!(matches!(
        runtime.prepare_external_set(&lent, &short).err(),
        Some(ProgramError::BufferTooSmall {
            set: 0,
            binding: 0,
            ..
        })
    ));
}

#[test]
fn a_depth_texture_suits_an_unfilterable_float_declaration() {
    let gpu = Gpu::new();
    let runtime = gpu.runtime();
    let engine = frame(&gpu);
    let lent = runtime.external_set(frame_set()).unwrap();
    // The depth texture at the unfilterable `R32Float` binding: wgpu reads it as a float.
    let depth = imports(&engine).with(Import::texture(
        EngineSemantic::named("frame.ambient"),
        engine.shadow.texture().clone(),
        engine.shadow.clone(),
    ));
    runtime.prepare_external_set(&lent, &depth).unwrap();
    // A filterable float declaration still refuses it.
    let mut filterable = frame_set();
    filterable.bindings[2].kind = ResourceKind::Texture {
        dimension: wgpu::TextureViewDimension::D2,
        sample: wgpu::TextureSampleType::Float { filterable: true },
        multisampled: false,
    };
    let lent = runtime.external_set(filterable).unwrap();
    assert!(matches!(
        runtime.prepare_external_set(&lent, &depth).err(),
        Some(ProgramError::WrongResource {
            set: 0,
            binding: 2,
            ..
        })
    ));
}
