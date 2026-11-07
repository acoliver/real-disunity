use super::*;
use worldrunner_render_plan::{MaterialHandle, MeshHandle, SceneRotation, SceneTransform};

#[test]
fn distance_bounds_enclose_shader_order_and_fma_at_transformed_fade_boundaries() {
    for scale_millionths in [1, 500_000, 1_500_000, 10_000_000] {
        for rotation in [
            SceneRotation::IDENTITY,
            SceneRotation {
                x: 0,
                y: 0,
                z: 707_107,
                w: 707_107,
            },
        ] {
            let transform = SceneTransform {
                position_millimeters: [1_000_000, -2_000_000, 500],
                rotation,
                scale_millionths,
            };
            check_reference(transform);
        }
    }
}

fn check_reference(transform: SceneTransform) {
    let model = model_matrix(&transform);
    let local = [1.0, 2.0, 1.0];
    let center = model.w_axis.truncate()
        + model.x_axis.truncate() * local[0]
        + model.y_axis.truncate() * local[1]
        + model.z_axis.truncate() * local[2];
    let band = SceneFadeBand::new(2000, 6000).unwrap().with_terminal_fade();
    let object = SceneObject::new(
        transform,
        MeshHandle::from_raw(0),
        MaterialHandle::from_raw(0),
    )
    .with_fade(band);
    for clamp in [None, Some(0), Some(1000)] {
        let mut reference = SceneFadeReference::new([1000, 2000, 1000]);
        if let Some(clamp) = clamp {
            reference = reference.with_eye_vertical_clamp(clamp).unwrap();
        }
        let policy = Fade {
            reference: Some(reference),
            band: None,
            direction: SceneFadeDirection::Keep,
            quantization: SceneFadeQuantization::Continuous,
            coverage: SceneFadeCoverage::Hashed,
        };
        for distance in [
            2.0_f32.next_down(),
            2.0,
            2.0_f32.next_up(),
            6.0_f32.next_down(),
            6.0,
            6.0_f32.next_up(),
            7.0,
        ] {
            for offset in [
                Vec3::Y * distance,
                Vec3::NEG_Z * distance,
                Vec3::ONE.normalize() * distance,
            ] {
                let eye = center + offset;
                let bounded = policy.distance(object, eye);
                for fused in [false, true] {
                    let actual = shader_distance(model, local, eye, clamp, fused);
                    assert!(
                        bounded.lo <= actual && bounded.hi >= actual,
                        "{transform:?}, {eye:?}, {clamp:?}, {fused}: {bounded:?} versus {actual}"
                    );
                    if policy.empty(object, eye) {
                        assert!(actual >= 6.0);
                    }
                }
            }
        }
    }
}

fn shader_distance(
    model: glam::Mat4,
    local: [f32; 3],
    mut eye: Vec3,
    clamp: Option<u32>,
    fused: bool,
) -> f32 {
    let [a, b, c, pivot] = model.to_cols_array_2d();
    let point: [f32; 3] = std::array::from_fn(|axis| {
        if fused {
            a[axis].mul_add(local[0], b[axis].mul_add(local[1], c[axis] * local[2])) + pivot[axis]
        } else {
            pivot[axis] + ((a[axis] * local[0] + b[axis] * local[1]) + c[axis] * local[2])
        }
    });
    if let Some(clamp) = clamp {
        eye.z = eye.z.max(point[2] + millimeter_to_meter(i64::from(clamp)));
    }
    (eye - Vec3::from_array(point)).length()
}
