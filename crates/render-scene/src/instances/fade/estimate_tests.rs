//! The distance estimate against the interval arithmetic it settles without, and the whole
//! fade test against the reference kept from before anchors and estimates.
use super::*;
use worldrunner_render_plan::{
    MaterialHandle, MeshHandle, SceneAlphaMode, SceneFadeBand, SceneImpostor, SceneImpostorLayout,
    SceneMaterialParts, SceneObject, SceneRotation, SceneTransform, TexelFormat, TextureColorSpace,
    TextureHandle,
};

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, bound: u64) -> u64 {
        self.next() % bound
    }

    fn signed(&mut self, magnitude: i64) -> i64 {
        i64::try_from(self.below(u64::try_from(2 * magnitude + 1).unwrap())).unwrap() - magnitude
    }

    fn unit(&mut self) -> f32 {
        f32::from(u16::try_from(self.next() >> 48).unwrap()) / 65_536.0
    }

    fn direction(&mut self) -> Vec3 {
        loop {
            let v = Vec3::new(self.unit(), self.unit(), self.unit()) * 2.0 - Vec3::ONE;
            if v.length_squared() > 0.01 && v.length_squared() <= 1.0 {
                return v.normalize();
            }
        }
    }
}

const ROTATIONS: [SceneRotation; 5] = [
    SceneRotation::IDENTITY,
    SceneRotation {
        x: 0,
        y: 0,
        z: 707_107,
        w: 707_107,
    },
    SceneRotation {
        x: 382_683,
        y: 0,
        z: 0,
        w: 923_880,
    },
    SceneRotation {
        x: 0,
        y: 382_683,
        z: 0,
        w: -923_880,
    },
    SceneRotation {
        x: 500_000,
        y: -500_000,
        z: 500_000,
        w: 500_000,
    },
];

fn transform(rng: &mut Rng, meters: i64) -> SceneTransform {
    SceneTransform {
        position_millimeters: [(); 3].map(|()| rng.signed(meters * 1000)),
        rotation: ROTATIONS[usize::try_from(rng.below(5)).unwrap()],
        scale_millionths: 1 + rng.below(8_000_000),
    }
}

fn reference(rng: &mut Rng) -> SceneFadeReference {
    let offset = [(); 3].map(|()| i32::try_from(rng.signed(6_000)).unwrap());
    let reference = SceneFadeReference::new(offset);
    match rng.below(4) {
        0 => reference,
        1 => reference.with_eye_vertical_clamp(0).unwrap(),
        2 => reference.with_eye_vertical_clamp(10).unwrap(),
        _ => reference
            .with_eye_vertical_clamp(u32::try_from(rng.below(1_000_001)).unwrap())
            .unwrap(),
    }
}

/// Bands in millimetres: smooth, terminal, steps of both kinds, narrow, wide and far.
fn band(rng: &mut Rng) -> SceneFadeBand {
    let start = match rng.below(4) {
        0 => 0,
        1 => u32::try_from(rng.below(2_000)).unwrap(),
        2 => u32::try_from(rng.below(400_000)).unwrap(),
        _ => u32::try_from(rng.below(5_000_000)).unwrap(),
    };
    let width = match rng.below(4) {
        0 => 0,
        1 => 1,
        2 => u32::try_from(rng.below(5_000)).unwrap(),
        _ => u32::try_from(rng.below(200_000)).unwrap(),
    };
    let band = SceneFadeBand::new(start, start + width).unwrap();
    if rng.below(2) == 0 {
        band.with_terminal_fade()
    } else {
        band
    }
}

