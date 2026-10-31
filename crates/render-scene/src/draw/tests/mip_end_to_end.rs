//! End to end through a `ConfiguredFrameRenderer` and the production scene pipelines: Temporal
//! reconstruction of a fine-detail mipped plane at render scales 0.5, 0.67 and 0.77 of a 64x64
//! output, with each candidate bias forced on the same renderer. The shipped policy is 0.
//!
//! The plane carries vertical stripes of 8 texels (4 dark, 4 light) in a 128 texel texture with
//! a box-filtered chain, so the stripes are 4 output pixels wide and 2 to 3 render pixels wide:
//! beyond what the render extent resolves without the finer mip. The reference is the same scene
//! through a plain 256x256 renderer (no jitter, bias 0), box averaged 4x4 in the 8 bit sRGB
//! output. Errors are mean absolute differences of sRGB bytes scaled to 0..=1 over the central
//! 32x32 pixels, which avoids the plane's edges. Shimmer is the mean absolute change of that
//! region between consecutive frames over one jitter cycle of the converged static scene.
use super::mip_bias::masked_scene;
use super::mip_textures::{smooth, stripes};
use super::*;
use crate::compositor::{ConfiguredFrameRenderer, RendererConfiguration};
use worldrunner_render_gpu::configuration::AllocationBudget;
use worldrunner_render_plan::post::{RenderScale, UpscaleMode, UpscaleSettings};
use worldrunner_render_post::{
    AttachmentView, FrameConfig, HdrPostSettings, PostResolve, Readback, ResourceRef, ToneMap,
};

const OUTPUT: u32 = 64;
const HDR: TextureFormat = TextureFormat::Rgba16Float;
const SLOTS: usize = 3;
const SCALES: [u32; 3] = [500_000, 670_000, 770_000];
const ROI: std::ops::Range<usize> = 16..48;

/// One renderer, its resident scene and the output it reads back.
struct Harness<'a> {
    context: &'a GpuContext,
    renderer: ConfiguredFrameRenderer,
    gpu: SceneGpu,
    pipelines: ScenePipelines,
    output: Texture,
    readback: Readback,
    submitted: usize,
}

