//! Passes with several colour attachments and with four samples per pixel: each target's own
//! blend and write mask, an empty mask that leaves an attachment as it was, and the resolve of
//! a draw whose edge crosses a pixel.
//!
//! Every attachment is cleared to a known value first, so what the draw left alone is as
//! visible as what it wrote.

use std::sync::Arc;

use wgpu::{ColorWrites, TextureFormat};
use worldrunner_render_gpu::tolerance_test_support::assert_code_within;
use worldrunner_render_program::error::{ProgramError, UnsupportedTarget};
use worldrunner_render_program::module::{
    StageDeclarations, StageKind, StageModule, StageProgram, StageSource,
};
use worldrunner_render_program::runtime::{
    GraphicsProgram, ImportedDraw, ImportedGeometry, ImportedVertexInput, PreparedProgramDraw,
    ProgramRuntime, TargetSignature,
};
use worldrunner_render_program::state::{ColorState, RenderState};

mod support;
use support::{Attachment, Gpu};

const WGSL: &str = "
@vertex fn vs(@location(0) position: vec2<f32>) -> @builtin(position) vec4<f32> {
    return vec4<f32>(position, 0.0, 1.0);
}
struct Two { @location(0) first: vec4<f32>, @location(1) second: vec4<f32> }
@fragment fn fs_one() -> @location(0) vec4<f32> { return vec4<f32>(0.25, 0.5, 0.75, 1.0); }
@fragment fn fs_two() -> Two {
    return Two(vec4<f32>(0.25, 0.5, 0.75, 1.0), vec4<f32>(1.0, 0.5, 0.25, 0.5));
}";

const SIZE: u32 = 4;
const RGBA8: TextureFormat = TextureFormat::Rgba8Unorm;
const DEPTH: TextureFormat = TextureFormat::Depth32Float;

/// What each colour attachment holds before the draw.
const FILLS: [[f64; 4]; 3] = [
    [0.0, 0.0, 0.0, 0.0],
    [0.2, 0.4, 0.6, 0.8],
    [0.9, 0.7, 0.5, 0.3],
];

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

fn program(runtime: &ProgramRuntime, fragment: &str) -> GraphicsProgram {
    runtime
        .graphics_program(
            &stage("vs", StageKind::Vertex),
            &stage(fragment, StageKind::Fragment),
        )
        .unwrap()
}

fn plain(write_mask: ColorWrites) -> ColorState {
    ColorState {
        blend: None,
        write_mask,
    }
}

fn state(colors: Vec<Option<ColorState>>) -> RenderState {
    let mut state = RenderState::opaque_depth();
    state.raster.cull = None;
    state.depth = None;
    state.colors = colors;
    state
}

/// Two triangles covering `[x0, y0]..[x1, y1]` of clip space.
fn rectangle(gpu: &Gpu, [x0, y0, x1, y1]: [f32; 4]) -> ImportedVertexInput {
    let corners = [[x0, y0], [x1, y0], [x1, y1], [x0, y0], [x1, y1], [x0, y1]];
    ImportedVertexInput {
        buffer: gpu.floats(wgpu::BufferUsages::VERTEX, &corners.concat()),
        stride: 8,
        step: wgpu::VertexStepMode::Vertex,
        attributes: wgpu::vertex_attr_array![0 => Float32x2].to_vec(),
    }
}

fn prepare(
    gpu: &Gpu,
    runtime: &mut ProgramRuntime,
    fragment: &str,
    state: &RenderState,
    (area, signature): ([f32; 4], TargetSignature),
) -> Result<PreparedProgramDraw, ProgramError> {
    runtime.prepare_imported_draw(
        &ImportedDraw {
            program: &program(runtime, fragment),
            state,
            vertex_inputs: &[rectangle(gpu, area)],
            bindings: &[],
            imports: &[],
            geometry: &ImportedGeometry::Vertices { first: 0, count: 6 },
            instances: 0..1,
            owners: &[],
        },
        signature,
    )
}

fn colour(values: [f64; 4]) -> wgpu::Color {
    wgpu::Color {
        r: values[0],
        g: values[1],
        b: values[2],
        a: values[3],
    }
}

