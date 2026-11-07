//! Sets built once from an import table and bound by many draws, texture views of any
//! dimension, registered variants as programs, and the layouts programs share.

use std::sync::Arc;

use wgpu::ShaderStages;
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
    PreparedSet, ProgramDesc, ProgramRuntime, StageDesc, VariantDesc,
};
use worldrunner_render_program::variant::{FeatureId, FeatureSet, VariantError};

use crate::common::{
    CLEAR, FRAME, OWN, PRODUCT, added, assert_halves, fragment_uniform, frame_semantic, is_dropped,
    lent_group, lent_layout, lent_program, lent_set, no_cull, probe, render, signature, vertex,
};
use crate::support::Gpu;

fn own_semantic() -> EngineSemantic {
    EngineSemantic::named("own.tint")
}

/// What the fragment stage declares for `own` at set 1 when the engine supplies it.
fn own_declarations() -> StageDeclarations {
    StageDeclarations {
        resources: ResourceLayout::new(vec![fragment_uniform(
            1,
            BindingSource::Engine(own_semantic()),
        )]),
        constants: Vec::new(),
    }
}

/// What it declares when the program owns set 0 too.
fn both_declarations() -> StageDeclarations {
    StageDeclarations {
        resources: ResourceLayout::new(vec![
            fragment_uniform(0, BindingSource::Engine(frame_semantic())),
            fragment_uniform(1, BindingSource::Engine(own_semantic())),
        ]),
        constants: Vec::new(),
    }
}

fn program(runtime: &ProgramRuntime, vertex_entry: &str) -> GraphicsProgram {
    runtime
        .graphics_program(
            &vertex(vertex_entry),
            &crate::common::fragment("fs_scaled", both_declarations()),
        )
        .unwrap()
}

fn draw_with(
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

#[test]
fn a_prepared_set_serves_two_draws_and_builds_one_bind_group() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime();
    let layout = lent_layout(&gpu);
    let shared = [lent_set(&layout)];
    let (group, _frame) = lent_group(&gpu, &layout, FRAME);
    // Set 0 is the pass owner's. Set 1 is the one the runtime builds.
    let build =
        |entry| lent_program(&runtime, (entry, "fs_scaled"), own_declarations(), &shared).unwrap();
    let (left, right) = (build("vs_left"), build("vs_right"));
    let own = gpu.floats(wgpu::BufferUsages::UNIFORM, &OWN);
    let table = ImportTable::new().with(Import::buffer(own_semantic(), own));

    let mark = runtime.stats();
    let set = runtime.prepare_set(&left, 1, &table, Vec::new()).unwrap();
    // One group and one prepared set; no module, pipeline or draw.
    assert_eq!(added(mark, runtime.stats()), [0, 0, 1, 1, 0]);

    let mark = runtime.stats();
    let first = draw_with(&mut runtime, &left, (&[], &[&set])).unwrap();
    assert_eq!(added(mark, runtime.stats()), [2, 1, 0, 0, 1]);
    let mark = runtime.stats();
    let second = draw_with(&mut runtime, &right, (&[], &[&set])).unwrap();
    assert_eq!(added(mark, runtime.stats()), [1, 1, 0, 0, 1]);
    assert_eq!(runtime.stats().bind_groups_built, 1);
    assert_eq!(runtime.stats().sets_prepared, 1);

    // The draws hold the group, so the set itself can go.
    drop(set);
    let image = render(&gpu, &[(0, &group)], &[&first, &second]);
    assert_halves(&image, PRODUCT, PRODUCT);
}

