//! Frames that edit many scattered instances and move the camera every frame, as a moving
//! frame with walking units does. The ignored probes are informational: run them serially
//! with `--ignored --nocapture`.
use super::*;
use crate::draw::PreparedScene;
use crate::instances::InstanceHandle;
use crate::offscreen::OffscreenTarget;
use crate::shadows::ShadowedTarget;
use crate::{CameraPose, Viewport};
use std::collections::VecDeque;

/// Four meshes by four materials: sixteen instance batches.
const MESHES: u32 = 4;
const MATERIALS: u32 = 4;
const COLUMNS: i64 = 64;

fn material(texture: u32) -> SceneMaterial {
    SceneMaterial::new(SceneMaterialParts {
        albedo: TextureHandle::from_raw(texture),
        normal: None,
        roughness_metallic: None,
        ambient_occlusion: None,
        self_illumination: None,
        roughness_micro: 1_000_000,
        metalness_micro: 0,
        alpha_mode: SceneAlphaMode::Opaque,
        double_sided: false,
    })
    .unwrap()
}

fn position(index: usize, frame: u64) -> [i64; 3] {
    let index = i64::try_from(index).unwrap();
    let lift = i64::try_from(frame % 7).unwrap() * 20;
    [
        (index % COLUMNS) * 120 - COLUMNS * 60,
        (index / COLUMNS) * 120 - 3_000,
        600 + lift,
    ]
}

/// `count` casters over a ground quad, spread round-robin over the sixteen batches.
fn source(count: usize) -> RenderScene {
    let texture = |shade: u8| SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Srgb,
        width: 1,
        height: 1,
        mips: 1,
        bytes: vec![shade, 200, 255 - shade, 255].into(),
    };
    let mut objects = vec![fixture::object([0, 0, 0], 0, 0)];
    objects.extend((0..count).map(|index| {
        let batch = u32::try_from(index).unwrap() % (MESHES * MATERIALS);
        fixture::object(position(index, 0), 1 + batch % MESHES, batch / MESHES)
    }));
    let meshes = std::iter::once(fixture::quad(6_000))
        .chain((0..MESHES).map(|mesh| fixture::quad(30 + i64::from(mesh) * 10)))
        .collect();
    let scene = RenderScene::new(SceneParts {
        resources: SceneResources::new(
            (0..MATERIALS)
                .map(|m| texture(u8::try_from(m * 60).unwrap()))
                .collect(),
            (0..MATERIALS).map(material).collect(),
        )
        .unwrap(),
        geometry: SceneGeometry::new(meshes, objects).unwrap(),
        terrain: None,
        effects: None,
    })
    .unwrap();
    cascades::source_with(
        &scene,
        SceneShadowCascades::new(vec![1_000, 2_000, 3_000, 4_000], 0).unwrap(),
    )
}

/// A camera that pans and dollies a little every frame, so every view's selection changes.
fn moving_view(frame: u64, viewport: Viewport) -> SceneView {
    let step = i64::try_from(frame).unwrap();
    SceneView::new(
        CameraPose {
            eye_millimeters: [step * 15 - 600, -4_000 + step * 5, 3_500],
            target_millimeters: [step * 15 - 600, 0, 0],
        },
        *fixture::effects(600_000).camera(),
        viewport,
    )
    .unwrap()
}

/// Moves every `stride`-th caster, offset by the frame. Casters are dealt round-robin to the
/// sixteen batches, so a stride coprime with sixteen moves casters of every batch that are not
/// next to each other there: one upload per moved caster, as walking units cause.
fn edit(renderer: &mut ShadowedScene, handles: &[InstanceHandle], frame: u64, stride: usize) {
    let offset = usize::try_from(frame).unwrap() % stride;
    for index in (offset..handles.len() - 1).step_by(stride) {
        let handle = handles[index + 1];
        let mut transform = *renderer.scene_mut().instance(handle).unwrap().0.transform();
        transform.position_millimeters = position(index, frame);
        renderer
            .scene_mut()
            .set_instance_transform(handle, transform)
            .unwrap();
    }
}

struct Flight {
    renderer: ShadowedScene,
    scheduler: crate::compositor::FrameRenderer,
    handles: Vec<InstanceHandle>,
    target: OffscreenTarget,
    pending: VecDeque<wgpu::SubmissionIndex>,
    viewport: Viewport,
}
impl Flight {
    fn new(context: &GpuContext, count: usize, viewport: Viewport) -> Self {
        let mut renderer = ShadowedScene::new(
            context,
            PreparedScene::new(&source(count)).unwrap(),
            DirectionalShadowSettings {
                half_extent_meters: 6.0,
                depth_half_extent_meters: 6.0,
                ..settings()
            },
            ShadowedTarget::new(OffscreenTarget::COLOR_FORMAT, 1),
        )
        .unwrap();
        let handles = renderer.scene_mut().instance_handles().collect();
        Self {
            renderer,
            scheduler: crate::compositor::FrameRenderer::new(context.device()),
            handles,
            target: OffscreenTarget::new(context.device(), viewport).unwrap(),
            pending: VecDeque::new(),
            viewport,
        }
    }

