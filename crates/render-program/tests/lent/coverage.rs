//! Alpha to coverage: a fragment's alpha decides how many of a pixel's samples it covers.
//!
//! The pass is 4x multisampled with an `Rgba16Float` colour attachment that resolves into a
//! single-sample one. The fragment writes white with the alpha under test, and the target starts
//! transparent black, so a resolved pixel's red is the fraction of its four samples the fragment
//! covered. That fraction is a multiple of 0.25, which `Rgba16Float` holds exactly and the resolve
//! averages without rounding, so the covered count reads back as an integer.

use std::sync::Arc;

use worldrunner_render_program::error::ProgramError;
use worldrunner_render_program::module::{
    StageDeclarations, StageKind, StageModule, StageProgram, StageSource,
};
use worldrunner_render_program::runtime::{
    DrawPacket, Geometry, GraphicsProgram, ImportedDraw, ImportedGeometry, PipelineDraw,
    PreparedProgramDraw, ProgramRuntime, TargetSignature, VertexLayoutSpec,
};
use worldrunner_render_program::state::RenderState;

use crate::common::SIZE;
use crate::support::{Attachment, Gpu};

const WGSL: &str = "
override alpha: f32 = 1.0;
@vertex fn vs(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    var xs = array<f32, 6>(-1.0, 1.0, 1.0, -1.0, 1.0, -1.0);
    var ys = array<f32, 6>(-1.0, -1.0, 1.0, -1.0, 1.0, 1.0);
    return vec4<f32>(xs[index], ys[index], 0.0, 1.0);
}
@fragment fn fs() -> @location(0) vec4<f32> { return vec4<f32>(1.0, 1.0, 1.0, alpha); }";

const HALF: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
const SAMPLES: u32 = 4;
const SAMPLES_F32: f32 = 4.0;

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

/// The program that writes fragments of `alpha`.
fn program(runtime: &ProgramRuntime, alpha: f32) -> GraphicsProgram {
    let fragment = stage("fs", StageKind::Fragment)
        .with_overrides(&[("alpha", f64::from(alpha))])
        .unwrap();
    runtime
        .graphics_program(&stage("vs", StageKind::Vertex), &fragment)
        .unwrap()
}

fn state(alpha_to_coverage: bool) -> RenderState {
    let mut state = RenderState::opaque_depth();
    state.raster.cull = None;
    state.raster.alpha_to_coverage = alpha_to_coverage;
    state.depth = None;
    state
}

fn signature(samples: u32) -> TargetSignature {
    TargetSignature::attachments(&[HALF], None, samples).unwrap()
}

fn prepare(
    runtime: &mut ProgramRuntime,
    program: &GraphicsProgram,
    state: &RenderState,
    samples: u32,
) -> Result<PreparedProgramDraw, ProgramError> {
    runtime.prepare_imported_draw(
        &ImportedDraw {
            program,
            state,
            vertex_inputs: &[],
            bindings: &[],
            imports: &[],
            sets: &[],
            geometry: &ImportedGeometry::Vertices { first: 0, count: 6 },
            instances: 0..1,
            owners: &[],
        },
        signature(samples),
    )
}

/// The `f32` an `Rgba16Float` channel holds, for the values a quarter-step count produces.
fn half(bits: [u8; 2]) -> f32 {
    let bits = u16::from_le_bytes(bits);
    let (exponent, fraction) = ((bits >> 10) & 0x1f, f32::from(bits & 0x3ff));
    let magnitude = match exponent {
        0 => fraction * 2f32.powi(-24),
        0x1f => panic!("{bits:#06x} is not finite"),
        _ => (1.0 + fraction / 1024.0) * 2f32.powi(i32::from(exponent) - 15),
    };
    if bits >> 15 == 1 {
        -magnitude
    } else {
        magnitude
    }
}

/// How many of its four samples each pixel's fragment covered, row by row, after one pass of
/// `draw` into a 4x target resolved into a single-sample one.
fn covered(gpu: &Gpu, draw: &PreparedProgramDraw) -> Vec<f32> {
    let multisampled = gpu.texture(SIZE, HALF, SAMPLES);
    let resolved = gpu.texture(SIZE, HALF, 1);
    let source = multisampled.create_view(&wgpu::TextureViewDescriptor::default());
    let target = resolved.create_view(&wgpu::TextureViewDescriptor::default());
    gpu.pass(
        &[Attachment {
            view: &source,
            resolve: Some(&target),
            clear: wgpu::Color::TRANSPARENT,
        }],
        &[draw],
    );
    gpu.read(&resolved)
        .chunks_exact(8)
        .map(|texel| half([texel[0], texel[1]]) * SAMPLES_F32)
        .collect()
}

/// Every pixel of the full-target quad is covered by the same count: the alpha, in quarters. The
/// alphas are the multiples of a quarter, where a count is determinate whatever the pattern a
/// device spreads covered samples in; both adapters give exactly these counts.
#[test]
fn alpha_to_coverage_drops_samples_by_the_fragment_alpha_at_four_samples() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime();
    let pixels = (SIZE * SIZE) as usize;
    for (alpha, want) in [(0.0, 0.0), (0.25, 1.0), (0.5, 2.0), (0.75, 3.0), (1.0, 4.0)] {
        let program = program(&runtime, alpha);
        let on = prepare(&mut runtime, &program, &state(true), SAMPLES).unwrap();
        assert_eq!(covered(&gpu, &on), vec![want; pixels], "alpha {alpha}");
        // Without the state the alpha is only a colour: every sample of every pixel is covered.
        let off = prepare(&mut runtime, &program, &state(false), SAMPLES).unwrap();
        assert_eq!(covered(&gpu, &off), vec![4.0; pixels], "alpha {alpha}, off");
    }
    // The state is part of the pipeline: five programs, each with and without it.
    assert_eq!(runtime.stats().pipelines_built, 10);
}

#[test]
fn alpha_to_coverage_at_one_sample_is_refused_by_name() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime();
    let program = program(&runtime, 0.5);
    let refused = ProgramError::AlphaToCoverageNeedsSamples { samples: 1 };
    let state = state(true);

    let imported = prepare(&mut runtime, &program, &state, 1);
    assert_eq!(imported.err(), Some(refused.clone()));
    let layout = VertexLayoutSpec::new(Vec::new());
    let pipeline = runtime.prepare_pipeline(
        &PipelineDraw {
            program: &program,
            state: &state,
            layout: &layout,
            bindings: &[],
            imports: &[],
            sets: &[],
            owners: &[],
        },
        signature(1),
    );
    assert_eq!(pipeline.err(), Some(refused.clone()));
    let plain = runtime.prepare_draw(
        &DrawPacket {
            program: &program,
            state: &state,
            vertex_inputs: &[],
            bindings: &[],
            geometry: Geometry::Vertices { first: 0, count: 6 },
        },
        signature(1),
    );
    assert_eq!(plain.err(), Some(refused));
    // Refused before anything was built.
    assert_eq!(runtime.stats().pipelines_built, 0);
    // The same state is fine for the pass that has samples to drop.
    assert!(prepare(&mut runtime, &program, &state, SAMPLES).is_ok());
}
