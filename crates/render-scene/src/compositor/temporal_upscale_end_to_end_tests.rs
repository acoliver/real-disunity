//! Reduced temporal reconstruction end to end through a `ConfiguredFrameRenderer`: TAA with
//! `UpscaleMode::Temporal` at render scales 0.5, 0.67 and 0.77 of a 64x64 output, the scene of
//! `taa_end_to_end_tests` rasterised into the render-extent targets, the jitter derived by
//! `post::jitter` from both extents, and the output read back in 8 bit sRGB.
//!
//! The reference is the one of `taa_end_to_end_tests`: the same scene pipeline rasterised raw at
//! 4x the output width and height (256x256, no jitter, no TAA, no MSAA), box averaged in linear
//! HDR and tone mapped by the CPU curve. The comparison is the mean absolute error of the 8 bit
//! sRGB bytes scaled to 0..=1, over the whole image.
//!
//! "Reset every frame" is the same configuration with the history discarded before each frame:
//! the output is then the reconstruction of one jittered frame at the first phase of the
//! sequence, without history. Static scene, zero camera motion, lavapipe only.
use super::taa_end_to_end_tests::{
    Image, OUTPUT, SIZE, Scene, Target, context, decode_output, frame, mean_abs_error,
    supersampled, texture,
};
use crate::{
    compositor::{ConfiguredFrameRenderer, RendererConfiguration},
    device::GpuContext,
};
use worldrunner_render_gpu::configuration::AllocationBudget;
use worldrunner_render_post::{
    FrameConfig, FrameError, HdrPostSettings, PostResolve, ProjectionJitter, Readback, RenderScale,
    ToneMap, UpscaleMode, UpscaleSettings,
};

const OUTPUT_EXTENT: [u32; 2] = [SIZE, SIZE];
const SLOTS: usize = 3;
/// The design bound: temporal error over reset-every-frame error.
const RATIO_BOUND: f64 = 0.90;
const SCALES: [u32; 3] = [500_000, 670_000, 770_000];

struct Rig<'a> {
    context: &'a GpuContext,
    renderer: ConfiguredFrameRenderer,
    scene: Scene,
    output: wgpu::Texture,
    readback: Readback,
    render: [u32; 2],
    expected: ProjectionJitter,
    submitted: usize,
}

impl<'a> Rig<'a> {
    fn new(context: &'a GpuContext, scale_micro: u32) -> Self {
        let render = RenderScale::new(scale_micro)
            .unwrap()
            .extent(OUTPUT_EXTENT)
            .unwrap();
        let frame_config = FrameConfig::new(OUTPUT_EXTENT, wgpu::TextureFormat::Rgba16Float, 1)
            .with_render_extent(render);
        let post = HdrPostSettings {
            resolve: PostResolve::taa(0.9),
            upscale: Some(UpscaleSettings {
                mode: UpscaleMode::Temporal,
                sharpness: None,
            }),
            ..HdrPostSettings::default()
        };
        let mut settings = RendererConfiguration::legacy(OUTPUT_EXTENT, OUTPUT);
        settings.frame = frame_config;
        settings.hdr = Some((1.0, ToneMap::Reinhard));
        settings.post = post;
        let renderer =
            ConfiguredFrameRenderer::new(context, settings, AllocationBudget(1 << 28)).unwrap();
        let device = context.device();
        Self {
            context,
            renderer,
            scene: Scene::new(device, 1),
            output: texture(device, SIZE, OUTPUT),
            readback: Readback::new(device, FrameConfig::new(OUTPUT_EXTENT, OUTPUT, 1), 1 << 20)
                .unwrap(),
            render,
            expected: ProjectionJitter::for_extents(render, OUTPUT_EXTENT).unwrap(),
            submitted: 0,
        }
    }

    fn cycle(&self) -> usize {
        usize::try_from(self.expected.length()).unwrap()
    }

    /// Submits one frame of the static scene in the next slot and returns the output bytes.
    fn render(&mut self) -> Vec<u8> {
        self.render_in_slot(self.submitted % SLOTS)
    }

    fn render_in_slot(&mut self, slot: usize) -> Vec<u8> {
        self.render_from(slot, 0)
    }

