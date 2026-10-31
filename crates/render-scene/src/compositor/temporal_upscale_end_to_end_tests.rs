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
        let mut renderer =
            ConfiguredFrameRenderer::new(context, settings, AllocationBudget(1 << 28)).unwrap();
        let jitter = crate::post::jitter(&post, frame_config, None).unwrap();
        renderer
            .renderer_mut()
            .set_projection_jitter(jitter)
            .unwrap();
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
        let device = self.context.device();
        let renderer = self.renderer.renderer_mut();
        renderer.select_frame_slot(slot);
        self.submitted += 1;
        let prepared = renderer.prepare_uniform(frame(0, self.render[0])).unwrap();
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
        self.renderer.renderer_mut().reset_temporal_history();
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
        let mut rig = Rig::new(&context, scale);
        let config = rig.renderer.targets().config();
        assert_eq!(config.size, rig.render, "{scale}");
        assert_eq!(config.output_extent(), OUTPUT_EXTENT, "{scale}");
        assert!(rig.render[0] < SIZE, "{scale}: a reduced render extent");
        let selected = rig.renderer.renderer_mut().projection_jitter();
        assert_eq!(selected, Some(rig.expected), "{scale}");
        assert!(rig.expected.length() > ProjectionJitter::DEFAULT_LENGTH);
    }
}

#[test]
fn every_frame_of_two_jitter_cycles_is_an_output_sized_fully_written_image() {
    let context = context();
    for scale in SCALES {
        let mut rig = Rig::new(&context, scale);
        assert!(!rig.renderer.renderer_mut().has_temporal_history());
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
        assert!(
            rig.renderer.renderer_mut().has_temporal_history(),
            "{scale}"
        );
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
