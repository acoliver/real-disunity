//! The temporal upscaler qualification matrix runner (issue #256, section 7).
//!
//! `qualification_matrix` is an explicit measurement, not a CI check. It renders the held-out
//! composite scene through `ConfiguredFrameRenderer` for each output extent, render scale and
//! comparison mode, compares the display-linear output with a supersampled reference and prints
//! one table row per cell with the whole-frame GPU timestamp envelope. It asserts only that
//! every printed number is finite. Run it as
//! `cargo test -p worldrunner-render-scene qualification_matrix -- --ignored --nocapture
//! --test-threads=1`.
//!
//! Modes: M1 bilinear, M2 EASU, M3 reduced TAA plus EASU, M4 Temporal (all with RCAS off and no
//! sharpening), M5 full-resolution 1x TAA with no upscale, unsharpened. Defaults are the full
//! matrix; the `RD_QUAL_*` variables shrink it (`OUTPUTS` as `WxH,...`, `SCALES` in millionths,
//! `MODES` as `M1,...`, `PAN` in output pixels per frame, `FRAMES`, `WARM`, `FACTOR`, and
//! `TIMING_WARM`, `TIMING_FRAMES`). Lavapipe numbers check that the runner works; they are not
//! quality or cost evidence for any other device.
use super::{
    qualification_reference::{decode_display, edge_mask, reference},
    qualification_scene::{CompositeScene, eye_travel_mm, frame, target},
    qualification_timing::{Envelope, FrameEnvelope, MEASURED_FRAMES, WARM_FRAMES, summarize},
    taa_end_to_end_tests::OUTPUT,
};
use crate::{
    compositor::{ConfiguredFrameRenderer, RendererConfiguration},
    device::{GpuContext, REQUIRED_FEATURES},
    quality_metrics::{
        EdgeProfile, Pixel, edge_profile, linear_mae, linear_rmse, luma_row,
        motion_compensated_instability, srgb_byte_mae,
    },
};
use std::str::FromStr;
use worldrunner_render_gpu::configuration::AllocationBudget;
use worldrunner_render_plan::post::{RenderScale, UpscaleMode, UpscaleSettings};
use worldrunner_render_post::{
    FrameConfig, FrameError, HdrPostSettings, PostResolve, Readback, ToneMap,
};

const SLOTS: usize = 3;
const EDGE_THRESHOLD: f32 = 0.05;
const EDGE_WINDOW: usize = 6;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    M1,
    M2,
    M3,
    M4,
    M5,
}

impl FromStr for Mode {
    type Err = String;
    fn from_str(text: &str) -> Result<Self, String> {
        match text {
            "M1" => Ok(Self::M1),
            "M2" => Ok(Self::M2),
            "M3" => Ok(Self::M3),
            "M4" => Ok(Self::M4),
            "M5" => Ok(Self::M5),
            other => Err(format!("unknown mode {other}")),
        }
    }
}

/// The renderer configuration and the raster extent of one cell.
fn settings(mode: Mode, output: [u32; 2], scale_micro: u32) -> (RendererConfiguration, [u32; 2]) {
    let upscale = |mode| {
        Some(UpscaleSettings {
            mode,
            sharpness: None,
        })
    };
    let reduced = || {
        RenderScale::new(scale_micro)
            .unwrap()
            .extent(output)
            .unwrap()
    };
    let (render, post) = match mode {
        Mode::M5 => (output, PostResolve::taa(0.9)).into_post(None),
        Mode::M1 => (reduced(), PostResolve::None).into_post(upscale(UpscaleMode::Bilinear)),
        Mode::M2 => (reduced(), PostResolve::None).into_post(upscale(UpscaleMode::Easu)),
        Mode::M3 => (reduced(), PostResolve::taa(0.9)).into_post(upscale(UpscaleMode::Easu)),
        Mode::M4 => (reduced(), PostResolve::taa(0.9)).into_post(upscale(UpscaleMode::Temporal)),
    };
    let mut configuration = RendererConfiguration::legacy(output, OUTPUT);
    configuration.frame =
        FrameConfig::new(output, wgpu::TextureFormat::Rgba16Float, 1).with_render_extent(render);
    configuration.hdr = Some((1.0, ToneMap::Reinhard));
    configuration.post = post;
    (configuration, render)
}

