//! Draws over resources another owner created: bindings filled by engine semantic, borrowed
//! vertex, instance and index buffers, and instance ranges.
//!
//! Colours are predicted in closed form from the tint, the texels and the sampler the draw is
//! given, and the draws land on exact pixel quadrants, so a wrong binding, a wrong range or the
//! runtime's own fixed sampler standing in for the lent one changes the expected bytes.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use wgpu::util::DeviceExt;
use wgpu::{ShaderStages, TextureFormat};
use worldrunner_render_gpu::tolerance_test_support::assert_code_within;
use worldrunner_render_program::binding::{
    BindingAddress, BindingSource, EngineSemantic, ResourceBinding, ResourceKind, ResourceLayout,
};
use worldrunner_render_program::error::ProgramError;
use worldrunner_render_program::layout::LayoutError;
use worldrunner_render_program::module::{
    StageDeclarations, StageKind, StageModule, StageProgram, StageSource,
};
use worldrunner_render_program::runtime::{
    BoundResource, GraphicsProgram, Import, ImportedDraw, ImportedGeometry, ImportedIndices,
    ImportedVertexInput, Keepalive, PreparedProgramDraw, ProgramRuntime, TargetSignature,
};
use worldrunner_render_program::state::RenderState;

mod support;
use support::{Attachment, Gpu};

const WGSL: &str = "
struct Tint { color: vec4<f32> }
@group(0) @binding(0) var<uniform> tint: Tint;
@group(0) @binding(1) var albedo: texture_2d<f32>;
@group(0) @binding(2) var albedo_sampler: sampler;
struct Varying { @builtin(position) position: vec4<f32>, @location(0) uv: vec2<f32> }
@vertex fn vs(
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) shift: vec2<f32>,
) -> Varying {
    return Varying(vec4<f32>(position + shift, 0.0, 1.0), uv);
}
@vertex fn vs_tinted(
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) shift: vec2<f32>,
) -> Varying {
    return Varying(vec4<f32>(position + shift, tint.color.x * 0.0, 1.0), uv);
}
@fragment fn fs(input: Varying) -> @location(0) vec4<f32> {
    return tint.color * textureSample(albedo, albedo_sampler, input.uv);
}";

const SIZE: u32 = 4;
const RGBA8: TextureFormat = TextureFormat::Rgba8Unorm;
const TINT: [f32; 4] = [0.5, 1.0, 1.0, 1.0];

fn at(binding: u32) -> BindingAddress {
    BindingAddress { set: 0, binding }
}

fn engine(name: &str) -> BindingSource {
    BindingSource::Engine(EngineSemantic::named(name))
}

fn semantic(name: &str) -> EngineSemantic {
    EngineSemantic::named(name)
}

fn declarations() -> StageDeclarations {
    let binding = |binding, name, kind| ResourceBinding {
        at: at(binding),
        source: engine(name),
        visibility: ShaderStages::FRAGMENT,
        kind,
    };
    StageDeclarations {
        resources: ResourceLayout::new(vec![
            binding(0, "test.tint", ResourceKind::Uniform { min_size: 16 }),
            binding(
                1,
                "test.albedo",
                ResourceKind::Texture {
                    dimension: wgpu::TextureViewDimension::D2,
                    sample: wgpu::TextureSampleType::Float { filterable: true },
                    multisampled: false,
                },
            ),
            binding(
                2,
                "test.sampler",
                ResourceKind::Sampler(wgpu::SamplerBindingType::Filtering),
            ),
        ]),
        constants: Vec::new(),
    }
}

fn stage(entry: &str, kind: StageKind, declarations: StageDeclarations) -> StageProgram {
    StageProgram::new(
        StageModule {
            source: StageSource::Wgsl(Arc::from(WGSL)),
            entry_point: entry.to_owned(),
            kind,
        },
        declarations,
    )
    .unwrap()
}