impl<'a> Harness<'a> {
    /// `scale_micro` of `None` renders at the output extent with no resolve at all.
    fn new(
        context: &'a GpuContext,
        output: u32,
        scale_micro: Option<u32>,
        texture: SceneTexture,
    ) -> Self {
        let extent = [output; 2];
        let mut settings = RendererConfiguration::legacy(extent, COLOR_FORMAT);
        settings.frame = FrameConfig::new(extent, HDR, 1);
        settings.hdr = Some((1.0, ToneMap::Reinhard));
        if let Some(scale) = scale_micro {
            let render = RenderScale::new(scale).unwrap().extent(extent).unwrap();
            settings.frame = settings.frame.with_render_extent(render);
            settings.post = HdrPostSettings {
                resolve: PostResolve::taa(0.9),
                upscale: Some(UpscaleSettings {
                    mode: UpscaleMode::Temporal,
                    sharpness: None,
                }),
                ..HdrPostSettings::default()
            };
        }
        let device = context.device();
        let renderer =
            ConfiguredFrameRenderer::new(context, settings, AllocationBudget(1 << 28)).unwrap();
        let pipelines = ScenePipelines::new(device, HDR);
        let mut gpu = SceneGpu::upload(
            device,
            context.submission(),
            &masked_scene(texture),
            &pipelines,
        )
        .unwrap();
        gpu.post = renderer.configuration().post;
        let output_texture = device.create_texture(&TextureDescriptor {
            label: Some("mip end to end output"),
            size: Extent3d {
                width: output,
                height: output,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: COLOR_FORMAT,
            usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        Self {
            context,
            renderer,
            gpu,
            pipelines,
            output: output_texture,
            readback: Readback::new(device, FrameConfig::new(extent, COLOR_FORMAT, 1), 1 << 24)
                .unwrap(),
            submitted: 0,
        }
    }

    fn cycle(&self) -> usize {
        self.renderer
            .projection_jitter()
            .map_or(1, |jitter| usize::try_from(jitter.length()).unwrap())
    }

    /// Submits the next frame of the static scene and returns the output bytes.
    fn frame(&mut self) -> Vec<u8> {
        let device = self.context.device();
        let render = self.renderer.targets().config().size;
        let view = SceneView::new(
            CameraPose {
                eye_millimeters: [0, -1732, 1000],
                target_millimeters: [0, 0, 1000],
            },
            camera(),
            Viewport {
                width: render[0],
                height: render[1],
            },
        )
        .unwrap();
        let color = AttachmentView::new(
            ResourceRef::new("scene color"),
            self.renderer.targets().scene().texture(),
        )
        .unwrap();
        let depth = AttachmentView::new(
            ResourceRef::new("scene depth"),
            self.renderer.targets().depth().texture(),
        )
        .unwrap();
        self.renderer.select_frame_slot(self.submitted % SLOTS);
        self.submitted += 1;
        let mut encoder = self.context.submission().begin(device).unwrap();
        self.renderer
            .encoding()
            .encode_scene(
                device,
                &mut encoder,
                (&mut self.gpu, &self.pipelines),
                (&color, &depth),
                FrameUniform::unlit(&view),
            )
            .unwrap();
        let target = self.output.create_view(&TextureViewDescriptor::default());
        self.renderer.encode_output(&mut encoder, &target).unwrap();
        encoder.finish().submit().unwrap();
        self.readback
            .read(device, self.context.submission(), &self.output)
            .unwrap()
    }
}

/// The 256x256 plain render, box averaged 4x4 per channel in sRGB bytes, as 0..=1.
fn reference(context: &GpuContext, texture: SceneTexture) -> Vec<f64> {
    let bytes = Harness::new(context, 4 * OUTPUT, None, texture).frame();
    let fine = usize::try_from(4 * OUTPUT).unwrap();
    let side = usize::try_from(OUTPUT).unwrap();
    let mut image = vec![0.0; side * side * 3];
    for (index, value) in image.iter_mut().enumerate() {
        let (pixel, channel) = (index / 3, index % 3);
        let (column, row) = (pixel % side, pixel / side);
        let total: u32 = (0..16)
            .map(|tap| {
                let (x, y) = (4 * column + tap % 4, 4 * row + tap / 4);
                u32::from(bytes[(y * fine + x) * 4 + channel])
            })
            .sum();
        *value = f64::from(total) / 16.0 / 255.0;
    }
    image
}

fn roi_mean_difference(a: impl Fn(usize) -> f64, b: impl Fn(usize) -> f64) -> f64 {
    let side = usize::try_from(OUTPUT).unwrap();
    let mut total = 0.0;
    for row in ROI {
        for column in ROI {
            for channel in 0..3 {
                let index = (row * side + column) * 3 + channel;
                total += (a(index) - b(index)).abs();
            }
        }
    }
    total / f64::from(3 * 32 * 32)
}

fn byte_of(image: &[u8]) -> impl Fn(usize) -> f64 + '_ {
    |index| f64::from(image[index / 3 * 4 + index % 3]) / 255.0
}

#[derive(Clone, Copy, Debug)]
struct Measurement {
    error: f64,
    shimmer: f64,
}

/// Converges one jitter cycle, then measures the next one. `scale` of `None` is the native
/// 1.0 render scale with temporal accumulation (bias exactly 0).
fn measure(
    context: &GpuContext,
    texture: SceneTexture,
    scale: Option<u32>,
    forced: Option<f32>,
    reference: &[f64],
) -> Measurement {
    let mut harness = Harness::new(context, OUTPUT, Some(scale.unwrap_or(1_000_000)), texture);
    harness.renderer.force_material_mip_bias(forced);
    let length = harness.cycle();
    for _ in 0..length {
        harness.frame();
    }
    let frames: Vec<Vec<u8>> = (0..=length).map(|_| harness.frame()).collect();
    let count = f64::from(u32::try_from(length).unwrap());
    let error = frames[..length]
        .iter()
        .map(|frame| roi_mean_difference(byte_of(frame), |index| reference[index]))
        .sum::<f64>()
        / count;
    let shimmer = frames
        .windows(2)
        .map(|pair| roi_mean_difference(byte_of(&pair[0]), byte_of(&pair[1])))
        .sum::<f64>()
        / count;
    Measurement { error, shimmer }
}

/// Candidate biases in the order of the decision rule: most negative first. Each is an offset
/// added to `log2(s)` where `s` is the render-to-output scale ratio; the last is the bias-0 case,
/// which is the shipped policy.
const OFFSETS: [Option<f32>; 4] = [Some(-1.0), Some(-0.5), Some(0.0), None];
const NAMES: [&str; 4] = ["log2(s)-1", "log2(s)-0.5", "log2(s)", "0"];

struct Table {
    native: Measurement,
    /// Per scale, per candidate of [`OFFSETS`].
    rows: [[Measurement; 4]; 3],
}

/// `log2(s)` of the rounded render extent, independent of the policy under test.
fn log2_scale(render: [u32; 2]) -> f32 {
    let ratio = f64::from(render[0].min(render[1])) / f64::from(OUTPUT);
    #[expect(
        clippy::cast_possible_truncation,
        reason = "log2 of a ratio in [0.5, 1] fits f32"
    )]
    let log2 = ratio.log2() as f32;
    log2
}

