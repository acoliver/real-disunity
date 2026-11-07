//! Measure the intentional invariant-only rounding change separately from reuse equality.
use super::*;

#[test]
fn gpu_invariant_only_baseline_reports_main_attachment_deltas() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let mut bytes = [0; 8];
    let mut pixels = [0; 8];
    let mut fixtures = 0;
    for (name, scene, pose) in scenes().into_iter().chain(fixtures::scenes()) {
        for jitter in [0.0, 0.375] {
            let main = capture::positions_frame(
                &context,
                (&scene, pose),
                (false, jitter),
                capture::Transform::World,
                false,
            );
            let invariant = capture::frame(&context, (&scene, pose), (false, jitter));
            for (index, (main, invariant)) in main.iter().zip(&invariant).enumerate() {
                let width = if matches!(index, 3 | 6 | 7) { 4 } else { 8 };
                let changed_bytes = main.iter().zip(invariant).filter(|(a, b)| a != b).count();
                let changed_pixels = main
                    .chunks_exact(width)
                    .zip(invariant.chunks_exact(width))
                    .filter(|(a, b)| a != b)
                    .count();
                bytes[index] += changed_bytes;
                pixels[index] += changed_pixels;
                println!(
                    "INVARIANT_DELTA fixture={name:?} jitter={jitter} attachment={index} bytes={changed_bytes} pixels={changed_pixels}"
                );
            }
            fixtures += 1;
        }
    }
    println!("INVARIANT_TOTAL fixtures={fixtures} bytes={bytes:?} pixels={pixels:?}");
}