trait IntoPost {
    fn into_post(self, upscale: Option<UpscaleSettings>) -> ([u32; 2], HdrPostSettings);
}

impl IntoPost for ([u32; 2], PostResolve) {
    fn into_post(self, upscale: Option<UpscaleSettings>) -> ([u32; 2], HdrPostSettings) {
        (
            self.0,
            HdrPostSettings {
                resolve: self.1,
                upscale,
                ..HdrPostSettings::default()
            },
        )
    }
}

struct Config {
    outputs: Vec<[u32; 2]>,
    scales: Vec<u32>,
    modes: Vec<Mode>,
    pans: Vec<f64>,
    frames: usize,
    warm: usize,
    factor: u32,
    timing: (usize, usize),
}

fn env_list<T: FromStr>(name: &str, default: &str) -> Vec<T> {
    let text = std::env::var(format!("RD_QUAL_{name}")).unwrap_or_else(|_| default.to_owned());
    text.split(',')
        .map(|item| {
            item.parse()
                .unwrap_or_else(|_| panic!("RD_QUAL_{name}: bad item {item}"))
        })
        .collect()
}

fn env_one<T: FromStr>(name: &str, default: T) -> T {
    std::env::var(format!("RD_QUAL_{name}")).map_or(default, |text| {
        text.parse()
            .unwrap_or_else(|_| panic!("RD_QUAL_{name}: bad value {text}"))
    })
}

/// `RD_QUAL_OUTPUTS` as extents, each written `WxH`.
fn env_outputs(default: &str) -> Vec<[u32; 2]> {
    env_list::<String>("OUTPUTS", default)
        .iter()
        .map(|text| {
            let (w, h) = text.split_once('x').expect("WxH");
            [w.parse().expect("width"), h.parse().expect("height")]
        })
        .collect()
}

impl Config {
    fn from_env() -> Self {
        Self {
            outputs: env_outputs("1920x1080,1452x921"),
            scales: env_list("SCALES", "500000,670000,770000"),
            modes: env_list("MODES", "M1,M2,M3,M4,M5"),
            pans: env_list("PAN", "0,0.3"),
            frames: env_one("FRAMES", 4),
            warm: env_one("WARM", 64),
            factor: env_one("FACTOR", 4),
            timing: (
                env_one("TIMING_WARM", WARM_FRAMES),
                env_one("TIMING_FRAMES", MEASURED_FRAMES),
            ),
        }
    }
}

/// One renderer with the scene, an output texture and an optional whole-frame envelope.
struct Harness<'a> {
    context: &'a GpuContext,
    scene: &'a CompositeScene,
    renderer: ConfiguredFrameRenderer,
    output: wgpu::Texture,
    readback: Readback,
    render: [u32; 2],
    envelope: Option<FrameEnvelope>,
    submitted: usize,
    /// Whether each frame begins a pass-level timing record (`stage_timings` turns it on).
    timed: bool,
}

impl<'a> Harness<'a> {
    fn new(
        context: &'a GpuContext,
        scene: &'a CompositeScene,
        cell: (Mode, [u32; 2], u32),
    ) -> Self {
        let (configuration, render) = settings(cell.0, cell.1, cell.2);
        let renderer =
            ConfiguredFrameRenderer::new(context, configuration, AllocationBudget(1 << 32))
                .unwrap();
        let device = context.device();
        let max_bytes = 4 * u64::from(cell.1[0]) * u64::from(cell.1[1]) + (1 << 20);
        Self {
            context,
            scene,
            renderer,
            output: target(device, cell.1, OUTPUT),
            readback: Readback::new(device, FrameConfig::new(cell.1, OUTPUT, 1), max_bytes)
                .unwrap(),
            render,
            envelope: FrameEnvelope::new(context),
            submitted: 0,
            timed: false,
        }
    }