/// Three RGBA8 attachments, each cleared to its fill, a draw of `fragment` with `state` over the
/// whole of them, and what each attachment holds afterwards.
fn draw_three(fragment: &str, state: &RenderState) -> Result<[Vec<u8>; 3], ProgramError> {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime();
    let signature = TargetSignature::attachments(&[RGBA8; 3], None, 1).unwrap();
    let draw = prepare(
        &gpu,
        &mut runtime,
        fragment,
        state,
        ([-1.0, -1.0, 1.0, 1.0], signature),
    )?;
    let targets: Vec<_> = (0..3).map(|_| gpu.texture(SIZE, RGBA8, 1)).collect();
    let views: Vec<_> = targets
        .iter()
        .map(|target| target.create_view(&wgpu::TextureViewDescriptor::default()))
        .collect();
    let attachments: Vec<_> = views
        .iter()
        .zip(FILLS)
        .map(|(view, fill)| Attachment {
            view,
            resolve: None,
            clear: colour(fill),
        })
        .collect();
    gpu.pass(&attachments, &[&draw]);
    Ok([
        gpu.read(&targets[0]),
        gpu.read(&targets[1]),
        gpu.read(&targets[2]),
    ])
}

fn assert_filled(image: &[u8], want: [f64; 4], what: &str) {
    for texel in image.chunks_exact(4) {
        for (channel, (&code, value)) in texel.iter().zip(want).enumerate() {
            assert_code_within(
                code,
                value * 255.0,
                0.0,
                &format!("{what} channel {channel}"),
            );
        }
    }
}

#[test]
fn an_empty_write_mask_leaves_an_attachment_the_fragment_does_not_write_as_it_was() {
    let mask = state(vec![
        Some(plain(ColorWrites::ALL)),
        Some(plain(ColorWrites::empty())),
        Some(plain(ColorWrites::empty())),
    ]);
    let [first, second, third] = draw_three("fs_one", &mask).unwrap();
    assert_filled(&first, [0.25, 0.5, 0.75, 1.0], "target 0");
    assert_filled(&second, FILLS[1], "target 1");
    assert_filled(&third, FILLS[2], "target 2");
}

#[test]
fn each_target_has_its_own_write_mask() {
    let masks = state(vec![
        Some(plain(ColorWrites::GREEN | ColorWrites::ALPHA)),
        Some(plain(ColorWrites::RED | ColorWrites::GREEN)),
        Some(plain(ColorWrites::empty())),
    ]);
    let [first, second, third] = draw_three("fs_two", &masks).unwrap();
    assert_filled(
        &first,
        [0.0, 0.5, 0.0, 1.0],
        "target 0 writes green and alpha",
    );
    assert_filled(
        &second,
        [1.0, 0.5, 0.6, 0.8],
        "target 1 writes red and green",
    );
    assert_filled(&third, FILLS[2], "target 2 writes nothing");
}

#[test]
fn each_target_has_its_own_blend() {
    let blended = state(vec![
        Some(plain(ColorWrites::ALL)),
        Some(ColorState {
            blend: Some(wgpu::BlendState::ALPHA_BLENDING),
            write_mask: ColorWrites::ALL,
        }),
        Some(plain(ColorWrites::empty())),
    ]);
    let [first, second, _] = draw_three("fs_two", &blended).unwrap();
    assert_filled(&first, [0.25, 0.5, 0.75, 1.0], "target 0 replaces");
    // Straight alpha 0.5 over the fill: colour = source * 0.5 + destination * 0.5, and
    // alpha = source alpha + destination alpha * (1 - source alpha).
    assert_filled(
        &second,
        [
            1.0 * 0.5 + 0.2 * 0.5,
            0.5 * 0.5 + 0.4 * 0.5,
            0.25 * 0.5 + 0.6 * 0.5,
            0.5 + 0.8 * 0.5,
        ],
        "target 1 blends",
    );
}

#[test]
fn a_state_that_does_not_describe_each_attachment_is_refused_by_name() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime();
    let signature = TargetSignature::attachments(&[RGBA8; 3], None, 1).unwrap();
    let area = [-1.0, -1.0, 1.0, 1.0];
    let one = state(vec![Some(plain(ColorWrites::ALL))]);
    assert_eq!(
        prepare(&gpu, &mut runtime, "fs_one", &one, (area, signature)).unwrap_err(),
        ProgramError::ColorStates {
            states: 1,
            targets: 3
        }
    );
    let unwritten = state(vec![
        Some(plain(ColorWrites::ALL)),
        None,
        Some(plain(ColorWrites::empty())),
    ]);
    assert_eq!(
        prepare(&gpu, &mut runtime, "fs_one", &unwritten, (area, signature)).unwrap_err(),
        ProgramError::UnsupportedState {
            what: "an unwritten colour target"
        }
    );
}