#[test]
fn the_owners_of_a_prepared_set_live_as_long_as_a_draw_or_frame_that_uses_it() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime();
    let layout = lent_layout(&gpu);
    let left = lent_program(
        &runtime,
        ("vs_left", "fs_scaled"),
        own_declarations(),
        &[lent_set(&layout)],
    )
    .unwrap();
    let own = gpu.floats(wgpu::BufferUsages::UNIFORM, &OWN);
    let table = ImportTable::new().with(Import::buffer(own_semantic(), own));
    let (owner, dropped) = probe();
    let set = runtime
        .prepare_set(&left, 1, &table, vec![owner.clone()])
        .unwrap();
    let draw = draw_with(&mut runtime, &left, (&[], &[&set])).unwrap();
    drop(owner);
    drop(set);
    assert!(
        !is_dropped(&dropped),
        "the draw holds the owners of the set it binds"
    );

    let frame = gpu.authority.begin(&gpu.device).unwrap();
    draw.retain(&frame).unwrap();
    drop(draw);
    assert!(
        !is_dropped(&dropped),
        "a frame that retained the draw holds the set's owners"
    );
    frame.finish().cancel();
    assert!(is_dropped(&dropped), "a cancelled frame gives them back");
}

const ARRAYS_WGSL: &str = "
@group(1) @binding(0) var layers: texture_2d_array<f32>;
@group(1) @binding(1) var one: texture_2d<f32>;
@group(1) @binding(2) var grid: texture_2d<u32>;
@group(1) @binding(3) var depth: texture_depth_2d;
fn full(index: u32) -> vec4<f32> {
    var xs = array<f32, 6>(-1.0, 1.0, 1.0, -1.0, 1.0, -1.0);
    var ys = array<f32, 6>(-1.0, -1.0, 1.0, -1.0, 1.0, 1.0);
    return vec4<f32>(xs[index], ys[index], 0.0, 1.0);
}
@vertex fn vs_full(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    return full(index);
}
@fragment fn fs_layer0() -> @location(0) vec4<f32> {
    return textureLoad(layers, vec2<i32>(0, 0), 0, 0);
}
@fragment fn fs_layer1() -> @location(0) vec4<f32> {
    return textureLoad(layers, vec2<i32>(0, 0), 1, 0);
}
@fragment fn fs_one() -> @location(0) vec4<f32> {
    return textureLoad(one, vec2<i32>(0, 0), 0);
}
@fragment fn fs_depth() -> @location(0) vec4<f32> {
    let level = textureLoad(depth, vec2<i32>(0, 0), 0);
    return vec4<f32>(level, level, level, 1.0);
}
@fragment fn fs_grid(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let texel = textureLoad(grid, vec2<i32>(i32(position.x) / 2, 0), 0).r;
    return vec4<f32>(f32(texel) / 255.0, 0.0, 0.0, 1.0);
}";

fn texture_semantic(name: &str) -> EngineSemantic {
    EngineSemantic::named(format!("sets.{name}"))
}

/// A fragment-visible texture at set 1, binding `binding`, that the engine names `sets.<name>`.
fn texture_binding(
    (binding, name): (u32, &str),
    (dimension, sample): (wgpu::TextureViewDimension, wgpu::TextureSampleType),
) -> StageDeclarations {
    StageDeclarations {
        resources: ResourceLayout::new(vec![ResourceBinding {
            at: BindingAddress { set: 1, binding },
            source: BindingSource::Engine(texture_semantic(name)),
            visibility: ShaderStages::FRAGMENT,
            kind: ResourceKind::Texture {
                dimension,
                sample,
                multisampled: false,
            },
        }]),
        constants: Vec::new(),
    }
}

fn arrays_program(
    runtime: &ProgramRuntime,
    entry: &str,
    declarations: StageDeclarations,
) -> GraphicsProgram {
    let stage = |entry: &str, kind, declarations| {
        StageProgram::new(
            StageModule {
                source: StageSource::Wgsl(Arc::from(ARRAYS_WGSL)),
                entry_point: entry.to_owned(),
                kind,
            },
            declarations,
        )
        .unwrap()
    };
    runtime
        .graphics_program(
            &stage("vs_full", StageKind::Vertex, StageDeclarations::default()),
            &stage(entry, StageKind::Fragment, declarations),
        )
        .unwrap()
}