/// Eyes at the band's edges from `center`, at and around them by a few units in the last
/// place up to metres, plus a few anywhere.
fn eyes(rng: &mut Rng, center: Vec3, band: [f32; 2]) -> Vec<Vec3> {
    let mut targets = vec![0.0, 1.0e-6, 0.01, rng.unit() * 900.0];
    for edge in band.map(f32::abs) {
        for offset in [0.0, 1.0e-4, 1.0e-3, 0.01, 0.3, 3.0] {
            targets.extend([edge + offset, (edge - offset).max(0.0)]);
        }
        let mut below = edge;
        let mut above = edge;
        for _ in 0..4 {
            below = below.next_down().max(0.0);
            above = above.next_up();
            targets.extend([below, above]);
        }
    }
    targets
        .into_iter()
        .map(|target| center + rng.direction() * target)
        .collect()
}

fn bits(span: Span) -> [u32; 2] {
    [span.lo.to_bits(), span.hi.to_bits()]
}

const DIRECTIONS: [SceneFadeDirection; 3] = [
    SceneFadeDirection::Inclusive,
    SceneFadeDirection::Keep,
    SceneFadeDirection::Complement,
];

#[test]
fn settled_estimates_equal_the_interval_coverage_bit_for_bit() {
    let mut rng = Rng(0x5eed_1234_abcd_0001);
    let (mut settled, mut unsettled) = (0_u32, 0_u32);
    for case in 0..3_000 {
        let meters = [1, 300, 3_000, 200_000, 4_000_000][case % 5];
        let transform = transform(&mut rng, meters);
        let anchor = if rng.below(3) == 0 {
            let origin = crate::geometry::millimeters_to_meters(transform.position_millimeters);
            Anchor::origin(origin.to_array())
        } else {
            Anchor::referenced(&transform, reference(&mut rng))
        };
        let raw = fade_band_meters(band(&mut rng));
        for direction in DIRECTIONS {
            let fade = Fade {
                band: None,
                reference: None,
                direction,
                quantization: SceneFadeQuantization::Continuous,
                coverage: SceneFadeCoverage::Hashed,
                threshold_above_zero: false,
            };
            let band = fade.directed(raw);
            for eye in eyes(&mut rng, anchor.middle(), band) {
                let span = anchor.span(eye);
                let Some(estimate) = anchor.estimate(eye) else {
                    continue;
                };
                let [low, high] = estimate.bounds();
                assert!(
                    low <= f64::from(span.lo) && f64::from(span.hi) <= high,
                    "{anchor:?} {eye:?}: {span:?} outside {low} {high}"
                );
                if let Some(opacity) = estimate.coverage(band) {
                    assert_eq!(
                        bits(opacity),
                        bits(distance_coverage(span, band)),
                        "{anchor:?} {eye:?} {band:?}: {span:?}"
                    );
                    settled += 1;
                } else {
                    unsettled += 1;
                }
            }
        }
    }
    assert!(
        settled > 100_000 && unsettled > 10_000,
        "{settled} {unsettled}"
    );
}

fn grid(zero: bool) -> SceneTexture {
    let mut bytes: Vec<u8> = (1..=16).map(|v| v * 15).collect();
    if zero {
        bytes[3] = 0;
    }
    SceneTexture::Texels {
        format: TexelFormat::R8Unorm,
        color_space: TextureColorSpace::Linear,
        width: 4,
        height: 4,
        mips: 1,
        bytes: bytes.into(),
    }
}

fn material(rng: &mut Rng, variant: u64) -> SceneMaterial {
    let mut material = SceneMaterial::new(SceneMaterialParts {
        albedo: TextureHandle::from_raw(0),
        normal: None,
        roughness_metallic: None,
        ambient_occlusion: None,
        self_illumination: None,
        roughness_micro: 1_000_000,
        metalness_micro: 0,
        alpha_mode: SceneAlphaMode::Opaque,
        double_sided: true,
    })
    .unwrap()
    .with_fade_direction(DIRECTIONS[usize::try_from(variant % 3).unwrap()])
    .with_fade_quantization(if (variant / 3).is_multiple_of(2) {
        SceneFadeQuantization::Continuous
    } else {
        SceneFadeQuantization::Byte
    });
    if let texture @ (1 | 2) = variant / 6 % 3 {
        material = material.with_fade_coverage(SceneFadeCoverage::Threshold {
            texture: TextureHandle::from_raw(u32::try_from(texture).unwrap()),
            jitter_pixels: [0; 2],
        });
    }
    if variant / 18 % 2 == 1 {
        material = material.with_fade_reference(reference(rng));
    }
    if variant / 36 % 2 == 1 {
        let impostor = SceneImpostor::new(SceneImpostorLayout::AzimuthElevation, [1, 1], 0, false)
            .unwrap()
            .with_fade(band(rng));
        material = material.with_impostor(impostor);
    }
    material
}