#[test]
fn a_signature_describes_one_to_four_attachments_of_a_power_of_two_samples() {
    let colors = |count| vec![RGBA8; count];
    assert_eq!(
        TargetSignature::attachments(&colors(1), Some(DEPTH), 1).unwrap(),
        TargetSignature::new(&colors(1), Some(DEPTH), 1).unwrap(),
        "one single-sample colour attachment is the signature the runtime's own targets have"
    );
    for count in 1..=4 {
        let signature = TargetSignature::attachments(&colors(count), Some(DEPTH), 4).unwrap();
        assert_eq!(signature.color_count(), count);
        assert_eq!(signature.samples(), 4);
    }
    assert!(matches!(
        TargetSignature::attachments(&colors(5), None, 1).unwrap_err(),
        ProgramError::UnsupportedTarget(UnsupportedTarget::ColorAttachments { count: 5 })
    ));
    assert!(matches!(
        TargetSignature::attachments(&[], None, 1).unwrap_err(),
        ProgramError::UnsupportedTarget(UnsupportedTarget::ColorAttachments { count: 0 })
    ));
}

#[test]
fn a_signature_admits_the_power_of_two_sample_counts_and_names_the_others() {
    let colors = |count| vec![RGBA8; count];
    for samples in [3, 6, 32] {
        assert!(matches!(
            TargetSignature::attachments(&colors(1), None, samples).unwrap_err(),
            ProgramError::UnsupportedTarget(UnsupportedTarget::SampleCount { samples: refused })
                if refused == samples
        ));
    }
    for samples in [2, 8, 16] {
        assert_eq!(
            TargetSignature::attachments(&colors(1), None, samples)
                .unwrap()
                .samples(),
            samples
        );
    }
    assert_ne!(
        TargetSignature::attachments(&colors(2), None, 1).unwrap(),
        TargetSignature::attachments(&colors(3), None, 1).unwrap()
    );
    assert_ne!(
        TargetSignature::attachments(&colors(1), None, 1).unwrap(),
        TargetSignature::attachments(&colors(1), None, 4).unwrap()
    );
}

#[test]
fn a_signature_hands_back_the_depth_format_it_was_given_and_refuses_the_ones_it_cannot_use() {
    for format in [
        TextureFormat::Depth16Unorm,
        TextureFormat::Depth24Plus,
        TextureFormat::Depth32Float,
    ] {
        let signature = TargetSignature::attachments(&[RGBA8], Some(format), 1).unwrap();
        assert_eq!(signature.depth(), Some(format));
    }
    assert_eq!(
        TargetSignature::attachments(&[RGBA8], None, 1)
            .unwrap()
            .depth(),
        None
    );
    assert!(matches!(
        TargetSignature::attachments(&[RGBA8], Some(TextureFormat::Depth24PlusStencil8), 1)
            .unwrap_err(),
        ProgramError::UnsupportedTarget(UnsupportedTarget::Stencil { .. })
    ));
    assert!(matches!(
        TargetSignature::attachments(&[RGBA8], Some(RGBA8), 1).unwrap_err(),
        ProgramError::UnsupportedTarget(UnsupportedTarget::NotDepth { .. })
    ));
}

#[test]
fn the_plain_draw_path_still_refuses_what_it_cannot_record() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime();
    let program = program(&runtime, "fs_one");
    let input = rectangle(&gpu, [-1.0, -1.0, 1.0, 1.0]);
    let buffer = runtime.create_buffer("quad", &[0; 48]).unwrap();
    let inputs = [worldrunner_render_program::runtime::VertexInput {
        buffer,
        stride: 8,
        step: wgpu::VertexStepMode::Vertex,
        attributes: input.attributes,
    }];
    let state = state(vec![Some(plain(ColorWrites::ALL))]);
    let packet = worldrunner_render_program::runtime::DrawPacket {
        program: &program,
        state: &state,
        vertex_inputs: &inputs,
        bindings: &[],
        geometry: worldrunner_render_program::runtime::Geometry::Vertices { first: 0, count: 6 },
    };
    let multisampled = TargetSignature::attachments(&[RGBA8], None, 4).unwrap();
    assert!(matches!(
        runtime.prepare_draw(&packet, multisampled).unwrap_err(),
        ProgramError::UnsupportedTarget(UnsupportedTarget::Multisampled { samples: 4 })
    ));
    let several = TargetSignature::attachments(&[RGBA8; 2], None, 1).unwrap();
    assert!(matches!(
        runtime.prepare_draw(&packet, several).unwrap_err(),
        ProgramError::UnsupportedTarget(UnsupportedTarget::ColorAttachments { count: 2 })
    ));
}