const FLOAT: wgpu::TextureSampleType = wgpu::TextureSampleType::Float { filterable: true };
const D2: wgpu::TextureViewDimension = wgpu::TextureViewDimension::D2;
const D2_ARRAY: wgpu::TextureViewDimension = wgpu::TextureViewDimension::D2Array;

/// The texel codes of a 1x1 layer each, so a layer is told apart by its colour.
const LAYERS: [[u8; 4]; 4] = [
    [64, 0, 0, 255],
    [0, 128, 0, 255],
    [0, 0, 192, 255],
    [128, 128, 0, 255],
];

/// The colour a shader reads from a texel of `Rgba8Unorm` codes.
fn color(codes: [u8; 4]) -> [f32; 4] {
    codes.map(|code| f32::from(code) / 255.0)
}

fn texture_of(
    gpu: &Gpu,
    (width, height, layers): (u32, u32, u32),
    format: wgpu::TextureFormat,
    texels: &[u8],
) -> wgpu::Texture {
    gpu.device.create_texture_with_data(
        gpu.authority.queue(),
        &wgpu::TextureDescriptor {
            label: Some("sets"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: layers,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        },
        wgpu::util::TextureDataOrder::LayerMajor,
        texels,
    )
}

fn view_of(
    texture: &wgpu::Texture,
    layers: (u32, u32),
    dimension: wgpu::TextureViewDimension,
) -> wgpu::TextureView {
    texture.create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(dimension),
        base_array_layer: layers.0,
        array_layer_count: Some(layers.1),
        ..wgpu::TextureViewDescriptor::default()
    })
}

/// Prepares set 1 of `program` from `imports` and draws it over the whole target.
fn full_screen(
    gpu: &Gpu,
    runtime: &mut ProgramRuntime,
    program: &GraphicsProgram,
    imports: Vec<Import>,
) -> Vec<u8> {
    let table = imports
        .into_iter()
        .fold(ImportTable::new(), ImportTable::with);
    let set = runtime.prepare_set(program, 1, &table, Vec::new()).unwrap();
    let draw = draw_with(runtime, program, (&[], &[&set])).unwrap();
    render(gpu, &[], &[&draw])
}

#[test]
fn a_texture_array_import_samples_the_layer_the_shader_names() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime();
    let texels = LAYERS.as_flattened();
    let texture = texture_of(&gpu, (1, 1, 4), wgpu::TextureFormat::Rgba8Unorm, texels);

    // A view of texture layers 1 to 3: the shader's layer 1 is the texture's layer 2.
    let program = arrays_program(
        &runtime,
        "fs_layer1",
        texture_binding((0, "layers"), (D2_ARRAY, FLOAT)),
    );
    let view = view_of(&texture, (1, 3), D2_ARRAY);
    let image = full_screen(
        &gpu,
        &mut runtime,
        &program,
        vec![Import::texture_array(
            texture_semantic("layers"),
            texture.clone(),
            view,
        )],
    );
    assert_halves(&image, color(LAYERS[2]), color(LAYERS[2]));

    // The same layer through a view that starts at it.
    let view = view_of(&texture, (2, 2), D2_ARRAY);
    let program = arrays_program(
        &runtime,
        "fs_layer0",
        texture_binding((0, "layers"), (D2_ARRAY, FLOAT)),
    );
    let import = Import::texture_array(texture_semantic("layers"), texture.clone(), view);
    let image = full_screen(&gpu, &mut runtime, &program, vec![import]);
    assert_halves(&image, color(LAYERS[2]), color(LAYERS[2]));

    // A texture of one layer can be an array too.
    let single = texture_of(
        &gpu,
        (1, 1, 1),
        wgpu::TextureFormat::Rgba8Unorm,
        LAYERS[3].as_slice(),
    );
    let view = view_of(&single, (0, 1), D2_ARRAY);
    let import = Import::texture_array(texture_semantic("layers"), single, view);
    let image = full_screen(&gpu, &mut runtime, &program, vec![import]);
    assert_halves(&image, color(LAYERS[3]), color(LAYERS[3]));

    // One layer of an array, viewed as a plain 2D texture.
    let program = arrays_program(&runtime, "fs_one", texture_binding((1, "one"), (D2, FLOAT)));
    let view = view_of(&texture, (1, 1), D2);
    let import = Import::texture_view(texture_semantic("one"), texture, view, D2);
    let image = full_screen(&gpu, &mut runtime, &program, vec![import]);
    assert_halves(&image, color(LAYERS[1]), color(LAYERS[1]));
}

