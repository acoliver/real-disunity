//! The object-motion residual of the TAA resolve: the reference rule (history lookup moved by
//! the residual of the closest-depth texel, from its minimum-depth sample when multisampled)
//! and the GPU resolve against that reference.
//!
//! Bound: the GPU tests use the parent module's 1e-3 comparison. Residuals are exact in half
//! precision, so the `Rg16Float` texture holds the reference's input. These tests establish the
//! resolve's consumption of a residual; they do not show that a scene pass produces one.
use super::{
    BOUND, Case, assert_matches_reference, f16_bits, noisy, passthrough_pipeline_with, run,
    sample_planes, sampled, uniform_history, view, write_texture,
};
use crate::TaaSettings;
use crate::cas_reference::Image;
use crate::taa_reference::{Depth, Frame, IDENTITY};
use crate::taa_velocity::Velocity;

const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rg16Float;

/// Per-sample upload: plane `s` is laid out in columns x * 4 + s of the source.
const SHADER: &str = "
@group(0) @binding(0) var source: texture_2d<f32>;
@vertex fn vertex(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    return vec4<f32>(f32((i << 1u) & 2u) * 2.0 - 1.0, f32(i & 2u) * 2.0 - 1.0, 0.0, 1.0);
}
@fragment fn upload_velocity(
    @builtin(position) p: vec4<f32>,
    @builtin(sample_index) s: u32,
) -> @location(0) vec2<f32> {
    return textureLoad(source, vec2<i32>(i32(p.x) * 4 + i32(s), i32(p.y)), 0).rg;
}
";

/// Uploads `velocity` as an `Rg16Float` texture, multisampled when it has four planes.
pub(super) fn upload(
    (device, queue): (&wgpu::Device, &wgpu::Queue),
    velocity: &Velocity,
) -> wgpu::TextureView {
    let size = velocity.size.map(|v| u32::try_from(v).unwrap());
    if velocity.planes.len() == 1 {
        let texture = sampled(device, "TAA test velocity", size, FORMAT);
        let bytes: Vec<u8> = velocity.planes[0]
            .iter()
            .flatten()
            .flat_map(|v| f16_bits(*v).to_le_bytes())
            .collect();
        write_texture(queue, &texture, &bytes, 4);
        return view(&texture);
    }
    assert_eq!(velocity.planes.len(), 4);
    let source = sampled(
        device,
        "TAA test sample velocity source",
        [size[0] * 4, size[1]],
        wgpu::TextureFormat::Rg32Float,
    );
    let bytes: Vec<u8> = (0..velocity.planes[0].len())
        .flat_map(|i| velocity.planes.iter().map(move |plane| plane[i]))
        .flatten()
        .flat_map(f32::to_le_bytes)
        .collect();
    write_texture(queue, &source, &bytes, 8);
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("TAA test multisampled velocity"),
        size: crate::targets::extent(size),
        mip_level_count: 1,
        sample_count: 4,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("TAA test velocity upload"),
        source: wgpu::ShaderSource::Wgsl(SHADER.into()),
    });
    let pipeline = passthrough_pipeline_with(device, &module, "upload_velocity", Some(FORMAT), 4);
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::TextureView(&view(&source)),
        }],
    });
    draw_color((device, queue), &target, &pipeline, &group);
    view(&target)
}

/// Draws one fullscreen triangle into the multisampled `target`, which is not resolved.
fn draw_color(
    (device, queue): (&wgpu::Device, &wgpu::Queue),
    target: &wgpu::Texture,
    pipeline: &wgpu::RenderPipeline,
    group: &wgpu::BindGroup,
) {
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    {
        let target_view = view(target);
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("TAA test velocity upload"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &target_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, group, &[]);
        pass.draw(0..3, 0..1);
    }
    queue.submit([encoder.finish()]);
}

/// One pixel of the reference with the identity reprojection and no jitter.
fn resolve_at(
    (color, history, depth): (&Image, &Image, &Depth),
    velocity: Option<&Velocity>,
    at: [i32; 2],
) -> f64 {
    Frame {
        color,
        depth,
        history,
        reprojection: IDENTITY,
        jitter: [0.0; 2],
        history_valid: true,
        settings: TaaSettings::DEFAULT,
        samples: u32::try_from(depth.planes.len().max(1)).unwrap(),
        velocity,
        depth_history: None,
        depth_terms: [0.0; 2],
    }
    .resolve_pixel(at)[0]
}

fn gray(v: f32) -> [f32; 4] {
    [v, v, v, 1.0]
}

/// Value `0.2 + 0.1 * t` along one axis of an 8x8 image.
fn ramp(along_y: bool) -> Image {
    Image::from_fn([8, 8], |x, y| {
        gray(0.2 + 0.1 * f32::from(i16::try_from(if along_y { y } else { x }).unwrap()))
    })
}

