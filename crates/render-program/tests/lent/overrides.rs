//! Pipeline override constants: the value a stage draws, the pipelines and modules that result
//! and the refusals that name a constant.

use std::sync::Arc;

use worldrunner_render_program::binding::{
    BindingAddress, BindingSource, PropertyId, ResourceBinding, ResourceKind, ResourceLayout,
    StorageAccess,
};
use worldrunner_render_program::error::ProgramError;
use worldrunner_render_program::module::{
    OverrideError, StageDeclarations, StageKind, StageModule, StageProgram, StageSource,
};
use worldrunner_render_program::runtime::{
    BoundResource, DispatchRequest, GraphicsProgram, ProgramDesc, ProgramRuntime, StageDesc,
};
use worldrunner_render_program::state::RenderState;
use worldrunner_render_program::variant::FeatureSet;

use crate::common::{CLEAR, Inputs, added, assert_halves, prepare, render};
use crate::support::Gpu;

/// `fs` writes `(gain, enabled, count / 4, 1)`, and the three overrides are of the three scalar
/// kinds a pipeline constant can be. `vs_left` fills columns 0 and 1 of the 4x4 target.
const WGSL: &str = "
override gain: f32 = 0.25;
override enabled: bool = true;
override count: u32 = 2u;
override level: i32 = -1;
@id(7) override numbered: f32 = 0.5;
@vertex fn vs_left(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    var xs = array<f32, 6>(-1.0, 0.0, 0.0, -1.0, 0.0, -1.0);
    var ys = array<f32, 6>(-1.0, -1.0, 1.0, -1.0, 1.0, 1.0);
    return vec4<f32>(xs[index], ys[index], 0.0, 1.0);
}
@fragment fn fs() -> @location(0) vec4<f32> {
    return vec4<f32>(gain, select(0.0, 1.0, enabled), f32(count) / 4.0, 1.0);
}
@fragment fn fs_signed() -> @location(0) vec4<f32> {
    return vec4<f32>(f32(level) / 4.0, numbered, 0.0, 1.0);
}";

/// What `fs` writes with every override at its default.
const DEFAULTS: [f32; 4] = [0.25, 1.0, 0.5, 1.0];
/// What `fs` writes with `gain` 0.75, `enabled` false and `count` 3.
const TUNED: [f32; 4] = [0.75, 0.0, 0.75, 1.0];
const TUNING: [(&str, f64); 3] = [("gain", 0.75), ("enabled", 0.0), ("count", 3.0)];

fn stage(entry: &str, kind: StageKind) -> StageProgram {
    StageProgram::new(module(entry, kind), StageDeclarations::default()).unwrap()
}

fn module(entry: &str, kind: StageKind) -> StageModule {
    StageModule {
        source: StageSource::Wgsl(Arc::from(WGSL)),
        entry_point: entry.to_owned(),
        kind,
    }
}

fn signed() -> StageProgram {
    stage("fs_signed", StageKind::Fragment)
}

fn vertex() -> StageProgram {
    stage("vs_left", StageKind::Vertex)
}

fn fragment() -> StageProgram {
    stage("fs", StageKind::Fragment)
}

fn program(runtime: &ProgramRuntime, fragment: &StageProgram) -> GraphicsProgram {
    runtime.graphics_program(&vertex(), fragment).unwrap()
}

/// The image of one draw of `program` over a cleared target.
fn drawn(gpu: &Gpu, runtime: &mut ProgramRuntime, program: &GraphicsProgram) -> Vec<u8> {
    let draw = prepare(runtime, program, &Inputs::bound(Vec::new())).unwrap();
    render(gpu, &[], &[&draw])
}

/// The image of one draw of `fragment` after `vs_left`.
fn drawn_of(gpu: &Gpu, runtime: &mut ProgramRuntime, fragment: &StageProgram) -> Vec<u8> {
    let program = program(runtime, fragment);
    drawn(gpu, runtime, &program)
}

fn unknown(name: &str) -> OverrideError {
    OverrideError::Unknown {
        name: name.to_owned(),
    }
}

fn refused_value(error: Result<StageProgram, OverrideError>) -> String {
    match error {
        Err(OverrideError::Value { name, .. }) => name,
        other => panic!("expected a refused value, got {other:?}"),
    }
}

#[test]
fn an_override_changes_the_drawn_value_for_its_pipeline_only() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime();
    let plain = program(&runtime, &fragment());
    let tuned = program(&runtime, &fragment().with_overrides(&TUNING).unwrap());

    assert_halves(&drawn(&gpu, &mut runtime, &plain), DEFAULTS, CLEAR);
    assert_halves(&drawn(&gpu, &mut runtime, &tuned), TUNED, CLEAR);
    // The tuned pipeline did not replace the plain one.
    assert_halves(&drawn(&gpu, &mut runtime, &plain), DEFAULTS, CLEAR);
    assert_eq!(runtime.stats().pipelines_built, 2);
}