impl Gpu {
    /// A 2x1 texture of a red and a blue texel.
    fn albedo(&self) -> (wgpu::Texture, wgpu::TextureView) {
        let texture = self.device.create_texture_with_data(
            self.authority.queue(),
            &wgpu::TextureDescriptor {
                label: Some("albedo"),
                size: wgpu::Extent3d {
                    width: 2,
                    height: 1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: RGBA8,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            &[255, 0, 0, 255, 0, 0, 255, 255],
        );
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        (texture, view)
    }

    fn sampler(&self, filter: wgpu::FilterMode) -> wgpu::Sampler {
        self.device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: filter,
            min_filter: filter,
            ..wgpu::SamplerDescriptor::default()
        })
    }

    fn target(&self) -> wgpu::Texture {
        self.texture(SIZE, RGBA8, 1)
    }

    /// Records one pass over `target`, cleared to transparent black, with `draws`, and reads
    /// the result back.
    fn render(&self, target: &wgpu::Texture, draws: &[&PreparedProgramDraw]) -> Vec<u8> {
        let view = target.create_view(&wgpu::TextureViewDescriptor::default());
        self.pass(
            &[Attachment {
                view: &view,
                resolve: None,
                clear: wgpu::Color::TRANSPARENT,
            }],
            draws,
        );
        self.read(target)
    }
}

fn pixel(image: &[u8], x: u32, y: u32) -> [u8; 4] {
    let at = ((y * SIZE + x) * 4) as usize;
    image[at..at + 4].try_into().unwrap()
}

fn assert_pixel(image: &[u8], at: (u32, u32), want: [f32; 4]) {
    assert_pixel_within(image, at, want, 0.0);
}

fn assert_pixel_within(image: &[u8], (x, y): (u32, u32), want: [f32; 4], extra: f64) {
    for (channel, (&code, value)) in pixel(image, x, y).iter().zip(want).enumerate() {
        assert_code_within(
            code,
            f64::from(value * 255.0),
            extra,
            &format!("pixel ({x}, {y}) channel {channel}"),
        );
    }
}

/// The lent resources of a quadrant draw, and the program that reads them.
struct Lent {
    program: GraphicsProgram,
    imports: Vec<Import>,
    inputs: Vec<ImportedVertexInput>,
    indices: wgpu::Buffer,
    instance_buffer: wgpu::Buffer,
}

/// The bottom-left quadrant of the target as a quad with `uv.x` of 0.4: left of the centre of
/// the second texel, so a point sampler reads the first texel and a linear one blends.
fn lent(gpu: &Gpu, runtime: &ProgramRuntime, filter: wgpu::FilterMode) -> Lent {
    let program = runtime
        .graphics_program(
            &stage("vs", StageKind::Vertex, StageDeclarations::default()),
            &stage("fs", StageKind::Fragment, declarations()),
        )
        .unwrap();
    let corner = |x: f32, y: f32| [x, y, 0.4, 0.5];
    let vertices: Vec<f32> = [
        corner(-1.0, -1.0),
        corner(0.0, -1.0),
        corner(0.0, 0.0),
        corner(-1.0, 0.0),
    ]
    .concat();
    let vertex_buffer = gpu.floats(wgpu::BufferUsages::VERTEX, &vertices);
    // One shift per instance: bottom-left, bottom-right, top-left.
    let shifts = [0.0, 0.0, 1.0, 0.0, 0.0, 1.0];
    let instance_buffer = gpu.floats(wgpu::BufferUsages::VERTEX, &shifts);
    let indices: Vec<u8> = [0u16, 1, 2, 0, 2, 3]
        .iter()
        .flat_map(|index| index.to_le_bytes())
        .collect();
    let (texture, view) = gpu.albedo();
    Lent {
        program,
        imports: vec![
            Import::buffer(
                semantic("test.tint"),
                gpu.floats(wgpu::BufferUsages::UNIFORM, &TINT),
            ),
            Import::texture(semantic("test.albedo"), texture, view),
            Import::sampler(semantic("test.sampler"), gpu.sampler(filter)),
        ],
        inputs: vec![
            ImportedVertexInput {
                buffer: vertex_buffer,
                stride: 16,
                step: wgpu::VertexStepMode::Vertex,
                attributes: wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2].to_vec(),
            },
            ImportedVertexInput {
                buffer: instance_buffer.clone(),
                stride: 8,
                step: wgpu::VertexStepMode::Instance,
                attributes: wgpu::vertex_attr_array![2 => Float32x2].to_vec(),
            },
        ],
        indices: gpu.buffer(wgpu::BufferUsages::INDEX, &indices),
        instance_buffer,
    }
}