fn flat_depth(value: f32) -> Depth {
    Depth::from_fn([8, 8], |_, _| value)
}

/// The output at ramp pixel 4 when the history is looked up `shift` pixels along the ramp,
/// where the Catmull-Rom filter reproduces a linear ramp exactly: the current value 0.6 and the
/// history value blended with the motion-adaptive weight (slope `k`, history weight 0.9).
fn ramp_expectation(shift: f64, k: f64) -> f64 {
    let weight = 0.9 * (1.0 - k * shift.abs()).clamp(0.0, 1.0);
    0.6 * (1.0 - weight) + (0.2 + 0.1 * (4.0 + shift)) * weight
}

#[test]
fn a_residual_at_the_closest_texel_moves_the_history_lookup_by_exactly_that_residual() {
    for (along_y, residual) in [(false, [-1.0 / 32.0, 0.0]), (true, [0.0, 1.0 / 16.0])] {
        let (image, depth) = (ramp(along_y), flat_depth(0.5));
        let velocity = Velocity::from_fn([8, 8], 1, |_, _, _| residual);
        let at = [4, 4];
        let moved = resolve_at((&image, &image, &depth), Some(&velocity), at);
        let shift = 8.0 * f64::from(residual[0] + residual[1]);
        assert!(
            (moved - ramp_expectation(shift, 1.5)).abs() < 1e-6,
            "axis y {along_y}: {moved}"
        );
        let unmoved = resolve_at((&image, &image, &depth), None, at);
        assert!((unmoved - 0.6).abs() < 1e-6);
        assert!((moved - unmoved).abs() > 1e-3, "{moved} {unmoved}");
    }
}

#[test]
fn the_residual_comes_from_the_closest_depth_texel_not_the_pixel_itself() {
    let image = ramp(false);
    // The object occupies columns 5 and up. Column 4 is one texel off its depth edge.
    let depth = Depth::from_fn([8, 8], |x, _| if x >= 5 { 0.25 } else { 0.5 });
    let edge = Velocity::from_fn([8, 8], 1, |x, _, _| {
        if x >= 5 { [-1.0 / 32.0, 0.0] } else { [0.0; 2] }
    });
    let uniform = Velocity::from_fn([8, 8], 1, |_, _, _| [-1.0 / 32.0, 0.0]);
    let resolve = |velocity, x| resolve_at((&image, &image, &depth), velocity, [x, 4]);
    let dilated = resolve(Some(&edge), 4);
    assert!(
        (dilated - resolve(Some(&uniform), 4)).abs() < 1e-6,
        "{dilated}"
    );
    assert!(
        (dilated - ramp_expectation(-0.25, 1.5)).abs() < 1e-6,
        "{dilated}"
    );
    assert!((dilated - resolve(None, 4)).abs() > 1e-3);
    // Column 3 has no object texel in its 3x3 neighborhood, so its own zero residual applies.
    assert!((resolve(Some(&edge), 3) - resolve(None, 3)).abs() < 1e-6);
}

#[test]
fn a_multisampled_texel_takes_the_residual_of_its_minimum_depth_sample() {
    let image = ramp(false);
    let planes: Vec<Depth> = [0.5, 0.5, 0.25, 0.5].map(flat_depth).into();
    let depth = Depth::minimum(&planes);
    let residual = |s: usize| [-f32::from(u8::try_from(s).unwrap()) / 512.0, 0.0];
    let velocity = Velocity::from_fn([8, 8], 4, |_, _, s| residual(s));
    let out = resolve_at((&image, &image, &depth), Some(&velocity), [4, 4]);
    let winner = 8.0 * f64::from(residual(2)[0]);
    assert!((out - ramp_expectation(winner, 16.0)).abs() < 1e-6, "{out}");
    let first = 8.0 * f64::from(residual(0)[0]);
    assert!((out - ramp_expectation(first, 16.0)).abs() > 1e-4);
}

#[test]
fn an_all_zero_velocity_texture_gives_the_same_output_as_none() {
    for planes in [vec![], sample_planes([17, 9])] {
        let samples = planes.len().max(1);
        let mut case = Case::new(noisy([17, 9], 0), noisy([17, 9], 5));
        if !planes.is_empty() {
            case = case.with_depth_samples(planes);
        }
        case.jitter = [0.25, -0.125];
        case.reprojection[3] = [0.05, -0.02, 0.0, 1.0];
        let without = run(&case);
        case.velocity = Some(Velocity::from_fn([17, 9], samples, |_, _, _| [0.0; 2]));
        assert_eq!(run(&case), without, "{samples} samples");
    }
}