#[test]
fn a_uint_texture_loads_exact_texels() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime();
    // Two texels. Columns 0 and 1 of the target read the first, columns 2 and 3 the second.
    let codes = [51_u8, 204];
    let texture = texture_of(&gpu, (2, 1, 1), wgpu::TextureFormat::R8Uint, &codes);
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let program = arrays_program(
        &runtime,
        "fs_grid",
        texture_binding((2, "grid"), (D2, wgpu::TextureSampleType::Uint)),
    );
    let import = Import::texture(texture_semantic("grid"), texture, view);
    let image = full_screen(&gpu, &mut runtime, &program, vec![import]);
    let red = |code: u8| [f32::from(code) / 255.0, 0.0, 0.0, 1.0];
    assert_halves(&image, red(codes[0]), red(codes[1]));
}

#[test]
fn a_depth_texture_loads_the_depth_a_pass_cleared_it_to() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime();
    let depth = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("depth"),
        size: wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Depth32Float,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let view = depth.create_view(&wgpu::TextureViewDescriptor::default());
    let mut frame = gpu.authority.begin(&gpu.device).unwrap();
    drop(frame.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("clear depth"),
        color_attachments: &[],
        depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
            view: &view,
            depth_ops: Some(wgpu::Operations {
                load: wgpu::LoadOp::Clear(0.25),
                store: wgpu::StoreOp::Store,
            }),
            stencil_ops: None,
        }),
        ..wgpu::RenderPassDescriptor::default()
    }));
    frame.finish().submit().unwrap();

    let program = arrays_program(
        &runtime,
        "fs_depth",
        texture_binding((3, "depth"), (D2, wgpu::TextureSampleType::Depth)),
    );
    let import = Import::texture(texture_semantic("depth"), depth, view);
    let image = full_screen(&gpu, &mut runtime, &program, vec![import]);
    assert_halves(&image, [0.25, 0.25, 0.25, 1.0], [0.25, 0.25, 0.25, 1.0]);
}

#[test]
fn a_view_that_disagrees_with_its_binding_is_refused_by_name() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime();
    let texels = LAYERS.as_flattened();
    let array = texture_of(&gpu, (1, 1, 4), wgpu::TextureFormat::Rgba8Unorm, texels);
    let flat = texture_of(
        &gpu,
        (1, 1, 1),
        wgpu::TextureFormat::Rgba8Unorm,
        &texels[..4],
    );
    let multisampled = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("multisampled"),
        size: wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 4,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let default_view =
        |texture: &wgpu::Texture| texture.create_view(&wgpu::TextureViewDescriptor::default());
    let array_slot = arrays_program(
        &runtime,
        "fs_layer0",
        texture_binding((0, "layers"), (D2_ARRAY, FLOAT)),
    );
    let flat_slot = arrays_program(&runtime, "fs_one", texture_binding((1, "one"), (D2, FLOAT)));
    let uint_slot = arrays_program(
        &runtime,
        "fs_grid",
        texture_binding((2, "grid"), (D2, wgpu::TextureSampleType::Uint)),
    );
    let layers = texture_semantic("layers");
    let one = texture_semantic("one");
    let grid = texture_semantic("grid");

    let attempts = [
        // A plain 2D view where an array is read.
        (
            &array_slot,
            Import::texture(layers.clone(), flat.clone(), default_view(&flat)),
        ),
        // An array view where a plain 2D texture is read.
        (
            &flat_slot,
            Import::texture_array(one.clone(), array.clone(), default_view(&array)),
        ),
        // A multisampled texture where a single-sample one is read.
        (
            &flat_slot,
            Import::texture(one, multisampled.clone(), default_view(&multisampled)),
        ),
        // A float texture where a uint one is read.
        (
            &uint_slot,
            Import::texture(grid, flat.clone(), default_view(&flat)),
        ),
    ];
    let mark = runtime.stats();
    for (program, import) in attempts {
        let semantic = import.semantic.clone();
        let table = ImportTable::new().with(import);
        let refused = runtime.prepare_set(program, 1, &table, Vec::new());
        assert!(
            matches!(
                refused.as_ref().err(),
                Some(ProgramError::WrongResource { set: 1, .. })
            ),
            "{semantic}: {refused:?}"
        );
    }
    // Nothing was built for a refused set.
    assert_eq!(added(mark, runtime.stats()), [0, 0, 0, 0, 0]);
}