    /// Edits, records and submits frame `frame` with up to `in_flight` frames outstanding, and
    /// returns the CPU time from beginning the frame to its submission.
    fn frame(
        &mut self,
        context: &GpuContext,
        frame: u64,
        stride: usize,
        in_flight: usize,
    ) -> std::time::Duration {
        while self.pending.len() >= in_flight {
            context
                .device()
                .poll(wgpu::PollType::Wait {
                    submission_index: self.pending.pop_front(),
                    timeout: Some(std::time::Duration::from_secs(30)),
                })
                .unwrap();
        }
        let start = std::time::Instant::now();
        edit(&mut self.renderer, &self.handles, frame, stride);
        self.renderer
            .select_frame_slot(usize::try_from(frame % 3).unwrap());
        let mut encoder = context.submission().begin(context.device()).unwrap();
        self.renderer
            .encode_with(
                context,
                &moving_view(frame, self.viewport),
                (
                    &self.target.color_attachment(),
                    &self.target.depth_attachment(),
                ),
                &mut encoder,
                &mut self.scheduler,
            )
            .unwrap();
        self.pending.push_back(encoder.finish().submit().unwrap());
        start.elapsed()
    }

    fn image(&mut self, context: &GpuContext, frame: u64) -> crate::offscreen::RgbaImage {
        while let Some(submission) = self.pending.pop_front() {
            context
                .device()
                .poll(wgpu::PollType::Wait {
                    submission_index: Some(submission),
                    timeout: Some(std::time::Duration::from_secs(30)),
                })
                .unwrap();
        }
        self.renderer.select_frame_slot(0);
        render_target(
            &mut self.renderer,
            &mut self.scheduler,
            context,
            &moving_view(frame, self.viewport),
            &self.target,
        )
        .unwrap();
        self.target.read_rgba8(context).unwrap()
    }
}

const STRIDE: usize = 9;
const STRIDE_FRAMES: u64 = 9;

#[test]
fn edited_moving_frames_stop_creating_staging_and_match_a_fresh_upload() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let viewport = Viewport {
        width: 192,
        height: 128,
    };
    let mut flight = Flight::new(&context, 1_024, viewport);
    let before = staging(&context);
    let mut staged = vec![before];
    for frame in 0..24 {
        flight.frame(&context, frame, STRIDE, 3);
        staged.push(staging(&context));
    }
    let created: Vec<u64> = staged.iter().map(|[.., created]| *created).collect();
    // About one upload per moved caster, not one per batch.
    let per_frame = (staged[24][0] - before[0]) / 24;
    assert!(per_frame > 100, "uploads per frame: {per_frame}");
    // Each frame's uploads, with their alignment, fit the smallest (256 KiB) chunk, and at most
    // three frames hold chunks at once. Whether and when the second and third chunks are created
    // depends on when the device delivers the mapping callbacks that free submitted chunks.
    assert!(
        staged
            .windows(2)
            .all(|pair| pair[1][1] - pair[0][1] + 8 * (pair[1][0] - pair[0][0]) <= 256 * 1024),
        "{staged:?}"
    );
    assert!(created[24] - created[0] <= 3, "{created:?}");
    // Once the device is idle every chunk's mapping has completed, so a frame that starts on an
    // idle device reuses free chunks and creates none.
    for frame in 24..32 {
        context
            .device()
            .poll(wgpu::PollType::wait_indefinitely())
            .unwrap();
        flight.frame(&context, frame, STRIDE, 1);
        assert_eq!(
            context.submission().upload_staging().chunks_created,
            created[24],
            "settled frame {frame} created staging; in flight: {created:?}"
        );
    }
    let edited = flight.image(&context, 32);
    let mut fresh = Flight::new(&context, 1_024, viewport);
    let handles = fresh.handles.clone();
    // The last STRIDE frames wrote each caster's final position once.
    for frame in 32 - STRIDE_FRAMES..32 {
        edit(&mut fresh.renderer, &handles, frame, STRIDE);
    }
    assert_eq!(
        fresh.image(&context, 32),
        edited,
        "incrementally uploaded instances must render like a fresh upload of the same state"
    );
}

/// Uploads, uploaded bytes and chunks created by the frame authority's upload staging.
fn staging(context: &GpuContext) -> [u64; 3] {
    let statistics = context.submission().upload_staging();
    [
        statistics.uploads,
        statistics.uploaded_bytes,
        statistics.chunks_created,
    ]
}

fn fnv(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, &byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// Prints a digest of every frame of a fixed edited, moving sequence, for comparing two builds.
#[test]
#[ignore = "informational upload staging image probe"]
fn upload_staging_probe_images() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let mut flight = Flight::new(
        &context,
        4_096,
        Viewport {
            width: 320,
            height: 192,
        },
    );
    for frame in 0..16 {
        flight.frame(&context, frame, 17, 3);
        let image = flight.image(&context, frame);
        println!(
            "UPLOAD_PROBE_IMAGE frame={frame} fnv={:016x}",
            fnv(&image.pixels)
        );
    }
}

/// Prints the CPU time per moving frame and its upload counts.
#[test]
#[ignore = "informational upload staging CPU timing probe"]
fn upload_staging_probe_timing() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let viewport = Viewport {
        width: 640,
        height: 360,
    };
    let mut flight = Flight::new(&context, 16_384, viewport);
    for frame in 0..16 {
        flight.frame(&context, frame, 17, 3);
    }
    let before = staging(&context);
    let mut times: Vec<f64> = (16..80)
        .map(|frame| flight.frame(&context, frame, 17, 3).as_secs_f64() * 1e3)
        .collect();
    let after = staging(&context);
    times.sort_by(f64::total_cmp);
    println!(
        "UPLOAD_PROBE_TIMING frames=64 median_ms={:.3} p10_ms={:.3} p90_ms={:.3}",
        (times[31] + times[32]) * 0.5,
        times[6],
        times[57]
    );
    println!(
        "UPLOAD_PROBE_STAGING uploads_in_64_frames={} bytes_in_64_frames={} chunks_created_in_64_frames={}",
        after[0] - before[0],
        after[1] - before[1],
        after[2] - before[2]
    );
}