#[test]
fn a_sub_pixel_residual_matches_the_reference_and_changes_the_output() {
    let size = [17, 9];
    let mut case = Case::new(noisy(size, 0), uniform_history(size));
    case.settings = TaaSettings::new(0.5);
    case.jitter = [0.25, -0.125];
    let without = case.expected();
    for residual in [[-1.0 / 64.0, 0.0], [1.0 / 128.0, -1.0 / 64.0]] {
        case.velocity = Some(Velocity::from_fn(size, 1, |_, _, _| residual));
        assert_matches_reference(&case, &format!("residual {residual:?}"));
        let spread: f64 = case
            .expected()
            .iter()
            .zip(&without)
            .map(|(a, b)| (a[0] - b[0]).abs())
            .sum();
        assert!(
            spread > 1e-2,
            "{residual:?} does not change the output: {spread}"
        );
    }
}

#[test]
fn a_multisampled_resolve_reads_the_velocity_of_the_minimum_depth_sample() {
    let size = [17, 9];
    let mut case =
        Case::new(noisy(size, 0), uniform_history(size)).with_depth_samples(sample_planes(size));
    case.settings = TaaSettings::new(0.5);
    let residual = |s: usize| [f32::from(u8::try_from(s + 1).unwrap()) / 1024.0, 0.0];
    case.velocity = Some(Velocity::from_fn(size, 4, |_, _, s| residual(s)));
    assert_matches_reference(&case, "4x velocity");
    let own = case.expected();
    case.velocity = Some(Velocity::from_fn(size, 4, |_, _, _| residual(0)));
    let first_sample = case.expected();
    let spread: f64 = own
        .iter()
        .zip(&first_sample)
        .map(|(a, b)| (a[0] - b[0]).abs())
        .sum();
    assert!(
        spread > 1e-2,
        "the winning sample does not matter: {spread}"
    );
}

const BAR_SIZE: [i32; 2] = [32, 8];
const BAR_WIDTH: i32 = 4;
/// Pixels the bar moves per frame; the residual is minus this in uv units of the 32 px width.
const BAR_STEP: i32 = 3;
const BAR_FRAMES: i32 = 7;
const BACKGROUND: f32 = 0.1;

fn bar_left(frame: i32) -> i32 {
    4 + BAR_STEP * frame
}

fn in_bar(x: i32, frame: i32) -> bool {
    (bar_left(frame)..bar_left(frame) + BAR_WIDTH).contains(&x)
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "the test stores f32 pixels"
)]
fn narrow(v: f64) -> f32 {
    v as f32
}

/// Mean absolute error against the true image over the strip the bar just left, over frames
/// 1 to the last, of a bar sequence with or without the matching residual.
fn bar_trail_error(with_velocity: bool) -> f64 {
    let mut history = Image::from_fn(BAR_SIZE, |_, _| gray(BACKGROUND));
    let mut total = 0.0;
    for frame in 0..BAR_FRAMES {
        let color = Image::from_fn(BAR_SIZE, |x, _| {
            gray(if in_bar(x, frame) { 1.0 } else { BACKGROUND })
        });
        let mut case = Case::new(color, history.clone());
        case.valid = frame > 0;
        case.depth = Depth::from_fn(BAR_SIZE, |x, _| if in_bar(x, frame) { 0.25 } else { 0.5 });
        if with_velocity {
            let step = f32::from(i16::try_from(BAR_STEP).unwrap()) / 32.0;
            case.velocity = Some(Velocity::from_fn(BAR_SIZE, 1, |x, _, _| {
                if in_bar(x, frame) {
                    [-step, 0.0]
                } else {
                    [0.0; 2]
                }
            }));
        }
        let out = assert_matches_reference(&case, &format!("bar frame {frame}"));
        history = Image {
            size: BAR_SIZE,
            pixels: out.iter().map(|p| p.map(narrow)).collect(),
        };
        if frame > 0 {
            let trail = (bar_left(frame) - BAR_STEP)..bar_left(frame);
            let error: f64 = (0..BAR_SIZE[1])
                .flat_map(|y| trail.clone().map(move |x| (x, y)))
                .map(|(x, y)| {
                    (out[usize::try_from(y * BAR_SIZE[0] + x).unwrap()][0] - f64::from(BACKGROUND))
                        .abs()
                })
                .sum();
            total += error / f64::from(BAR_SIZE[1] * BAR_STEP);
        }
    }
    total / f64::from(BAR_FRAMES - 1)
}

#[test]
fn matching_velocity_removes_the_trail_behind_a_moving_bar() {
    let with = bar_trail_error(true);
    let without = bar_trail_error(false);
    eprintln!("trail error with velocity {with:.6}, without {without:.6}");
    assert!(with < BOUND, "with velocity {with}");
    assert!(without > 0.1, "without velocity {without}");
}