/// Draws `draw` into a cleared 4x colour attachment with depth and returns the bytes of the
/// single-sample texture it resolves into.
fn resolved_pixels(gpu: &Gpu, draw: &PreparedProgramDraw) -> Vec<u8> {
    let multisampled = gpu.texture(SIZE, RGBA8, 4);
    let resolved = gpu.texture(SIZE, RGBA8, 1);
    let depth = gpu.texture(SIZE, DEPTH, 4);
    let color_view = multisampled.create_view(&wgpu::TextureViewDescriptor::default());
    let resolve_view = resolved.create_view(&wgpu::TextureViewDescriptor::default());
    let depth_view = depth.create_view(&wgpu::TextureViewDescriptor::default());
    let mut frame = gpu.authority.begin(&gpu.device).unwrap();
    draw.retain(&frame).unwrap();
    {
        let mut pass = frame.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("multisampled"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &color_view,
                depth_slice: None,
                resolve_target: Some(&resolve_view),
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Discard,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Discard,
                }),
                stencil_ops: None,
            }),
            ..wgpu::RenderPassDescriptor::default()
        });
        draw.record(&mut pass);
    }
    frame.finish().submit().unwrap();
    gpu.read(&resolved)
}

/// A 4x multisampled colour attachment with depth, resolved into a single-sample texture, over
/// which a draw covers the left half of a pixel column: `x` from -1 to -0.5 of a 4 pixel row.
#[test]
fn a_four_sample_draw_resolves_a_partly_covered_pixel_to_its_coverage() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime();
    let signature = TargetSignature::attachments(&[RGBA8], Some(DEPTH), 4).unwrap();
    // Clip x from -1 to -0.5 covers the first pixel column of a four pixel row; the
    // edge at -0.5 lies on the pixel boundary. Covering -1..-0.75 covers half of it.
    let mut opaque = state(vec![Some(plain(ColorWrites::ALL))]);
    opaque.depth = Some(worldrunner_render_program::state::DepthState {
        test: wgpu::CompareFunction::LessEqual,
        write: false,
        bias: wgpu::DepthBiasState::default(),
    });
    let draw = prepare(
        &gpu,
        &mut runtime,
        "fs_one",
        &opaque,
        ([-1.0, -1.0, -0.75, 1.0], signature),
    )
    .unwrap();
    let image = resolved_pixels(&gpu, &draw);
    let texel = |x: usize, y: usize| &image[(y * SIZE as usize + x) * 4..][..4];
    // The four standard sample positions have two of four with x below 0.5 of the pixel, so
    // half the samples are covered: the resolve is the average of the draw and the clear.
    for y in 0..SIZE as usize {
        for (channel, (&code, full)) in texel(0, y).iter().zip([0.25, 0.5, 0.75, 1.0]).enumerate() {
            let want = if channel == 3 { 1.0 } else { full * 0.5 };
            assert_code_within(code, f64::from(want * 255.0f32), 0.0, "half covered pixel");
        }
        assert_eq!(
            texel(1, y),
            [0, 0, 0, 255],
            "an uncovered pixel keeps the clear"
        );
    }
}

#[test]
fn a_four_sample_signature_is_part_of_the_pipeline_identity() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime();
    let opaque = state(vec![Some(plain(ColorWrites::ALL))]);
    let area = [-1.0, -1.0, 1.0, 1.0];
    let single = TargetSignature::attachments(&[RGBA8], None, 1).unwrap();
    let multi = TargetSignature::attachments(&[RGBA8], None, 4).unwrap();
    prepare(&gpu, &mut runtime, "fs_one", &opaque, (area, single)).unwrap();
    let after_single = runtime.cached_pipelines();
    prepare(&gpu, &mut runtime, "fs_one", &opaque, (area, multi)).unwrap();
    assert_eq!(runtime.cached_pipelines(), after_single + 1);
    prepare(&gpu, &mut runtime, "fs_one", &opaque, (area, multi)).unwrap();
    assert_eq!(
        runtime.cached_pipelines(),
        after_single + 1,
        "equal state and signature share one pipeline"
    );
}