#[test]
fn an_import_table_with_unused_entries_is_accepted_and_a_missing_one_is_named() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime();
    let layout = lent_layout(&gpu);
    let left = lent_program(
        &runtime,
        ("vs_left", "fs_scaled"),
        own_declarations(),
        &[lent_set(&layout)],
    )
    .unwrap();
    let buffer = |values: &[f32; 4]| gpu.floats(wgpu::BufferUsages::UNIFORM, values);
    // One table for every variant of a material: it lists what the variants use together. This
    // program uses `own.tint`, takes `scene.frame`'s sibling from the pass owner, and has no
    // use for the rest.
    let table = ImportTable::new()
        .with(Import::buffer(own_semantic(), buffer(&OWN)))
        .with(Import::buffer(frame_semantic(), buffer(&FRAME)))
        .with(Import::buffer(
            EngineSemantic::named("other.variant.only"),
            buffer(&FRAME),
        ));
    let mark = runtime.stats();
    runtime.prepare_set(&left, 1, &table, Vec::new()).unwrap();
    assert_eq!(added(mark, runtime.stats()), [0, 0, 1, 1, 0]);

    // A table without the used binding names it: the semantic, the set and the binding.
    let without = ImportTable::new().with(Import::buffer(frame_semantic(), buffer(&FRAME)));
    let mark = runtime.stats();
    let refused = runtime.prepare_set(&left, 1, &without, Vec::new());
    assert_eq!(
        refused.err(),
        Some(ProgramError::MissingImport {
            semantic: "own.tint".to_owned(),
            set: 1,
            binding: 0,
        })
    );
    assert_eq!(added(mark, runtime.stats()), [0, 0, 0, 0, 0]);
}

fn tinted() -> FeatureSet {
    FeatureSet::from([FeatureId::new("TINTED")])
}

fn register(runtime: &mut ProgramRuntime) -> worldrunner_render_program::runtime::ProgramHandle {
    let stage =
        |entry, kind, declarations| StageDesc::wgsl(crate::common::WGSL, entry, kind, declarations);
    let frame_only = StageDeclarations {
        resources: ResourceLayout::new(vec![fragment_uniform(
            0,
            BindingSource::Engine(frame_semantic()),
        )]),
        constants: Vec::new(),
    };
    runtime
        .register_program(ProgramDesc {
            label: "variants".to_owned(),
            state: no_cull(),
            variants: vec![
                VariantDesc {
                    features: FeatureSet::new(),
                    vertex: stage("vs_left", StageKind::Vertex, StageDeclarations::default()),
                    fragment: stage("fs_frame", StageKind::Fragment, frame_only),
                },
                VariantDesc {
                    features: tinted(),
                    vertex: stage("vs_right", StageKind::Vertex, StageDeclarations::default()),
                    fragment: stage("fs_scaled", StageKind::Fragment, both_declarations()),
                },
            ],
        })
        .unwrap()
}

