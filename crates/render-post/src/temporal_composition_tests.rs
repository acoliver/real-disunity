//! Composition consumption of the prepared mask in the locked entry point, on lavapipe. The
//! scenes are the wires of `temporal_thin_feature_scene` with a producer that paints the G
//! channel over the whole frame, so every lock candidate of the wire lies under it. Locks are off
//! by default; the tests build their chains under `WithLocks`. A composition of 1 removes the lock
//! share and the lock itself, a composition of 0 is the locked path before masks.
use crate::taa_mask_gpu::WithoutMasks;
use crate::taa_upscale_gpu::{WithLocks, WithoutLocks};
use crate::temporal_scene_test_rig::SceneRig;
use crate::temporal_thin_feature_scene::{Fixture, fixtures};
use worldrunner_render_plan::post::RenderScale;

const OUTPUT: [u32; 2] = crate::temporal_scene_test_rig::OUTPUT;
/// Half of a half float step near 1.
const F16_STEP: f64 = 1.0 / 2048.0;
/// Canvas pixels per frame of the pan.
const SPEED: f64 = 0.3;
const SCALES: [u32; 3] = [500_000, 670_000, 770_000];

/// The scene of the fixture with a producer writing `composition` to the G channel everywhere.
fn painted(fixture: &Fixture, composition: f32) -> String {
    format!(
        "{}
@fragment fn paint_mask() -> @location(0) vec4<f32> {{
    return vec4<f32>(0.0, {composition:?} + frame.tail.w * 0.0, 0.0, 0.0);
}}
",
        fixture.wgsl()
    )
}

#[derive(Clone, Copy)]
enum Chain {
    /// Locks on, a mask whose G channel is the value.
    LockedMasked(f32),
    /// Locks on, no mask.
    LockedUnmasked,
    /// Locks off, no mask.
    Unlocked,
}

fn rig(
    gpu: &(wgpu::Device, wgpu::Queue),
    millionths: u32,
    fixture: &Fixture,
    kind: Chain,
) -> SceneRig {
    let render = RenderScale::new(millionths)
        .unwrap()
        .extent(OUTPUT)
        .unwrap();
    let (_locks, _masks): (Box<dyn std::any::Any>, Option<WithoutMasks>) = match kind {
        Chain::LockedMasked(_) => (Box::new(WithLocks::new()), None),
        Chain::LockedUnmasked => (Box::new(WithLocks::new()), Some(WithoutMasks::new())),
        Chain::Unlocked => (Box::new(WithoutLocks::new()), Some(WithoutMasks::new())),
    };
    let source = match kind {
        Chain::LockedMasked(c) => painted(fixture, c),
        Chain::LockedUnmasked | Chain::Unlocked => fixture.wgsl(),
    };
    SceneRig::new(gpu, render, source)
}

/// The last frame of two jitter cycles of a pan: the image bits-as-values and the lock bank.
struct Last {
    image: Vec<f64>,
    bank: Option<Vec<[f64; 4]>>,
}

fn run(rig: &SceneRig) -> Last {
    let total = 2 * u64::from(rig.jitter.length());
    for index in 0..total {
        let pan = SPEED * f64::from(u32::try_from(index).unwrap());
        #[expect(clippy::cast_possible_truncation, reason = "the GPU block is f32")]
        let tail = [pan as f32, SPEED as f32, 1.0, 0.0];
        rig.frame(index, index > 0, (tail, None));
    }
    Last {
        image: rig.image(),
        bank: rig.lock_bank(),
    }
}

fn locks_alive(bank: &[[f64; 4]]) -> usize {
    bank.iter().filter(|p| p[0] > 0.0).count()
}

fn bits(image: &[f64]) -> Vec<u64> {
    image.iter().map(|v| v.to_bits()).collect()
}

#[test]
fn composition_one_removes_the_locks_and_equals_the_lock_off_image() {
    let gpu = crate::gpu_tests::gpu();
    for millionths in SCALES {
        for fixture in fixtures() {
            let locked = run(&rig(&gpu, millionths, &fixture, Chain::LockedUnmasked));
            let composed = run(&rig(&gpu, millionths, &fixture, Chain::LockedMasked(1.0)));
            let off = run(&rig(&gpu, millionths, &fixture, Chain::Unlocked));
            let alive = locks_alive(locked.bank.as_ref().unwrap());
            let bank = composed.bank.as_ref().unwrap();
            let worst = composed
                .image
                .iter()
                .zip(&off.image)
                .map(|(a, b)| (a - b).abs() / b.abs().max(1.0))
                .fold(0.0, f64::max);
            eprintln!(
                "composition 1 scale {millionths} {}: locks without masks {alive}, with {}, worst {worst:.2e}",
                fixture.name,
                locks_alive(bank)
            );
            assert!(alive > 0, "{}: the scene must create locks", fixture.name);
            assert!(
                bank.iter().all(|p| p.iter().all(|v| *v == 0.0)),
                "{}: a bank under composition 1 is zero",
                fixture.name
            );
            assert!(worst <= F16_STEP, "{}: {worst}", fixture.name);
        }
    }
}

#[test]
fn composition_zero_is_the_locked_path_without_masks() {
    let gpu = crate::gpu_tests::gpu();
    for millionths in SCALES {
        for fixture in fixtures() {
            let plain = run(&rig(&gpu, millionths, &fixture, Chain::LockedUnmasked));
            let zero = run(&rig(&gpu, millionths, &fixture, Chain::LockedMasked(0.0)));
            assert_eq!(bits(&plain.image), bits(&zero.image), "{}", fixture.name);
            let (a, b) = (plain.bank.unwrap(), zero.bank.unwrap());
            assert!(locks_alive(&a) > 0);
            let words = |bank: &[[f64; 4]]| -> Vec<u64> {
                bank.iter().flatten().map(|v| v.to_bits()).collect()
            };
            assert_eq!(words(&a), words(&b), "{}", fixture.name);
        }
    }
}
