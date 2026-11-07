//! The runtime driven only through its neutral interface: WGSL stages, declared resources and
//! constant layouts, render state and variants, with no Unity record anywhere.
//!
//! Colours are predicted in closed form from the constant the program is given, and the clip
//! convention is pinned by which rows of the target a half-height quad covers.

use std::sync::Arc;

use wgpu::{ShaderStages, TextureFormat};
use worldrunner_render_gpu::configuration::AllocationBudget;
use worldrunner_render_gpu::device::{DeviceRequirements, headless_instance, request_device};
use worldrunner_render_gpu::submission::SubmissionAuthority;
use worldrunner_render_gpu::tolerance_test_support::assert_code_within;
use worldrunner_render_program::binding::{
    BindingAddress, BindingSource, PropertyId, ResourceBinding, ResourceKind, ResourceLayout,
};
use worldrunner_render_program::constants::{
    BoundConstantLayout, ConstantLayout, Member, ScalarKind, pack_constants,
};
use worldrunner_render_program::error::ProgramError;
use worldrunner_render_program::layout::{LayoutError, ProgramLayout};
use worldrunner_render_program::module::{
    ModuleError, StageDeclarations, StageKind, StageModule, StageProgram, StageSource,
};
use worldrunner_render_program::properties::{PropertySet, PropertyValue};
use worldrunner_render_program::runtime::{
    BoundResource, DrawRequest, Geometry, GraphicsProgram, ProgramRuntime, TargetImage,
    TargetSignature, VertexInput,
};
use worldrunner_render_program::state::{RenderState, StencilState};
use worldrunner_render_program::variant::{FeatureId, FeatureSet, UnknownFeatures, Variants};

const WGSL: &str = "
struct Tint { color: vec4<f32> }
@group(0) @binding(0) var<uniform> tint: Tint;
@vertex fn vs(@location(0) position: vec2<f32>) -> @builtin(position) vec4<f32> {
    return vec4<f32>(position, 0.0, 1.0);
}
@fragment fn fs() -> @location(0) vec4<f32> { return tint.color; }
@fragment fn fs_double() -> @location(0) vec4<f32> { return tint.color * 2.0; }";

const COLOR: [f32; 4] = [0.25, 0.5, 0.75, 1.0];

fn at(set: u32, binding: u32) -> BindingAddress {
    BindingAddress { set, binding }
}

fn property(name: &str) -> BindingSource {
    BindingSource::Property(PropertyId::new(name))
}

/// A uniform buffer at set 0 binding 0 holding one `vec4<f32>` named `color`.
fn tint_declarations(visibility: ShaderStages, min_size: u64) -> StageDeclarations {
    StageDeclarations {
        resources: ResourceLayout::new(vec![ResourceBinding {
            at: at(0, 0),
            source: property("tint"),
            visibility,
            kind: ResourceKind::Uniform { min_size },
        }]),
        constants: vec![BoundConstantLayout {
            at: at(0, 0),
            layout: ConstantLayout {
                name: "Tint".to_owned(),
                byte_size: 16,
                members: vec![Member {
                    source: property("color"),
                    kind: ScalarKind::F32,
                    rows: 1,
                    cols: 4,
                    array: None,
                    offset: 0,
                    matrix: None,
                }],
            },
        }],
    }
}

fn module(source: &str, entry: &str, kind: StageKind) -> StageModule {
    StageModule {
        source: StageSource::Wgsl(Arc::from(source)),
        entry_point: entry.to_owned(),
        kind,
    }
}

fn stage(entry: &str, kind: StageKind, declarations: StageDeclarations) -> StageProgram {
    StageProgram::new(module(WGSL, entry, kind), declarations).unwrap()
}

fn vertex() -> StageProgram {
    stage("vs", StageKind::Vertex, StageDeclarations::default())
}

fn fragment(entry: &str) -> StageProgram {
    stage(
        entry,
        StageKind::Fragment,
        tint_declarations(ShaderStages::FRAGMENT, 0),
    )
}

struct Gpu {
    device: wgpu::Device,
    authority: SubmissionAuthority,
}

impl Gpu {
    fn new() -> Self {
        let instance = headless_instance();
        let adapter =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
                .expect("the native tests need a GPU adapter");
        let info = adapter.get_info();
        eprintln!("render-program native tests adapter: {}", info.name);
        let (device, queue, _) =
            pollster::block_on(request_device(&adapter, &DeviceRequirements::default()))
                .expect("the adapter gives a device");
        Self {
            device,
            authority: SubmissionAuthority::new(queue),
        }
    }

    fn runtime(&self) -> ProgramRuntime {
        ProgramRuntime::new(
            self.device.clone(),
            self.authority.clone(),
            AllocationBudget(1 << 24),
        )
    }
}

/// A program, its constants bound, and a 2x2 target cleared to transparent black.
struct Rig {
    runtime: ProgramRuntime,
    program: GraphicsProgram,
    bindings: Vec<BoundResource>,
    target: worldrunner_render_program::runtime::TargetHandle,
}