fn no_cull() -> RenderState {
    let mut state = RenderState::opaque_depth();
    state.raster.cull = None;
    state.depth = None;
    state
}

fn signature() -> TargetSignature {
    TargetSignature::new(&[RGBA8], None, 1).unwrap()
}

impl Lent {
    fn geometry(&self) -> ImportedGeometry {
        ImportedGeometry::Indexed(ImportedIndices {
            buffer: self.indices.clone(),
            format: wgpu::IndexFormat::Uint16,
            first: 0,
            count: 6,
            base_vertex: 0,
        })
    }

    fn prepare(
        &self,
        runtime: &mut ProgramRuntime,
        instances: std::ops::Range<u32>,
        owners: &[Keepalive],
    ) -> Result<PreparedProgramDraw, ProgramError> {
        self.prepare_with(runtime, &self.imports, instances, owners)
    }

    fn prepare_with(
        &self,
        runtime: &mut ProgramRuntime,
        imports: &[Import],
        instances: std::ops::Range<u32>,
        owners: &[Keepalive],
    ) -> Result<PreparedProgramDraw, ProgramError> {
        runtime.prepare_imported_draw(
            &ImportedDraw {
                program: &self.program,
                state: &no_cull(),
                vertex_inputs: &self.inputs,
                bindings: &[],
                imports,
                geometry: &self.geometry(),
                instances,
                owners,
            },
            signature(),
        )
    }
}

const RED_TINTED: [f32; 4] = [0.5, 0.0, 0.0, 1.0];
const CLEAR: [f32; 4] = [0.0; 4];

#[test]
fn a_lent_buffer_texture_and_sampler_fill_the_bindings_that_name_them() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime();
    let lent = lent(&gpu, &runtime, wgpu::FilterMode::Nearest);
    let draw = lent.prepare(&mut runtime, 0..1, &[]).unwrap();
    let image = gpu.render(&gpu.target(), &[&draw]);
    // The bottom-left quadrant is rows 2 and 3, columns 0 and 1; a point sampler at u = 0.4
    // reads the red texel, so the lent sampler, not the runtime's linear one, is bound.
    for (x, y) in [(0, 2), (1, 2), (0, 3), (1, 3)] {
        assert_pixel(&image, (x, y), RED_TINTED);
    }
    for (x, y) in [(2, 2), (0, 1), (2, 0), (3, 3)] {
        assert_pixel(&image, (x, y), CLEAR);
    }
}

#[test]
fn the_runtimes_own_linear_sampler_would_have_blended_the_texels() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime();
    let lent = lent(&gpu, &runtime, wgpu::FilterMode::Linear);
    let draw = lent.prepare(&mut runtime, 0..1, &[]).unwrap();
    let image = gpu.render(&gpu.target(), &[&draw]);
    // u = 0.4 of two texels is 0.3 texels past the first centre: 70% red and 30% blue.
    // Linear weights are fixed point on some devices, so allow a step beyond the shared bound.
    assert_pixel_within(&image, (0, 2), [0.5 * 0.7, 0.0, 0.3, 1.0], 1.0);
}

#[test]
fn an_instance_range_draws_exactly_the_instances_it_names() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime();
    let lent = lent(&gpu, &runtime, wgpu::FilterMode::Nearest);
    let target = gpu.target();

    let last_two = lent.prepare(&mut runtime, 1..3, &[]).unwrap();
    let image = gpu.render(&target, &[&last_two]);
    assert_pixel(&image, (0, 2), CLEAR);
    assert_pixel(&image, (2, 2), RED_TINTED);
    assert_pixel(&image, (3, 3), RED_TINTED);
    assert_pixel(&image, (0, 0), RED_TINTED);
    assert_pixel(&image, (1, 1), RED_TINTED);

    let first = lent.prepare(&mut runtime, 0..1, &[]).unwrap();
    let image = gpu.render(&target, &[&first]);
    assert_pixel(&image, (0, 2), RED_TINTED);
    assert_pixel(&image, (2, 2), CLEAR);
    assert_pixel(&image, (0, 0), CLEAR);

    let none = lent.prepare(&mut runtime, 2..2, &[]).unwrap();
    let image = gpu.render(&target, &[&none]);
    assert!(
        image.iter().all(|byte| *byte == 0),
        "an empty range draws nothing"
    );
}