    /// One frame in `slot` from a camera at `eye_x` millimetres.
    fn render_from(&mut self, slot: usize, eye_x: i64) -> Vec<u8> {
        let device = self.context.device();
        let renderer = self.renderer.renderer_mut();
        renderer.select_frame_slot(slot);
        self.submitted += 1;
        let prepared = renderer
            .prepare_uniform(frame(eye_x, self.render[0]))
            .unwrap();
        let view = self
            .output
            .create_view(&wgpu::TextureViewDescriptor::default());
        let scene = &self.scene;
        let transaction = self
            .context
            .submission()
            .prepare(device, |encoder, _| {
                let targets = renderer.targets();
                let target = Target {
                    color: targets.scene(),
                    resolve: None,
                    depth: targets.depth(),
                };
                scene.draw(device, encoder, &target, prepared.view_projection);
                renderer.encode_frame(device, encoder, (&[], &[]), &mut [], (prepared, None))?;
                renderer.encode_output(encoder, &view).unwrap();
                Ok::<_, FrameError>(())
            })
            .unwrap();
        transaction.submit().unwrap();
        self.readback
            .read(device, self.context.submission(), &self.output)
            .unwrap()
    }

    /// The output of a frame without history, whatever came before.
    fn reset_frame(&mut self) -> Vec<u8> {
        self.renderer.reset_temporal_history();
        self.render_in_slot(0)
    }
}

fn error(bytes: &[u8], reference: &Image) -> f64 {
    mean_abs_error(&decode_output(bytes), reference)
}

#[test]
fn the_renderer_builds_the_output_extent_and_derives_the_jitter_from_both_extents() {
    let context = context();
    for scale in SCALES {
        let rig = Rig::new(&context, scale);
        let config = rig.renderer.targets().config();
        assert_eq!(config.size, rig.render, "{scale}");
        assert_eq!(config.output_extent(), OUTPUT_EXTENT, "{scale}");
        assert!(rig.render[0] < SIZE, "{scale}: a reduced render extent");
        let selected = rig.renderer.projection_jitter();
        assert_eq!(selected, Some(rig.expected), "{scale}");
        assert!(rig.expected.length() > ProjectionJitter::DEFAULT_LENGTH);
    }
}

#[test]
fn every_frame_of_two_jitter_cycles_is_an_output_sized_fully_written_image() {
    let context = context();
    for scale in SCALES {
        let mut rig = Rig::new(&context, scale);
        assert!(!rig.renderer.has_temporal_history());
        for index in 0..2 * rig.cycle() {
            let bytes = rig.render();
            assert_eq!(
                bytes.len(),
                4 * (SIZE * SIZE) as usize,
                "{scale} frame {index}"
            );
            // Alpha is written 255 by the output stage; an untouched texel stays 0.
            assert!(
                bytes.chunks_exact(4).all(|texel| texel[3] == 255),
                "{scale} frame {index} has an unwritten pixel"
            );
            let lit = bytes
                .chunks_exact(4)
                .filter(|t| t[..3] != [0, 0, 0])
                .count();
            assert!(
                lit > bytes.len() / 8,
                "{scale} frame {index}: {lit} lit pixels"
            );
        }
        assert!(rig.renderer.has_temporal_history(), "{scale}");
    }
}

#[test]
fn the_first_frame_after_a_reset_equals_the_first_frame_of_a_fresh_renderer() {
    let context = context();
    for scale in SCALES {
        let fresh = Rig::new(&context, scale).render_in_slot(0);
        let mut warm = Rig::new(&context, scale);
        for _ in 0..warm.cycle() {
            warm.render();
        }
        let after_reset = warm.reset_frame();
        assert_eq!(after_reset, fresh, "{scale}: same code twice on one device");
    }
}

fn measured(context: &GpuContext, scale: u32) -> (f64, f64) {
    let reference = supersampled(context, 0);
    let mut rig = Rig::new(context, scale);
    let length = rig.cycle();
    for _ in 0..length {
        rig.render();
    }
    let temporal = (0..length)
        .map(|_| error(&rig.render(), &reference))
        .sum::<f64>()
        / f64::from(u32::try_from(length).unwrap());
    let reset = error(&rig.reset_frame(), &reference);
    (temporal, reset)
}