#[test]
fn a_registered_variant_resolves_to_the_program_of_its_feature_set() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime();
    let handle = register(&mut runtime);
    let plain = runtime.variant_program(handle, &FeatureSet::new()).unwrap();
    let scaled = runtime.variant_program(handle, &tinted()).unwrap();
    let frame = gpu.floats(wgpu::BufferUsages::UNIFORM, &FRAME);
    let own = gpu.floats(wgpu::BufferUsages::UNIFORM, &OWN);
    let imports = [
        Import::buffer(frame_semantic(), frame),
        Import::buffer(own_semantic(), own),
    ];
    let table = imports
        .iter()
        .cloned()
        .fold(ImportTable::new(), ImportTable::with);

    // The variant no feature selects draws the left half the frame colour, and TINTED the right
    // half scaled by `own`. Each is drawn alone so that the other half stays clear.
    let set0 = runtime.prepare_set(&plain, 0, &table, Vec::new()).unwrap();
    let draw = draw_with(&mut runtime, &plain, (&[], &[&set0])).unwrap();
    assert_halves(&render(&gpu, &[], &[&draw]), FRAME, CLEAR);

    let sets = [
        runtime.prepare_set(&scaled, 0, &table, Vec::new()).unwrap(),
        runtime.prepare_set(&scaled, 1, &table, Vec::new()).unwrap(),
    ];
    let draw = draw_with(&mut runtime, &scaled, (&[], &[&sets[0], &sets[1]])).unwrap();
    assert_halves(&render(&gpu, &[], &[&draw]), CLEAR, PRODUCT);

    // Resolving builds nothing, and a set no variant has is named.
    let mark = runtime.stats();
    runtime.variant_program(handle, &tinted()).unwrap();
    assert_eq!(added(mark, runtime.stats()), [0, 0, 0, 0, 0]);
    let other = FeatureSet::from([FeatureId::new("NOT_A_FEATURE")]);
    assert_eq!(
        runtime.variant_program(handle, &other).err(),
        Some(ProgramError::Variant(VariantError::UnknownFeature {
            name: "NOT_A_FEATURE".to_owned(),
        }))
    );
    let unregistered = ProgramRuntime::new(
        gpu.device.clone(),
        gpu.authority.clone(),
        worldrunner_render_gpu::configuration::AllocationBudget(1 << 20),
    );
    assert_eq!(
        unregistered
            .variant_program(handle, &FeatureSet::new())
            .err(),
        Some(ProgramError::UnknownHandle { kind: "program" })
    );
}

#[test]
fn two_programs_with_equal_slots_share_a_set_layout_and_a_prepared_set() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime();
    let (left, right) = (program(&runtime, "vs_left"), program(&runtime, "vs_right"));
    let frame = gpu.floats(wgpu::BufferUsages::UNIFORM, &FRAME);
    let own = gpu.floats(wgpu::BufferUsages::UNIFORM, &OWN);
    let table = ImportTable::new()
        .with(Import::buffer(frame_semantic(), frame))
        .with(Import::buffer(own_semantic(), own));

    // Both sets are prepared from `left` and bound by `right`'s draw too: no draw builds a group.
    let sets = [
        runtime.prepare_set(&left, 0, &table, Vec::new()).unwrap(),
        runtime.prepare_set(&left, 1, &table, Vec::new()).unwrap(),
    ];
    let both = [&sets[0], &sets[1]];
    // Equal slots at a set are one layout object. A program whose slots differ has its own.
    assert_eq!(left.set_layout(1), right.set_layout(1));
    assert_eq!(left.set_layout(0), right.set_layout(0));
    assert_eq!(left.set_layout(2), None);
    let mark = runtime.stats();
    let first = draw_with(&mut runtime, &left, (&[], &both)).unwrap();
    let second = draw_with(&mut runtime, &right, (&[], &both)).unwrap();
    assert_eq!(added(mark, runtime.stats()), [3, 2, 0, 0, 2]);
    assert_eq!(runtime.stats().bind_groups_built, 2);
    assert_eq!(runtime.stats().sets_prepared, 2);
    assert_halves(&render(&gpu, &[], &[&first, &second]), PRODUCT, PRODUCT);

    let mut wide = both_declarations();
    wide.resources.bindings[1].visibility = ShaderStages::VERTEX_FRAGMENT;
    let wider = runtime
        .graphics_program(
            &vertex("vs_left"),
            &crate::common::fragment("fs_scaled", wide),
        )
        .unwrap();
    assert_ne!(left.set_layout(1), wider.set_layout(1));
    assert_eq!(left.set_layout(0), wider.set_layout(0));
}