    /// Submits one frame from a camera at `eye_x` and returns its GPU envelope when measurable.
    fn frame(&mut self, eye_x: i64) -> Option<f64> {
        let device = self.context.device();
        let renderer = self.renderer.renderer_mut();
        renderer.select_frame_slot(self.submitted % SLOTS);
        self.submitted += 1;
        let timing_id = u64::try_from(self.submitted).ok().filter(|_| self.timed);
        let prepared = renderer.prepare_uniform(frame(eye_x, self.render)).unwrap();
        let view = self
            .output
            .create_view(&wgpu::TextureViewDescriptor::default());
        let (scene, envelope) = (self.scene, self.envelope.as_ref());
        let transaction = self
            .context
            .submission()
            .prepare(device, |encoder, _| {
                if let Some(id) = timing_id {
                    renderer.begin_timing(id).unwrap();
                }
                if let Some(e) = envelope {
                    e.begin(encoder);
                }
                let targets = renderer.targets();
                scene.draw(
                    device,
                    encoder,
                    (targets.scene(), targets.depth()),
                    prepared.view_projection,
                );
                renderer.encode_frame(device, encoder, (&[], &[]), &mut [], (prepared, None))?;
                renderer.encode_output(encoder, &view).unwrap();
                if let Some(e) = envelope {
                    e.end(encoder);
                }
                Ok::<_, FrameError>(())
            })
            .unwrap();
        transaction.submit().unwrap();
        envelope.and_then(|e| e.read(device))
    }

    fn display(&mut self) -> Vec<Pixel> {
        let bytes = self
            .readback
            .read(
                self.context.device(),
                self.context.submission(),
                &self.output,
            )
            .unwrap();
        decode_display(&bytes)
    }
}

#[derive(Debug)]
struct Quality {
    mae: f64,
    rmse: f64,
    byte_mae: f64,
    edge_mae: f64,
    interior_mae: f64,
    edge_shift: Option<f64>,
    contrast_ratio: Option<f64>,
    instability: Option<f64>,
}

/// The edge of the strongest horizontal luma step in the middle row of the reference.
fn edge_of(image: &[Pixel], width: usize, y: usize, at: usize) -> Option<EdgeProfile> {
    let (x0, x1) = (
        at.saturating_sub(EDGE_WINDOW),
        (at + EDGE_WINDOW + 1).min(width),
    );
    edge_profile(&luma_row(image, width, y, x0, x1))
}

fn strongest_step(image: &[Pixel], width: usize, y: usize) -> usize {
    let row = luma_row(image, width, y, 0, width);
    (1..width)
        .max_by(|&a, &b| {
            (row[a] - row[a - 1])
                .abs()
                .total_cmp(&(row[b] - row[b - 1]).abs())
        })
        .unwrap_or(1)
}

fn quality(frames: &[Vec<Pixel>], truth: &[Vec<Pixel>], output: [u32; 2]) -> Quality {
    let (last, wanted) = (frames.last().unwrap(), truth.last().unwrap());
    let width = output[0] as usize;
    let edges = edge_mask(wanted, width, EDGE_THRESHOLD);
    let interior: Vec<bool> = edges.iter().map(|e| !e).collect();
    let (y, at) = (
        output[1] as usize / 2,
        strongest_step(wanted, width, output[1] as usize / 2),
    );
    let (seen, expected) = (edge_of(last, width, y, at), edge_of(wanted, width, y, at));
    Quality {
        mae: linear_mae(last, wanted, None),
        rmse: linear_rmse(last, wanted, None),
        byte_mae: srgb_byte_mae(last, wanted, None),
        edge_mae: linear_mae(last, wanted, Some(&edges)),
        interior_mae: linear_mae(last, wanted, Some(&interior)),
        edge_shift: seen
            .zip(expected)
            .map(|(s, e)| (s.position - e.position).abs()),
        contrast_ratio: seen.zip(expected).map(|(s, e)| s.contrast / e.contrast),
        instability: (frames.len() > 1)
            .then(|| motion_compensated_instability(frames, truth, width, (0, 0), None)),
    }
}