fn rig(gpu: &Gpu, fragment: &StageProgram) -> Rig {
    let mut runtime = gpu.runtime();
    let program = runtime.graphics_program(&vertex(), fragment).unwrap();
    let mut properties = PropertySet::new();
    properties.set("color", PropertyValue::Vector(COLOR));
    let target_layout = &fragment.constant_targets()[0];
    let packed = pack_constants(&target_layout.layout, &[&properties], 16).unwrap();
    assert!(packed.missing.is_empty());
    let buffer = runtime.create_buffer("tint", &packed.bytes).unwrap();
    let bindings = vec![BoundResource::buffer(
        target_layout.at.set,
        target_layout.at.binding,
        buffer,
    )];
    let target = runtime
        .create_render_target("native", [2, 2], TextureFormat::Rgba8Unorm, None)
        .unwrap();
    submit(&mut runtime, |runtime, frame| {
        runtime.clear_target(frame, target, [0.0; 4], 1.0)
    })
    .unwrap();
    Rig {
        runtime,
        program,
        bindings,
        target,
    }
}

fn submit<T>(
    runtime: &mut ProgramRuntime,
    record: impl FnOnce(&mut ProgramRuntime, &mut wgpu::CommandEncoder) -> Result<T, ProgramError>,
) -> Result<(T, wgpu::SubmissionIndex), ProgramError> {
    let authority = runtime.authority().clone();
    let device = runtime.device().clone();
    let mut frame = authority.begin(&device)?;
    let value = record(runtime, &mut frame)?;
    let index = frame.finish().submit()?;
    Ok((value, index))
}

/// Two triangles covering `[x0, y0]..[x1, y1]` of clip space.
fn rectangle(runtime: &mut ProgramRuntime, [x0, y0, x1, y1]: [f32; 4]) -> VertexInput {
    let corners = [[x0, y0], [x1, y0], [x1, y1], [x0, y0], [x1, y1], [x0, y1]];
    let bytes: Vec<u8> = corners
        .iter()
        .flatten()
        .flat_map(|value| value.to_le_bytes())
        .collect();
    VertexInput {
        buffer: runtime.create_buffer("rectangle", &bytes).unwrap(),
        stride: 8,
        step: wgpu::VertexStepMode::Vertex,
        attributes: wgpu::vertex_attr_array![0 => Float32x2].to_vec(),
    }
}

fn no_cull() -> RenderState {
    let mut state = RenderState::opaque_depth();
    state.raster.cull = None;
    state
}

fn draw(rig: &mut Rig, state: &RenderState, area: [f32; 4]) -> Result<TargetImage, ProgramError> {
    let input = rectangle(&mut rig.runtime, area);
    let request = DrawRequest {
        program: &rig.program,
        state,
        target: rig.target,
        vertex_inputs: std::slice::from_ref(&input),
        bindings: &rig.bindings,
        geometry: Geometry::Vertices { first: 0, count: 6 },
    };
    submit(&mut rig.runtime, |runtime, frame| {
        runtime.draw(frame, &request)
    })?;
    let (pending, index) = submit(&mut rig.runtime, |runtime, frame| {
        runtime.read_target(frame, rig.target)
    })?;
    pending.resolve(index)
}

fn assert_pixel(image: &TargetImage, (x, y): (u32, u32), want: [f32; 4]) {
    for (channel, (&code, value)) in image.pixel(x, y).iter().zip(want).enumerate() {
        assert_code_within(
            code,
            f64::from(value * 255.0),
            0.0,
            &format!("pixel ({x}, {y}) channel {channel}"),
        );
    }
}

#[test]
fn a_wgsl_program_fills_the_target_with_its_constant_colour() {
    let gpu = Gpu::new();
    let mut rig = rig(&gpu, &fragment("fs"));
    let image = draw(&mut rig, &no_cull(), [-1.0, -1.0, 1.0, 1.0]).unwrap();
    for y in 0..2 {
        for x in 0..2 {
            assert_pixel(&image, (x, y), COLOR);
        }
    }
}

#[test]
fn a_wgsl_position_is_used_as_written_so_positive_y_is_the_top_row() {
    let gpu = Gpu::new();
    let mut rig = rig(&gpu, &fragment("fs"));
    let image = draw(&mut rig, &no_cull(), [-1.0, 0.0, 1.0, 1.0]).unwrap();
    for x in 0..2 {
        assert_pixel(&image, (x, 0), COLOR);
        assert_pixel(&image, (x, 1), [0.0; 4]);
    }
}

#[test]
fn a_feature_selects_the_variant_whose_fragment_doubles_the_colour() {
    let gpu = Gpu::new();
    let mut plain = rig(&gpu, &fragment("fs"));
    let program_for_double = plain
        .runtime
        .graphics_program(&vertex(), &fragment("fs_double"))
        .unwrap();
    let names =
        |names: &[&str]| -> FeatureSet { names.iter().map(|name| FeatureId::new(*name)).collect() };
    let variants = Variants::new(
        names(&["DOUBLE"]),
        [
            (names(&[]), plain.program.clone()),
            (names(&["DOUBLE"]), program_for_double),
        ],
    )
    .unwrap();
    plain.program = variants
        .select(&names(&["DOUBLE"]), UnknownFeatures::Fail)
        .unwrap()
        .clone();
    let image = draw(&mut plain, &no_cull(), [-1.0, -1.0, 1.0, 1.0]).unwrap();
    assert_pixel(&image, (0, 0), [0.5, 1.0, 1.0, 1.0]);
    assert!(
        variants
            .select(&names(&["TRIPLE"]), UnknownFeatures::Fail)
            .is_err()
    );
}