#[test]
fn signed_overrides_and_overrides_with_an_id_are_set_by_the_key_wgpu_names() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime();
    let signed = signed();
    // A numbered override is keyed by its id, as the pipeline constants of wgpu are.
    let tuned = signed
        .with_overrides(&[("level", 3.0), ("7", 0.75)])
        .unwrap();

    // `level` defaults to -1, which the unorm target stores as 0.
    assert_halves(
        &drawn_of(&gpu, &mut runtime, &signed),
        [0.0, 0.5, 0.0, 1.0],
        CLEAR,
    );
    assert_halves(
        &drawn_of(&gpu, &mut runtime, &tuned),
        [0.75, 0.75, 0.0, 1.0],
        CLEAR,
    );
}

#[test]
fn an_unknown_override_is_refused_by_name() {
    let stage = fragment();
    assert_eq!(
        stage.with_overrides(&[("gian", 0.5)]).unwrap_err(),
        unknown("gian")
    );
    // The known name next to it does not save the call.
    assert_eq!(
        stage
            .with_overrides(&[("gain", 0.5), ("brightness", 1.0)])
            .unwrap_err(),
        unknown("brightness")
    );
    // A numbered override is keyed by its id, not by its identifier.
    assert_eq!(
        signed().with_overrides(&[("numbered", 0.5)]).unwrap_err(),
        unknown("numbered")
    );
    assert!(
        unknown("gian").to_string().contains("gian"),
        "the message names the override"
    );
}

#[test]
fn a_value_of_the_wrong_type_is_refused_by_name() {
    let stage = fragment();
    let kinds = [
        ("enabled", 0.5),
        ("enabled", 2.0),
        ("count", -1.0),
        ("count", 1.5),
        ("count", 4_294_967_296.0),
        ("gain", f64::NAN),
        ("gain", f64::INFINITY),
        ("gain", 1.0e39),
    ];
    for (name, value) in kinds {
        assert_eq!(
            refused_value(stage.with_overrides(&[(name, value)])),
            name,
            "{name} = {value}"
        );
    }
    let level = stage;
    assert_eq!(
        refused_value(level.with_overrides(&[("level", 2_147_483_648.0)])),
        "level"
    );
    assert_eq!(
        refused_value(level.with_overrides(&[("level", 0.5)])),
        "level"
    );
    // The edges of each type are values.
    assert!(
        level
            .with_overrides(&[
                ("count", 4_294_967_295.0),
                ("level", -2_147_483_648.0),
                ("enabled", 1.0),
                ("gain", -3.0e38),
            ])
            .is_ok()
    );
}

#[test]
fn an_override_given_twice_is_refused_by_name() {
    let error = fragment()
        .with_overrides(&[("gain", 0.5), ("gain", 0.75)])
        .unwrap_err();
    assert_eq!(
        error,
        OverrideError::Duplicate {
            name: "gain".to_owned()
        }
    );
}

#[test]
fn a_module_that_declares_no_override_refuses_every_name() {
    let plain = StageProgram::new(
        StageModule {
            source: StageSource::Wgsl(Arc::from(
                "@vertex fn vs() -> @builtin(position) vec4<f32> { return vec4<f32>(0.0); }",
            )),
            entry_point: "vs".to_owned(),
            kind: StageKind::Vertex,
        },
        StageDeclarations::default(),
    )
    .unwrap();
    assert_eq!(
        plain.with_overrides(&[("gain", 1.0)]).unwrap_err(),
        unknown("gain")
    );
    // No overrides is always allowed, and is the stage it started as.
    assert_eq!(
        plain.with_overrides(&[]).unwrap().content_key(),
        plain.content_key()
    );
}

#[test]
fn equal_sources_with_different_overrides_share_one_shader_module() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime();
    let half = program(
        &runtime,
        &fragment().with_overrides(&[("gain", 0.5)]).unwrap(),
    );
    let most = program(
        &runtime,
        &fragment().with_overrides(&[("gain", 0.75)]).unwrap(),
    );
    let inputs = Inputs::bound(Vec::new());

    let before = runtime.stats();
    prepare(&mut runtime, &half, &inputs).unwrap();
    let first = added(before, runtime.stats());
    let modules = runtime.cached_modules();
    assert_eq!(first[1], 1, "the first draw builds a pipeline: {first:?}");

    // The second differs from the first only in an override: a new pipeline over the same
    // compiled modules.
    let before = runtime.stats();
    prepare(&mut runtime, &most, &inputs).unwrap();
    assert_eq!(added(before, runtime.stats()), [0, 1, 0, 0, 1]);
    assert_eq!(runtime.cached_modules(), modules);

    // Equal overrides are the same pipeline again.
    let before = runtime.stats();
    prepare(&mut runtime, &half, &inputs).unwrap();
    assert_eq!(added(before, runtime.stats()), [0, 0, 0, 0, 1]);
    assert_eq!(runtime.stats().pipelines_built, 2);
}