#[test]
fn an_instance_range_past_the_instance_buffer_is_refused_by_name() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime();
    let lent = lent(&gpu, &runtime, wgpu::FilterMode::Nearest);
    assert!(lent.instance_buffer.size() == 24);
    // Three instances of eight bytes are held, and the instance-rate input is slot 1.
    assert_eq!(
        lent.prepare(&mut runtime, 0..4, &[]).unwrap_err(),
        ProgramError::VertexRangePastBuffer {
            slot: 1,
            needed: 32,
            have: 24
        }
    );
    assert_eq!(
        lent.prepare(&mut runtime, 3..5, &[]).unwrap_err(),
        ProgramError::VertexRangePastBuffer {
            slot: 1,
            needed: 40,
            have: 24
        },
        "the range's end bounds the buffer, not its length"
    );
    assert_eq!(
        lent.prepare(&mut runtime, 2..1, &[]).unwrap_err(),
        ProgramError::InstanceRange { start: 2, end: 1 }
    );
    assert!(lent.prepare(&mut runtime, 0..3, &[]).is_ok());
}

#[test]
fn an_index_range_past_the_lent_index_buffer_is_refused_by_name() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime();
    let lent = lent(&gpu, &runtime, wgpu::FilterMode::Nearest);
    let geometry = ImportedGeometry::Indexed(ImportedIndices {
        buffer: lent.indices.clone(),
        format: wgpu::IndexFormat::Uint16,
        first: 3,
        count: 6,
        base_vertex: 0,
    });
    let error = runtime
        .prepare_imported_draw(
            &ImportedDraw {
                program: &lent.program,
                state: &no_cull(),
                vertex_inputs: &lent.inputs,
                bindings: &[],
                imports: &lent.imports,
                geometry: &geometry,
                instances: 0..1,
                owners: &[],
            },
            signature(),
        )
        .unwrap_err();
    assert_eq!(
        error,
        ProgramError::IndexRangePastBuffer {
            end: 9,
            available: 6
        }
    );
}

#[test]
fn a_non_indexed_draw_over_lent_vertices_reads_only_the_range_it_names() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime();
    let lent = lent(&gpu, &runtime, wgpu::FilterMode::Nearest);
    // Vertices 0..3 of the quad are one triangle: the lower right half of the quadrant.
    let geometry = ImportedGeometry::Vertices { first: 0, count: 3 };
    let draw = runtime
        .prepare_imported_draw(
            &ImportedDraw {
                program: &lent.program,
                state: &no_cull(),
                vertex_inputs: &lent.inputs,
                bindings: &[],
                imports: &lent.imports,
                geometry: &geometry,
                instances: 0..1,
                owners: &[],
            },
            signature(),
        )
        .unwrap();
    let image = gpu.render(&gpu.target(), &[&draw]);
    assert_pixel(&image, (1, 3), RED_TINTED);
    assert_pixel(&image, (0, 2), CLEAR);
    let past = ImportedGeometry::Vertices { first: 2, count: 3 };
    let error = runtime
        .prepare_imported_draw(
            &ImportedDraw {
                program: &lent.program,
                state: &no_cull(),
                vertex_inputs: &lent.inputs,
                bindings: &[],
                imports: &lent.imports,
                geometry: &past,
                instances: 0..1,
                owners: &[],
            },
            signature(),
        )
        .unwrap_err();
    assert!(matches!(
        error,
        ProgramError::VertexRangePastBuffer { slot: 0, .. }
    ));
}