/// Warm frames, then `frames` measured frames read back; returns the measured display images.
fn measured_frames(harness: &mut Harness<'_>, eyes: &[i64], warm: usize) -> Vec<Vec<Pixel>> {
    for &eye in &eyes[..warm] {
        harness.frame(eye);
    }
    eyes[warm..]
        .iter()
        .map(|&eye| {
            harness.frame(eye);
            harness.display()
        })
        .collect()
}

fn cost(harness: &mut Harness<'_>, (warm, measured): (usize, usize)) -> Envelope {
    let samples: Vec<f64> = (0..warm + measured)
        .filter_map(|index| harness.frame(0).filter(|_| index >= warm))
        .collect();
    summarize(&samples)
}

fn open_context() -> GpuContext {
    let wanted = REQUIRED_FEATURES | wgpu::Features::TIMESTAMP_QUERY;
    pollster::block_on(GpuContext::request_with_features(wanted))
        .or_else(|_| pollster::block_on(GpuContext::request()))
        .unwrap()
}

fn fmt_option(value: Option<f64>) -> String {
    value.map_or_else(|| "n/a".to_owned(), |v| format!("{v:.4}"))
}

fn print_row(
    cell: (Mode, [u32; 2], u32, f64),
    render: [u32; 2],
    q: &Quality,
    gpu: Option<Envelope>,
) {
    let numbers = [q.mae, q.rmse, q.byte_mae, q.edge_mae, q.interior_mae];
    assert!(numbers.iter().all(|v| v.is_finite()), "{cell:?}: {q:?}");
    assert!(
        [q.edge_shift, q.contrast_ratio, q.instability]
            .iter()
            .flatten()
            .all(|v| v.is_finite()),
        "{cell:?}: {q:?}"
    );
    let gpu = gpu.map_or_else(|| "-".to_owned(), |g| g.to_string());
    println!(
        "{:?} {}x{} scale={} render={}x{} pan={} | mae={:.4} rmse={:.4} byte={:.4} edge={:.4} interior={:.4} \
         edge_shift={} contrast={} instability={} | gpu_ms median/p95={}",
        cell.0,
        cell.1[0],
        cell.1[1],
        cell.2,
        render[0],
        render[1],
        cell.3,
        q.mae,
        q.rmse,
        q.byte_mae,
        q.edge_mae,
        q.interior_mae,
        fmt_option(q.edge_shift),
        fmt_option(q.contrast_ratio),
        fmt_option(q.instability),
        gpu,
    );
}

fn run_cell(
    context: &GpuContext,
    scene: &CompositeScene,
    config: &Config,
    cell: (Mode, [u32; 2], u32, f64),
) {
    let (mode, output, scale, pan) = cell;
    let eyes: Vec<i64> = (0..config.warm + config.frames)
        .map(|k| eye_travel_mm(pan, k, output[1]))
        .collect();
    let truth: Vec<Vec<Pixel>> = eyes[config.warm..]
        .iter()
        .map(|&eye| reference(context, scene, output, (eye, config.factor)))
        .collect();
    let mut harness = Harness::new(context, scene, (mode, output, scale));
    let frames = measured_frames(&mut harness, &eyes, config.warm);
    let timing = (pan == 0.0).then(|| cost(&mut harness, config.timing));
    print_row(
        cell,
        harness.render,
        &quality(&frames, &truth, output),
        timing,
    );
}

#[test]
#[ignore = "qualification measurement matrix: run explicitly with --ignored --nocapture"]
fn qualification_matrix() {
    let config = Config::from_env();
    let context = open_context();
    let scene = CompositeScene::new(context.device());
    for output in &config.outputs {
        for &pan in &config.pans {
            for (index, &scale) in config.scales.iter().enumerate() {
                for &mode in &config.modes {
                    // Full-resolution TAA does not depend on the scale: run it once per output.
                    if mode != Mode::M5 || index == 0 {
                        run_cell(&context, &scene, &config, (mode, *output, scale, pan));
                    }
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "qualification_stage_timings_tests.rs"]
mod stage_timings;