#[test]
fn the_order_overrides_are_given_in_does_not_make_a_new_pipeline() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime();
    let forward = fragment()
        .with_overrides(&[("gain", 0.5), ("count", 3.0)])
        .unwrap();
    let backward = fragment()
        .with_overrides(&[("count", 3.0), ("gain", 0.5)])
        .unwrap();
    assert_eq!(forward.content_key(), backward.content_key());
    let inputs = Inputs::bound(Vec::new());
    let forward = program(&runtime, &forward);
    let backward = program(&runtime, &backward);
    prepare(&mut runtime, &forward, &inputs).unwrap();
    prepare(&mut runtime, &backward, &inputs).unwrap();
    assert_eq!(runtime.stats().pipelines_built, 1);
}

#[test]
fn the_overrides_of_a_stage_are_part_of_its_content_key() {
    let base = fragment();
    let tuned = base.with_overrides(&TUNING).unwrap();
    assert_ne!(base.content_key(), tuned.content_key());
    assert_ne!(
        tuned.content_key(),
        base.with_overrides(&[("gain", 0.75)])
            .unwrap()
            .content_key()
    );
    assert_eq!(
        tuned.content_key(),
        base.with_overrides(&TUNING).unwrap().content_key()
    );
    // A later call replaces the overrides a stage had.
    assert_eq!(
        tuned.with_overrides(&[]).unwrap().content_key(),
        base.content_key()
    );
}

fn registered(runtime: &mut ProgramRuntime, fragment: StageDesc) -> GraphicsProgram {
    let vertex = StageDesc::wgsl(
        WGSL,
        "vs_left",
        StageKind::Vertex,
        StageDeclarations::default(),
    );
    let handle = runtime
        .register_program(ProgramDesc::single(
            "overrides",
            RenderState::opaque_depth(),
            vertex,
            fragment,
        ))
        .unwrap();
    runtime.variant_program(handle, &FeatureSet::new()).unwrap()
}

fn fragment_desc() -> StageDesc {
    StageDesc::wgsl(
        WGSL,
        "fs",
        StageKind::Fragment,
        StageDeclarations::default(),
    )
}

#[test]
fn a_registered_stage_applies_the_overrides_of_its_description() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime();
    let plain = registered(&mut runtime, fragment_desc());
    let tuned = registered(&mut runtime, fragment_desc().with_overrides(&TUNING));
    assert_halves(&drawn(&gpu, &mut runtime, &plain), DEFAULTS, CLEAR);
    assert_halves(&drawn(&gpu, &mut runtime, &tuned), TUNED, CLEAR);
}

#[test]
fn registering_a_stage_with_an_unknown_override_is_refused_by_name() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime();
    let vertex = StageDesc::wgsl(
        WGSL,
        "vs_left",
        StageKind::Vertex,
        StageDeclarations::default(),
    );
    let desc = ProgramDesc::single(
        "overrides",
        RenderState::opaque_depth(),
        vertex,
        fragment_desc().with_overrides(&[("gian", 1.0)]),
    );
    assert_eq!(
        runtime.register_program(desc).unwrap_err(),
        ProgramError::Override(unknown("gian"))
    );
}

/// Each invocation stores `scale + its index` into `result`.
const KERNEL: &str = "
override scale: u32 = 3u;
@group(0) @binding(0) var<storage, read_write> result: array<u32>;
@compute @workgroup_size(1) fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    result[id.x] = scale + id.x;
}";

#[test]
fn an_override_sets_a_constant_of_a_compute_pipeline_too() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime();
    let declarations = StageDeclarations {
        resources: ResourceLayout::new(vec![ResourceBinding {
            at: BindingAddress { set: 0, binding: 0 },
            source: BindingSource::Property(PropertyId::new("result")),
            visibility: wgpu::ShaderStages::COMPUTE,
            kind: ResourceKind::Storage {
                access: StorageAccess::ReadWrite,
                min_size: 4,
            },
        }]),
        constants: Vec::new(),
    };
    let kernel = StageProgram::new(
        StageModule {
            source: StageSource::Wgsl(Arc::from(KERNEL)),
            entry_point: "main".to_owned(),
            kind: StageKind::Compute,
        },
        declarations,
    )
    .unwrap();
    let ten = kernel.with_overrides(&[("scale", 10.0)]).unwrap();

    let words = |runtime: &mut ProgramRuntime, stage: &StageProgram| {
        let program = runtime.compute_program(stage).unwrap();
        let output = runtime.create_buffer("result", &[0xff; 12]).unwrap();
        let mut frame = gpu.authority.begin(&gpu.device).unwrap();
        runtime
            .dispatch(
                &mut frame,
                &DispatchRequest {
                    program: &program,
                    bindings: &[BoundResource::buffer(0, 0, output)],
                    groups: [3, 1, 1],
                },
            )
            .unwrap();
        let pending = runtime.read_buffer(&mut frame, output).unwrap();
        let index = frame.finish().submit().unwrap();
        pending
            .resolve(index)
            .unwrap()
            .chunks_exact(4)
            .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect::<Vec<_>>()
    };
    // Closed form: invocation i stores scale + i.
    assert_eq!(words(&mut runtime, &kernel), [3, 4, 5]);
    assert_eq!(words(&mut runtime, &ten), [10, 11, 12]);
    assert_eq!(runtime.stats().pipelines_built, 2);
    assert_eq!(runtime.stats().modules_built, 1);
}