fn table(context: &GpuContext, fixture: fn() -> SceneTexture, name: &str) -> Table {
    let reference = reference(context, fixture());
    let native = measure(context, fixture(), None, None, &reference);
    println!("fixture {name}: native TAA (scale 1.0, bias 0) {native:?}");
    println!("scale | candidate | bias | error | shimmer | shimmer/native | error/bias0 error");
    let rows = SCALES.map(|scale| {
        let render = RenderScale::new(scale)
            .unwrap()
            .extent([OUTPUT; 2])
            .unwrap();
        let log2_s = log2_scale(render);
        let row = OFFSETS.map(|offset| {
            let bias = offset.map_or(0.0, |offset| log2_s + offset);
            measure(context, fixture(), Some(scale), Some(bias), &reference)
        });
        for (index, measurement) in row.iter().enumerate() {
            let bias = OFFSETS[index].map_or(0.0, |offset| log2_s + offset);
            println!(
                "{scale} | {} | {bias:.3} | {:.4} | {:.6} | {:.3} | {:.3}",
                NAMES[index],
                measurement.error,
                measurement.shimmer,
                measurement.shimmer / native.shimmer,
                measurement.error / row[3].error,
            );
        }
        row
    });
    Table { native, rows }
}

/// Index of the bias-0 row entry in [`Table::rows`], the shipped policy.
const SHIPPED: usize = 3;
/// The native TAA shimmer limit as a multiple, from the decision rule.
const SHIMMER_LIMIT: f64 = 1.10;

/// Every nonzero candidate lowers the detail error against bias 0 at every scale. Measured error
/// over the bias-0 error is 0.58 to 0.91 on lavapipe and on the AMD Radeon 780M for the stripes
/// (target/up3.md), so a strict comparison keeps at least 9% of margin against the 8 to 10%
/// spread between the two devices.
///
/// The shipped policy (bias 0) stays within 1.10 times the native TAA shimmer at every scale. The
/// stripes measured at most 0.76 on lavapipe and 0.83 on the Radeon, a margin of at least 24%.
fn assert_policy_bounds(table: &Table, name: &str) {
    for (scale, row) in SCALES.iter().zip(&table.rows) {
        for candidate in 0..SHIPPED {
            assert!(
                row[candidate].error < row[SHIPPED].error,
                "{name} {scale} {}: error {:?} is not below bias 0 {:?}",
                NAMES[candidate],
                row[candidate],
                row[SHIPPED]
            );
        }
        assert!(
            row[SHIPPED].shimmer <= SHIMMER_LIMIT * table.native.shimmer,
            "{name} {scale}: shipped shimmer {:?} exceeds {SHIMMER_LIMIT} x native {:?}",
            row[SHIPPED],
            table.native
        );
    }
}

#[test]
fn the_candidates_lower_the_error_and_the_shipped_zero_keeps_native_shimmer_on_the_stripes() {
    assert_policy_bounds(&table(&context(), stripes, "stripes"), "stripes");
}

#[test]
fn the_candidates_lower_the_error_and_the_shipped_zero_keeps_native_shimmer_on_the_smooth_texture()
{
    assert_policy_bounds(&table(&context(), smooth, "smooth"), "smooth");
}

#[test]
fn a_renderer_without_reconstruction_ignores_the_forced_zero_bias() {
    let context = context();
    let frames = |forced| {
        let mut harness = Harness::new(&context, OUTPUT, None, stripes());
        harness.renderer.force_material_mip_bias(forced);
        (0..3).map(|_| harness.frame()).collect::<Vec<_>>()
    };
    assert_eq!(
        frames(None),
        frames(Some(0.0)),
        "same code twice on one device"
    );
}