fn without(imports: &[Import], name: &str) -> Vec<Import> {
    imports
        .iter()
        .filter(|import| import.semantic != semantic(name))
        .cloned()
        .collect()
}

#[test]
fn a_binding_with_no_import_is_refused_by_name_and_so_is_one_nothing_takes() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime();
    let lent = lent(&gpu, &runtime, wgpu::FilterMode::Nearest);
    let imports = without(&lent.imports, "test.albedo");
    assert_eq!(
        lent.prepare_with(&mut runtime, &imports, 0..1, &[])
            .unwrap_err(),
        ProgramError::MissingImport {
            semantic: "test.albedo".to_owned(),
            set: 0,
            binding: 1
        }
    );
    let mut extra = lent.imports.clone();
    extra.push(Import::sampler(
        semantic("test.unknown"),
        gpu.sampler(wgpu::FilterMode::Nearest),
    ));
    assert_eq!(
        lent.prepare_with(&mut runtime, &extra, 0..1, &[])
            .unwrap_err(),
        ProgramError::UnusedImport {
            semantic: "test.unknown".to_owned()
        }
    );
    let mut twice = lent.imports.clone();
    twice.push(lent.imports[2].clone());
    assert_eq!(
        lent.prepare_with(&mut runtime, &twice, 0..1, &[])
            .unwrap_err(),
        ProgramError::DuplicateImport {
            semantic: "test.sampler".to_owned()
        }
    );
}

#[test]
fn an_import_of_the_wrong_kind_or_too_small_is_refused_by_name() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime();
    let lent = lent(&gpu, &runtime, wgpu::FilterMode::Nearest);
    let mut wrong = without(&lent.imports, "test.sampler");
    wrong.push(Import::buffer(
        semantic("test.sampler"),
        gpu.floats(wgpu::BufferUsages::UNIFORM, &TINT),
    ));
    let error = lent
        .prepare_with(&mut runtime, &wrong, 0..1, &[])
        .unwrap_err();
    assert!(matches!(
        error,
        ProgramError::WrongResource {
            set: 0,
            binding: 2,
            ..
        }
    ));
    let mut small = without(&lent.imports, "test.tint");
    small.push(Import::buffer(
        semantic("test.tint"),
        gpu.floats(wgpu::BufferUsages::UNIFORM, &[1.0, 1.0]),
    ));
    assert_eq!(
        lent.prepare_with(&mut runtime, &small, 0..1, &[])
            .unwrap_err(),
        ProgramError::BufferTooSmall {
            set: 0,
            binding: 0,
            needed: 16,
            have: 8
        }
    );
}

#[test]
fn a_runtime_handle_cannot_fill_a_binding_that_takes_an_import() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime();
    let lent = lent(&gpu, &runtime, wgpu::FilterMode::Nearest);
    let own = runtime.create_buffer("own", &[0; 16]).unwrap();
    let error = runtime
        .prepare_imported_draw(
            &ImportedDraw {
                program: &lent.program,
                state: &no_cull(),
                vertex_inputs: &lent.inputs,
                bindings: &[BoundResource::buffer(0, 0, own)],
                imports: &lent.imports,
                geometry: &lent.geometry(),
                instances: 0..1,
                owners: &[],
            },
            signature(),
        )
        .unwrap_err();
    assert_eq!(
        error,
        ProgramError::UnexpectedBinding { set: 0, binding: 0 }
    );
}

#[test]
fn stages_that_source_one_binding_from_different_semantics_conflict_by_name() {
    let gpu = Gpu::new();
    let runtime = gpu.runtime();
    let both = |name| {
        let mut declared = declarations();
        declared.resources.bindings[0].source = engine(name);
        declared.resources.bindings[0].visibility = ShaderStages::VERTEX_FRAGMENT;
        declared
    };
    let error = runtime
        .graphics_program(
            &stage("vs_tinted", StageKind::Vertex, both("test.tint")),
            &stage("fs", StageKind::Fragment, both("test.other")),
        )
        .map(|_| ())
        .unwrap_err();
    assert!(matches!(
        error,
        ProgramError::Layout(LayoutError::Conflict {
            set: 0,
            binding: 0,
            ..
        })
    ));
    assert!(
        runtime
            .graphics_program(
                &stage("vs_tinted", StageKind::Vertex, both("test.tint")),
                &stage("fs", StageKind::Fragment, both("test.tint")),
            )
            .is_ok(),
        "one semantic in both stages is one binding"
    );
}

