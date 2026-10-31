//! End to end through a `ConfiguredFrameRenderer` and the production scene pipelines: Temporal
//! reconstruction of a fine-detail mipped plane at render scales 0.5, 0.67 and 0.77 of a 64x64
//! output, with the policy bias and with the bias forced to exactly 0 on the same renderer.
//!
//! The plane carries vertical stripes of 8 texels (4 dark, 4 light) in a 128 texel texture with
//! a box-filtered chain, so the stripes are 4 output pixels wide and 2 to 3 render pixels wide:
//! beyond what the render extent resolves without the finer mip. The reference is the same scene
//! through a plain 256x256 renderer (no jitter, bias 0), box averaged 4x4 in the 8 bit sRGB
//! output. Errors are mean absolute differences of sRGB bytes scaled to 0..=1 over the central
//! 32x32 pixels, which avoids the plane's edges. Shimmer is the mean absolute change of that
//! region between consecutive frames over one jitter cycle of the converged static scene.
use super::mip_bias::masked_scene;
use super::*;
use crate::compositor::{ConfiguredFrameRenderer, RendererConfiguration};
use worldrunner_render_gpu::configuration::AllocationBudget;
use worldrunner_render_plan::TexelFormat;
use worldrunner_render_post::{
    AttachmentView, FrameConfig, HdrPostSettings, PostResolve, Readback, RenderScale, ResourceRef,
    ToneMap, UpscaleMode, UpscaleSettings,
};

const OUTPUT: u32 = 64;
const HDR: TextureFormat = TextureFormat::Rgba16Float;
const SLOTS: usize = 3;
const SCALES: [u32; 3] = [500_000, 670_000, 770_000];
const ROI: std::ops::Range<usize> = 16..48;

fn stripes() -> SceneTexture {
    let dark = 13_u32;
    let light = 242_u32;
    let mut bytes = Vec::new();
    for level in 0..8_usize {
        let size = 128_usize >> level;
        for _ in 0..size {
            for x in 0..size {
                let columns = 1_usize << level;
                let lit = (x * columns..(x + 1) * columns)
                    .filter(|column| (column / 4) % 2 == 1)
                    .count();
                let lit = u32::try_from(lit).unwrap();
                let width = u32::try_from(columns).unwrap();
                let value = u8::try_from((light * lit + dark * (width - lit)) / width).unwrap();
                bytes.extend_from_slice(&[value, value, value, 255]);
            }
        }
    }
    SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Linear,
        width: 128,
        height: 128,
        mips: 8,
        bytes: bytes.into(),
    }
}

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
    fn new(context: &'a GpuContext, output: u32, scale_micro: Option<u32>) -> Self {
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
            &masked_scene(stripes()),
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
fn reference(context: &GpuContext) -> Vec<f64> {
    let bytes = Harness::new(context, 4 * OUTPUT, None).frame();
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

#[derive(Debug)]
struct Measurement {
    error: f64,
    shimmer: f64,
}

/// Converges one jitter cycle, then measures the next one.
fn measure(
    context: &GpuContext,
    scale: u32,
    forced: Option<f32>,
    reference: &[f64],
) -> Measurement {
    let mut harness = Harness::new(context, OUTPUT, Some(scale));
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

/// Per scale: the measurement with the bias forced to 0 and with the policy bias.
fn paired(context: &GpuContext) -> [(u32, Measurement, Measurement); 3] {
    let reference = reference(context);
    SCALES.map(|scale| {
        let zero = measure(context, scale, Some(0.0), &reference);
        let biased = measure(context, scale, None, &reference);
        println!("scale {scale}: bias 0 {zero:?}, policy bias {biased:?}");
        (scale, zero, biased)
    })
}

#[test]
fn the_policy_bias_lowers_the_texture_detail_error_at_each_scale() {
    let context = context();
    for (scale, zero, biased) in paired(&context) {
        assert!(
            biased.error < zero.error,
            "{scale}: error {biased:?} is not below {zero:?}"
        );
    }
}

/// The design caps the shimmer added by the bias at 10% of the bias-0 case. Measured on this
/// fixture on lavapipe the cap is EXCEEDED (see target/up3.md session 7): this test fails and
/// records the numbers for the coordinator; the policy is not tuned toward the cap.
#[test]
fn the_policy_bias_keeps_the_shimmer_within_the_design_cap() {
    let context = context();
    for (scale, zero, biased) in paired(&context) {
        assert!(
            biased.shimmer <= 1.10 * zero.shimmer,
            "{scale}: shimmer {biased:?} exceeds 1.1 x {zero:?}"
        );
    }
}

#[test]
fn a_renderer_without_reconstruction_ignores_the_forced_zero_bias() {
    let context = context();
    let frames = |forced| {
        let mut harness = Harness::new(&context, OUTPUT, None);
        harness.renderer.force_material_mip_bias(forced);
        (0..3).map(|_| harness.frame()).collect::<Vec<_>>()
    };
    assert_eq!(
        frames(None),
        frames(Some(0.0)),
        "same code twice on one device"
    );
}