#[test]
fn a_prepared_set_the_draw_cannot_use_is_refused_by_name() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime();
    let left = program(&runtime, "vs_left");
    let frame = gpu.floats(wgpu::BufferUsages::UNIFORM, &FRAME);
    let own = gpu.floats(wgpu::BufferUsages::UNIFORM, &OWN);
    let imports = [
        Import::buffer(frame_semantic(), frame),
        Import::buffer(own_semantic(), own),
    ];
    let table = imports
        .iter()
        .cloned()
        .fold(ImportTable::new(), ImportTable::with);
    let set0 = runtime.prepare_set(&left, 0, &table, Vec::new()).unwrap();
    let set1 = runtime.prepare_set(&left, 1, &table, Vec::new()).unwrap();
    let mut refusals = Vec::new();

    // A set given twice, and a set given alongside an import for one of its bindings.
    refusals.push(draw_with(
        &mut runtime,
        &left,
        (&[], &[&set0, &set1, &set1]),
    ));
    refusals.push(draw_with(&mut runtime, &left, (&imports, &[&set1])));
    // A set built for a layout this program does not have at that set.
    let mut wide = both_declarations();
    wide.resources.bindings[1].visibility = ShaderStages::VERTEX_FRAGMENT;
    let wider = runtime
        .graphics_program(
            &vertex("vs_left"),
            &crate::common::fragment("fs_scaled", wide),
        )
        .unwrap();
    refusals.push(draw_with(&mut runtime, &wider, (&imports[..1], &[&set1])));
    // A set with equal slots that another runtime built.
    let mut other = gpu.runtime();
    let theirs = program(&other, "vs_left");
    let foreign = other.prepare_set(&theirs, 1, &table, Vec::new()).unwrap();
    refusals.push(draw_with(&mut runtime, &left, (&imports[..1], &[&foreign])));
    // A set the pass owner binds.
    let layout = lent_layout(&gpu);
    let passbound = lent_program(
        &runtime,
        ("vs_left", "fs_scaled"),
        own_declarations(),
        &[lent_set(&layout)],
    )
    .unwrap();
    refusals.push(draw_with(&mut runtime, &passbound, (&[], &[&set0, &set1])));

    let errors: Vec<_> = refusals.into_iter().map(Result::err).collect();
    assert_eq!(
        errors,
        [
            Some(ProgramError::PreparedSetConflict { set: 1 }),
            Some(ProgramError::PreparedSetConflict { set: 1 }),
            Some(ProgramError::SetLayoutMismatch { set: 1 }),
            Some(ProgramError::SetLayoutMismatch { set: 1 }),
            Some(ProgramError::ExternalSetConflict { set: 0 }),
        ]
    );

    // Preparing a set the program cannot have is refused too.
    assert_eq!(
        runtime.prepare_set(&left, 2, &table, Vec::new()).err(),
        Some(ProgramError::UnknownSet { set: 2 })
    );
    assert_eq!(
        runtime.prepare_set(&passbound, 0, &table, Vec::new()).err(),
        Some(ProgramError::ExternalSetConflict { set: 0 })
    );
    // A binding the engine does not name has nothing to come from.
    let property = crate::common::plain_program(&runtime, "vs_left");
    assert_eq!(
        runtime.prepare_set(&property, 1, &table, Vec::new()).err(),
        Some(ProgramError::MissingBinding { set: 1, binding: 0 })
    );
}