#[test]
fn the_static_error_against_reset_every_frame_meets_the_design_ratio_at_each_scale() {
    let context = context();
    for scale in SCALES {
        let (temporal, reset) = measured(&context, scale);
        println!(
            "scale {scale}: temporal {temporal:.5} reset {reset:.5} ratio {:.3}",
            temporal / reset
        );
        assert!(
            temporal <= RATIO_BOUND * reset,
            "{scale}: temporal {temporal} reset {reset}"
        );
    }
}

/// How a pan run treats the history.
#[derive(Clone, Copy)]
enum PanRun {
    /// History kept, camera term uploaded.
    WithCamera,
    /// History kept, the camera upload forced to "no camera" (the object residual only).
    WithoutCamera,
    /// History reset before each frame: the spatial reconstruction alone.
    ResetEachFrame,
}

/// Mean error over the frames after `WARM_FRAMES`, to the reference of each frame's camera, of a
/// camera that moves `step_mm` per frame, for [`PanRun`] with the camera term, without it and
/// with the history reset before each frame. All three run on the same device.
fn pan_errors(context: &GpuContext, scale: u32, step_mm: i64) -> [f64; 3] {
    let eyes: Vec<i64> = (0..PAN_FRAMES)
        .map(|index| i64::try_from(index).unwrap() * step_mm)
        .collect();
    let references: Vec<Image> = eyes.iter().map(|&eye| supersampled(context, eye)).collect();
    let run = |mode: PanRun| {
        let mut rig = Rig::new(context, scale);
        rig.renderer
            .renderer_mut()
            .force_no_camera_term(matches!(mode, PanRun::WithoutCamera));
        let errors: Vec<f64> = eyes
            .iter()
            .zip(&references)
            .map(|(&eye, reference)| {
                if matches!(mode, PanRun::ResetEachFrame) {
                    rig.renderer.reset_temporal_history();
                }
                let slot = rig.submitted % SLOTS;
                error(&rig.render_from(slot, eye), reference)
            })
            .collect();
        let late = &errors[WARM_FRAMES..];
        late.iter().sum::<f64>() / f64::from(u32::try_from(late.len()).unwrap())
    };
    [
        run(PanRun::WithCamera),
        run(PanRun::WithoutCamera),
        run(PanRun::ResetEachFrame),
    ]
}

const PAN_FRAMES: usize = 40;
const WARM_FRAMES: usize = 24;
/// Camera steps per frame in millimetres, both directions. At this distance and field of view
/// one output pixel is about 180 mm, so a step of 60 mm is about a third of a pixel, where the
/// history of the wrong (zero) motion is visibly misaligned.
const PAN_STEPS: [i64; 2] = [60, -60];
/// With the camera term the error is at most this fraction of the error without it.
const CAMERA_OVER_NONE_BOUND: f64 = 0.95;

/// A camera pan with the temporal stage stays aligned with the native-resolution reference: the
/// error with the camera term is clearly below the error of the same renderer with the camera
/// upload forced to none, on one device, and no worse than the spatial reconstruction alone.
///
/// Measured on lavapipe at 60 mm steps, both signs, three scales: with over without 0.72 to
/// 0.84, with over reset 0.62 to 0.81. With the upload forced to none in the with-camera run too
/// the ratio is 1.000 and the test fails. The 0.95 limit leaves 0.11 over the worst measured
/// ratio, more than the 8 to 10% platform spread. Larger steps (90 mm) do not widen the gap, the
/// motion weight then discards the history.
#[test]
fn a_camera_pan_stays_aligned_with_the_native_resolution_reference() {
    let context = context();
    for scale in SCALES {
        for step in PAN_STEPS {
            let [with, without, reset] = pan_errors(&context, scale, step);
            println!(
                "scale {scale} step {step}: with {with:.5} without {without:.5} reset {reset:.5} \
                 with/without {:.3} with/reset {:.3}",
                with / without,
                with / reset
            );
            assert!(with.is_finite() && without.is_finite() && reset.is_finite());
            assert!(
                with <= CAMERA_OVER_NONE_BOUND * without,
                "{scale} {step}: with {with} without {without}"
            );
            assert!(
                with <= PAN_RATIO_BOUND * reset,
                "{scale} {step}: with {with} reset {reset}"
            );
        }
    }
}

const PAN_RATIO_BOUND: f64 = 1.0;
