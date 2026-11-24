//! Per-frame CPU cost of a moving-camera frame through the real retained host.
//!
//! Builds a synthetic outdoor scene (terrain, a few thousand props over a few dozen materials,
//! a shadowed sun and the HDR bloom + FXAA chain), then renders
//! frames without a window while the camera walks a circle, moving every frame. Each frame is
//! timed from the driver's camera update to the submit. The example waits for the GPU between
//! frames, outside the timed span. Build it in release mode: debug builds run far slower and say
//! nothing about a shipped frame.
//!
//! ```text
//! VK_DRIVER_FILES=/usr/share/vulkan/icd.d/lvp_icd.json WGPU_BACKEND=vulkan \
//!   cargo run --release -p worldrunner-scene-viewer --example moving_frame_cpu -- \
//!   [--frames 300] [--warmup 60] [--objects 3000] [--in-flight 1] [--size 1280x720] \
//!   [--clutter 20000 --edits 300]
//! ```
//!
//! `--clutter N` adds N pieces of clutter to the scene, spread over eight large instance
//! batches. With `--edits E` the driver also edits E of them every frame, as a game that streams
//! clutter around the camera does: a third are removed, a third are replaced by new pieces near
//! the camera and a third move where they stand. The edits happen in the driver's update, so
//! the timed span includes publishing them to the retained host and the renderer.
//!
//! Lavapipe makes GPU time meaningless; the CPU figures stay valid. `--in-flight 1` keeps the
//! software rasterizer from competing with the timed thread for cores.
use std::{
    cell::Cell,
    rc::Rc,
    time::{Duration, Instant},
};

use worldrunner_render_scene::{
    CameraPose,
    shadows::{CascadeRefreshPolicy, DirectionalShadowSettings},
};
use worldrunner_scene_viewer::{
    FrameProfile, OrbitLimits, RetainedDriver, RetainedHost, RetainedTick, RetainedUpdate, ToneMap,
    ViewerOptions, WindowlessRetained,
};

#[path = "moving_frame_cpu/edits.rs"]
mod edits;
#[path = "moving_frame_cpu/scene.rs"]
mod scene;
#[path = "moving_frame_cpu/stats.rs"]
mod stats;

type Failure = Box<dyn std::error::Error>;

struct Config {
    frames: usize,
    warmup: usize,
    objects: usize,
    clutter: usize,
    edits: usize,
    in_flight: usize,
    size: [u32; 2],
}

impl Config {
    fn from_args() -> Result<Self, Failure> {
        let mut config = Self {
            frames: 300,
            warmup: 60,
            objects: 3000,
            clutter: 0,
            edits: 0,
            in_flight: 1,
            size: [1280, 720],
        };
        let mut args = std::env::args().skip(1);
        while let Some(flag) = args.next() {
            let mut value = || args.next().ok_or(format!("{flag} needs a value"));
            match flag.as_str() {
                "--frames" => config.frames = value()?.parse()?,
                "--warmup" => config.warmup = value()?.parse()?,
                "--objects" => config.objects = value()?.parse()?,
                "--clutter" => config.clutter = value()?.parse()?,
                "--edits" => config.edits = value()?.parse()?,
                "--in-flight" => config.in_flight = value()?.parse()?,
                "--size" => config.size = parse_size(&value()?)?,
                other => return Err(format!("unknown argument {other}").into()),
            }
        }
        Ok(config)
    }

    fn options(&self) -> ViewerOptions {
        ViewerOptions {
            activity_reason: worldrunner_scene_viewer::ActivityReason::default(),
            presentation_budget: worldrunner_scene_viewer::PresentationBudget::default().into(),
            capture_pool: worldrunner_scene_viewer::CapturePoolPolicy::default(),
            window_size_unit: worldrunner_scene_viewer::WindowSizeUnit::default(),
            window_mode: worldrunner_scene_viewer::WindowMode::default(),
            camera_convention: worldrunner_camera::ViewConvention::RIGHT_HANDED,
            texture_tiers: None,
            game: worldrunner_scene_viewer::GameRendering::default(),
            title: "Moving frame CPU".into(),
            size: self.size,
            pose: pose(0),
            camera: scene::lens(),
            orbit: OrbitLimits {
                min_distance_millimeters: 100.0,
                max_distance_millimeters: 400_000.0,
                min_elevation_radians: -1.4,
                max_elevation_radians: 1.4,
            },
            shadows: Some(DirectionalShadowSettings {
                focus_meters: glam::Vec3::ZERO,
                half_extent_meters: 50.0,
                depth_half_extent_meters: 100.0,
                resolution: 2048,
                depth_bias: 0.0005,
                normal_bias_meters: 0.02,
                refresh_policy: CascadeRefreshPolicy::default(),
            }),
            frame_profile: FrameProfile::Hdr {
                samples: 1,
                tone_map: ToneMap::Reinhard,
            },
            frames_in_flight: self.in_flight,
            present_mode: worldrunner_scene_viewer::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            gpu_timing: false,
            adapter_callback: None,
            cadence: worldrunner_scene_viewer::PresentationCadence::default(),
            ssr: None,
            render_clock: worldrunner_scene_viewer::RenderClock::default(),
        }
    }
}

fn parse_size(text: &str) -> Result<[u32; 2], Failure> {
    let (width, height) = text.split_once('x').ok_or("size is WIDTHxHEIGHT")?;
    Ok([width.parse()?, height.parse()?])
}