#[test]
fn the_fade_test_equals_the_reference_for_every_material_kind() {
    let textures = [grid(false), grid(false), grid(true)];
    let mut rng = Rng(0x0dd_f00d_5eed_0002);
    let mut outcomes = [0_u32; 2];
    for variant in 0..72 {
        for _ in 0..6 {
            let material = material(&mut rng, variant);
            let fade = Fade::new(&material, &textures);
            let reference = reference::Fade::new(&material, &textures);
            for case in 0..40 {
                let mut object = SceneObject::new(
                    transform(&mut rng, [2, 500, 3_000, 100_000][case % 4]),
                    MeshHandle::from_raw(0),
                    MaterialHandle::from_raw(0),
                );
                if rng.below(3) != 0 {
                    object = object.with_fade(band(&mut rng));
                }
                let Some(raw) = object
                    .fade()
                    .or(material.impostor().and_then(SceneImpostor::fade))
                else {
                    let eye = Vec3::new(1.0, 2.0, 3.0);
                    assert!(!fade.empty_alone(object, eye));
                    continue;
                };
                let center = fade.reference.map_or_else(
                    || {
                        let origin = object.transform().position_millimeters;
                        crate::geometry::millimeters_to_meters(origin)
                    },
                    |authored| Anchor::referenced(object.transform(), authored).middle(),
                );
                let band = fade.directed(fade_band_meters(raw));
                for eye in eyes(&mut rng, center, band) {
                    let expected =
                        reference.empty(object, eye, &mut reference::Distances::default());
                    assert_eq!(
                        fade.empty_alone(object, eye),
                        expected,
                        "{material:?} {object:?} {eye:?}"
                    );
                    outcomes[usize::from(expected)] += 1;
                }
            }
        }
    }
    assert!(outcomes[0] > 50_000 && outcomes[1] > 50_000, "{outcomes:?}");
}

#[test]
fn huge_and_non_finite_eyes_leave_the_fade_to_the_intervals() {
    let textures = [grid(false), grid(false), grid(true)];
    let mut rng = Rng(0x7777_0000_1234_0003);
    let mut checked = 0_u32;
    for variant in 0..72 {
        let material = material(&mut rng, variant);
        let fade = Fade::new(&material, &textures);
        let reference = reference::Fade::new(&material, &textures);
        let object = SceneObject::new(
            transform(&mut rng, 3_000),
            MeshHandle::from_raw(0),
            MaterialHandle::from_raw(0),
        )
        .with_fade(
            SceneFadeBand::new(2_000, 6_000)
                .unwrap()
                .with_terminal_fade(),
        );
        for magnitude in [1.0e17_f32, 6.0e17, 1.0e18, 2.0e19, 1.0e25, f32::MAX] {
            for eye in [
                Vec3::splat(magnitude),
                Vec3::new(-magnitude, 0.0, magnitude),
                Vec3::new(0.0, 0.0, -magnitude),
            ] {
                assert_eq!(
                    fade.empty_alone(object, eye),
                    reference.empty(object, eye, &mut reference::Distances::default()),
                    "{material:?} {eye:?}"
                );
                checked += 1;
            }
        }
        for eye in [
            Vec3::INFINITY,
            Vec3::NAN,
            Vec3::new(1.0, f32::NEG_INFINITY, 0.0),
        ] {
            assert!(!fade.empty_alone(object, eye));
        }
    }
    assert!(checked > 1_000);
}
