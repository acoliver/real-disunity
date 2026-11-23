//! What preparing the body draws costs on the CPU once every program is built: each frame
//! prepares one draw per body, and a draw whose program, state and targets an earlier frame
//! prepared finds its pipeline without computing the pipeline's content key again. wgpu's no-op
//! device validates what is created as a real device does and runs nothing.

use super::*;
use worldrunner_render_gpu::submission::SubmissionAuthority;

const BODIES: u32 = 8;
const FRAMES: u32 = 64;

#[test]
fn later_frames_compute_no_pipeline_key() {
    let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
    let authority = SubmissionAuthority::new(queue);
    let bodies = (0..BODIES)
        .map(|index| SceneWaterBody {
            east_min_millimeters: -10_000 + i64::from(index) * 25_000,
            ..settings()
        })
        .collect();
    let config = FrameConfig::new([32, 32], wgpu::TextureFormat::Rgba16Float, 1);
    let water = Water::new(&device, &authority, config, bodies).unwrap();
    let frame = FrameUniform::new(&fixture_view(), None).unwrap();
    let stats = || water.programs.borrow().runtime().stats();
    let prepare = |slot| {
        let encoder = authority.begin(&device).unwrap();
        let feature = water
            .feature(&encoder, &frame, (0.0, slot), None, DepthMode::Independent)
            .unwrap();
        assert!(feature.draws.iter().all(Option::is_some));
        drop(feature);
        encoder.finish().cancel();
    };
    prepare(0);
    let first = stats();
    let start = std::time::Instant::now();
    for frame in 0..FRAMES {
        prepare(usize::try_from(frame % 2).unwrap());
    }
    let elapsed = start.elapsed();
    let later = stats();
    eprintln!(
        "{FRAMES} frames of {BODIES} bodies: {} pipeline keys computed, {:.2} us per body draw",
        later.pipeline_keys_computed - first.pipeline_keys_computed,
        elapsed.as_secs_f64() * 1e6 / f64::from(FRAMES) / f64::from(BODIES)
    );
    assert_eq!(first.pipeline_keys_computed, 1, "the bodies share one key");
    assert_eq!(
        later.pipeline_keys_computed, first.pipeline_keys_computed,
        "later frames reuse the key"
    );
    assert_eq!(
        later.draws_prepared - first.draws_prepared,
        u64::from(FRAMES) * u64::from(BODIES)
    );
}