/// Camera position on a sixteen-sided path of 60 m radius, in thousandths of a side.
fn path_point(phase: i64, climb_mm: i64) -> [i64; 3] {
    const RADIUS_MM: i64 = 60_000;
    let side = usize::try_from(phase.div_euclid(1000).rem_euclid(16)).expect("below sixteen");
    let along = phase.rem_euclid(1000);
    let corner = |index: usize| scene::CIRCLE[index % 16];
    let (from, to) = (corner(side), corner(side + 1));
    let east = RADIUS_MM * (from.0 * (1000 - along) + to.0 * along) / (1000 * 1_000_000);
    let north = RADIUS_MM * (from.1 * (1000 - along) + to.1 * along) / (1000 * 1_000_000);
    [east, north, scene::ground_height(east, north) + climb_mm]
}

/// The camera at `frame`: walking pace of about 0.3 m per frame, looking three sides ahead.
fn pose(frame: u64) -> CameraPose {
    let phase = i64::try_from(frame).expect("small") * 13;
    CameraPose {
        eye_millimeters: path_point(phase, 1_700),
        target_millimeters: path_point(phase + 3_000, 1_500),
    }
}

/// Walks the camera, edits the streamed clutter and records how long its own update took.
struct Walker {
    spent: Rc<Cell<Duration>>,
    clutter: edits::Clutter,
}

impl RetainedDriver for Walker {
    fn update(&mut self, host: &mut RetainedHost, tick: &RetainedTick) -> Result<(), String> {
        let started = Instant::now();
        let pose = pose(tick.frame);
        let mut result = host.apply(RetainedUpdate::Camera {
            pose,
            lens: scene::lens(),
            convention: worldrunner_camera::ViewConvention::RIGHT_HANDED,
        });
        if result.is_ok()
            && let Some(edits) = self.clutter.edits(pose.eye_millimeters)
        {
            result = host.apply(RetainedUpdate::Objects(edits));
        }
        self.spent.set(self.spent.get() + started.elapsed());
        result.map_err(|error| error.to_string())
    }
}

/// On-CPU time of the calling thread, which excludes time spent blocked or asleep.
fn thread_cpu() -> Option<Duration> {
    let text = std::fs::read_to_string("/proc/thread-self/schedstat").ok()?;
    let nanoseconds: u64 = text.split_whitespace().next()?.parse().ok()?;
    Some(Duration::from_nanos(nanoseconds))
}

/// Sleeps past the GPU's turn before each attempt, so that the host finds the previous frame
/// finished: its cleanup then lands in the timed call, and refused admission attempts stay rare.
struct Pacer {
    last_submit: Instant,
    wait: Duration,
}

impl Pacer {
    fn pause(&self) {
        std::thread::sleep(self.wait.saturating_sub(self.last_submit.elapsed()));
    }

    /// Records whether the first attempt after the pause was admitted: a refusal stretches the
    /// pause to the turn just seen, and an admission shortens it slowly.
    fn submitted(&mut self, refused: bool) {
        let turn = self.last_submit.elapsed();
        self.wait = if refused {
            turn.mul_f64(1.15)
        } else {
            self.wait.mul_f64(0.98)
        };
        self.last_submit = Instant::now();
    }
}

/// Calls `frame` until the host submits one. Only the submitting call is timed, together with
/// the driver update when an earlier refused call ran it: refused calls wait for the GPU.
fn submit_one(
    host: &mut WindowlessRetained<'_>,
    (pacer, spent): (&mut Pacer, &Cell<Duration>),
) -> Result<stats::Sample, Failure> {
    pacer.pause();
    let give_up = Instant::now() + Duration::from_mins(2);
    let mut carried = Duration::ZERO;
    let mut refused = false;
    loop {
        let cpu_before = thread_cpu();
        let started = Instant::now();
        let submitted = host.frame()?;
        let wall = started.elapsed();
        if submitted {
            spent.take();
            pacer.submitted(refused);
            let cpu = thread_cpu()
                .zip(cpu_before)
                .map(|(after, before)| after.saturating_sub(before) + carried);
            return Ok(stats::Sample {
                active: wall + carried,
                cpu,
            });
        }
        (refused, carried) = (true, carried + spent.take());
        host.poll()?;
        if Instant::now() > give_up {
            return Err("the host admitted no frame within 120 s".into());
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn main() -> Result<(), Failure> {
    let started = Instant::now();
    let config = Config::from_args()?;
    let scene = scene::scene(config.objects, config.clutter)?;
    let spent = Rc::new(Cell::new(Duration::ZERO));
    let mut walker = Walker {
        spent: Rc::clone(&spent),
        clutter: edits::Clutter::new(config.objects, config.clutter, config.edits),
    };
    let mut host = RetainedHost::new(scene, config.options())?.windowless(&mut walker, None)?;
    let mut pacer = Pacer {
        last_submit: Instant::now(),
        wait: Duration::ZERO,
    };
    let mut samples = Vec::new();
    let mut warmed = started.elapsed();
    for index in 0..config.warmup + config.frames {
        if index == config.warmup {
            warmed = started.elapsed();
        }
        let sample = submit_one(&mut host, (&mut pacer, &spent))?;
        if index >= config.warmup {
            samples.push(sample);
        }
    }
    let finished = started.elapsed();
    let draws = host.host().statistics().map(|latest| latest.scene.passes);
    host.shutdown()?;
    stats::report(&config_summary(&config), &samples);
    stats::report_timeline(warmed, finished);
    if let Some(draws) = draws {
        stats::report_draws(&draws);
    }
    Ok(())
}

fn config_summary(config: &Config) -> String {
    format!(
        "{} objects, {} clutter edited {} per frame, {}x{}, HDR bloom+FXAA profile, {} in flight, {} warm-up frames",
        config.objects,
        config.clutter,
        config.edits,
        config.size[0],
        config.size[1],
        config.in_flight,
        config.warmup,
    )
}