#[test]
fn state_a_draw_does_not_support_is_refused_by_name() {
    let gpu = Gpu::new();
    let mut rig = rig(&gpu, &fragment("fs"));
    let mut state = no_cull();
    state.stencil = Some(StencilState {
        ops: wgpu::StencilState::default(),
        reference: 1,
    });
    let error = draw(&mut rig, &state, [-1.0, -1.0, 1.0, 1.0]).unwrap_err();
    assert_eq!(error, ProgramError::UnsupportedState { what: "stencil" });
    let error = TargetSignature::new(&[TextureFormat::Rgba8Unorm; 2], None, 1).unwrap_err();
    assert!(matches!(error, ProgramError::UnsupportedTarget(_)));
}

#[test]
fn wgsl_that_does_not_parse_or_lacks_the_entry_point_is_a_named_error() {
    let broken = StageProgram::new(
        module("fn broken(", "vs", StageKind::Vertex),
        StageDeclarations::default(),
    );
    assert!(matches!(broken.unwrap_err(), ModuleError::Parse(_)));
    let missing = StageProgram::new(
        module(WGSL, "nowhere", StageKind::Vertex),
        StageDeclarations::default(),
    );
    assert_eq!(
        missing.unwrap_err(),
        ModuleError::EntryPointMissing {
            name: "nowhere".to_owned(),
            kind: StageKind::Vertex
        }
    );
}

#[test]
fn declarations_are_checked_against_what_the_shader_uses() {
    let build =
        |declarations| ProgramLayout::build(&[&stage("fs", StageKind::Fragment, declarations)]);

    let undeclared = build(StageDeclarations::default()).unwrap_err();
    assert!(matches!(
        undeclared,
        LayoutError::UndeclaredResource {
            set: 0,
            binding: 0,
            ..
        }
    ));

    let mut sampler = tint_declarations(ShaderStages::FRAGMENT, 0);
    sampler.resources.bindings[0].kind = ResourceKind::Sampler(wgpu::SamplerBindingType::Filtering);
    assert!(matches!(
        build(sampler).unwrap_err(),
        LayoutError::Mismatch {
            set: 0,
            binding: 0,
            ..
        }
    ));

    let vertex_only = tint_declarations(ShaderStages::VERTEX, 0);
    assert!(matches!(
        build(vertex_only).unwrap_err(),
        LayoutError::Visibility {
            set: 0,
            binding: 0,
            ..
        }
    ));

    let mut twice = tint_declarations(ShaderStages::FRAGMENT, 0);
    let again = twice.resources.bindings[0].clone();
    twice.resources.bindings.push(again);
    assert_eq!(
        build(twice).unwrap_err(),
        LayoutError::DuplicateDeclaration { set: 0, binding: 0 }
    );
}

#[test]
fn a_declared_minimum_size_larger_than_the_shaders_need_sizes_the_slot() {
    let layout = ProgramLayout::build(&[&stage(
        "fs",
        StageKind::Fragment,
        tint_declarations(ShaderStages::FRAGMENT, 64),
    )])
    .unwrap();
    assert_eq!(layout.slot(0, 0).unwrap().min_size, 64);
    let smaller = ProgramLayout::build(&[&fragment("fs")]).unwrap();
    assert_eq!(smaller.slot(0, 0).unwrap().min_size, 16);
}

#[test]
fn a_declaration_visible_to_both_stages_is_unused_by_the_stage_that_skips_it() {
    let shared = tint_declarations(ShaderStages::VERTEX | ShaderStages::FRAGMENT, 0);
    let vertex = stage("vs", StageKind::Vertex, shared.clone());
    let fragment = stage("fs", StageKind::Fragment, shared);
    let layout = ProgramLayout::build(&[&vertex, &fragment]).unwrap();
    assert_eq!(layout.unused().len(), 1);
    assert_eq!(layout.unused()[0].stage, StageKind::Vertex);
    assert_eq!(
        layout.slot(0, 0).unwrap().visibility,
        ShaderStages::VERTEX | ShaderStages::FRAGMENT
    );
}

#[test]
fn the_content_key_of_a_wgsl_stage_follows_its_text_and_entry_point() {
    let base = fragment("fs").content_key();
    assert_eq!(base, fragment("fs").content_key());
    assert_ne!(base, fragment("fs_double").content_key());
    let edited = format!("{WGSL}\n// edited");
    let other = StageProgram::new(
        module(&edited, "fs", StageKind::Fragment),
        StageDeclarations::default(),
    )
    .unwrap();
    assert_ne!(base, other.content_key());
}