/// Sets a flag when the last holder lets go.
struct Probe(Arc<AtomicBool>);

impl Drop for Probe {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

fn probe() -> (Keepalive, Arc<AtomicBool>) {
    let dropped = Arc::new(AtomicBool::new(false));
    (Arc::new(Probe(dropped.clone())), dropped)
}

#[test]
fn a_lenders_owner_is_held_by_the_draw_and_by_the_frames_that_retain_it() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime();
    let lent = lent(&gpu, &runtime, wgpu::FilterMode::Nearest);
    let (owner, dropped) = probe();
    let draw = lent
        .prepare(&mut runtime, 0..1, std::slice::from_ref(&owner))
        .unwrap();
    drop(owner);
    assert!(!dropped.load(Ordering::SeqCst), "the draw holds its owners");

    drop(draw);
    assert!(
        dropped.load(Ordering::SeqCst),
        "a draw no frame retained lets the owner go with it"
    );
}

#[test]
fn a_frame_that_retained_the_draw_holds_the_owner_until_it_is_cancelled() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime();
    let lent = lent(&gpu, &runtime, wgpu::FilterMode::Nearest);
    let (owner, dropped) = probe();
    let draw = lent
        .prepare(&mut runtime, 0..1, std::slice::from_ref(&owner))
        .unwrap();
    drop(owner);
    let frame = gpu.authority.begin(&gpu.device).unwrap();
    draw.retain(&frame).unwrap();
    drop(draw);
    assert!(
        !dropped.load(Ordering::SeqCst),
        "the unsubmitted frame holds the owner after the draw is gone"
    );
    frame.finish().cancel();
    assert!(
        dropped.load(Ordering::SeqCst),
        "a cancelled frame gives the owner back at once"
    );
}

#[test]
fn a_submitted_frame_holds_the_owner_until_the_gpu_has_completed_it() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime();
    let lent = lent(&gpu, &runtime, wgpu::FilterMode::Nearest);
    let (owner, dropped) = probe();
    let draw = lent
        .prepare(&mut runtime, 0..1, std::slice::from_ref(&owner))
        .unwrap();
    drop(owner);
    let frame = gpu.authority.begin(&gpu.device).unwrap();
    draw.retain(&frame).unwrap();
    drop(draw);
    frame.finish().submit().unwrap();
    assert!(
        !dropped.load(Ordering::SeqCst),
        "submitted, not yet observed complete"
    );
    gpu.completes();
    assert!(
        dropped.load(Ordering::SeqCst),
        "the completed frame let the owner go"
    );
}

#[test]
fn a_frame_that_records_the_draw_pixels_still_draws_after_the_lender_lets_go() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime();
    let (owner, dropped) = probe();
    let draw = {
        let lent = lent(&gpu, &runtime, wgpu::FilterMode::Nearest);
        lent.prepare(&mut runtime, 0..1, std::slice::from_ref(&owner))
            .unwrap()
        // The lender's own handles to the buffers, texture and sampler end here.
    };
    drop(owner);
    let image = gpu.render(&gpu.target(), &[&draw]);
    assert_pixel(&image, (0, 2), RED_TINTED);
    drop(draw);
    gpu.completes();
    assert!(dropped.load(Ordering::SeqCst));
}

#[test]
fn a_frame_of_another_authority_cannot_retain_the_draw() {
    let gpu = Gpu::new();
    let other = Gpu::new();
    let mut runtime = gpu.runtime();
    let lent = lent(&gpu, &runtime, wgpu::FilterMode::Nearest);
    let draw = lent.prepare(&mut runtime, 0..1, &[]).unwrap();
    let frame = other.authority.begin(&other.device).unwrap();
    assert!(draw.retain(&frame).is_err());
    frame.finish().cancel();
}
