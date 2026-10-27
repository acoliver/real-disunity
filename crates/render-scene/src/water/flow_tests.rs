use super::*;

fn phases(time: f32, period: f32) -> ([f32; 2], [f32; 2]) {
    let a = (time / period).rem_euclid(1.0);
    let b = (a + 0.5).fract();
    let weight = (1.0 - 2.0 * a).abs();
    ([a, b], [1.0 - weight, weight])
}

#[test]
fn two_phase_weights_partition_unity_and_hide_both_resets() {
    for time in [0.0, 0.25, 0.5, 0.75, 1.0, 12.25] {
        let (phase, weight) = phases(time, 1.0);
        assert!((weight[0] + weight[1] - 1.0).abs() < f32::EPSILON);
        for (phase, weight) in phase.into_iter().zip(weight) {
            if phase == 0.0 {
                assert_eq!(weight.to_bits(), 0.0f32.to_bits());
            }
        }
    }
    let blended = |time| {
        let (phase, weight) = phases(time, 1.0);
        phase[0].sin() * weight[0] + phase[1].sin() * weight[1]
    };
    for reset in [0.5, 1.0, 3.5, 4.0] {
        assert!((blended(reset - 0.00001) - blended(reset + 0.00001)).abs() < 0.0001);
    }
    let (phase, _) = phases(1.0, 4.0);
    let uv = glam::Vec2::new(0.3, 0.7);
    let velocity = glam::Vec2::new(0.5, -0.25);
    let tile_reciprocal = glam::Vec2::splat(0.5);
    assert_eq!(
        uv + velocity * 4.0 * tile_reciprocal * phase[0],
        glam::Vec2::new(0.55, 0.575)
    );
}

fn expected_normal(
    position: glam::Vec3,
    right: bool,
    time: f32,
    field: bool,
    shore: bool,
) -> glam::Vec3 {
    let mut velocity = 0.5
        + if field {
            filtered(
                if shore {
                    (position.x - 1.0) / 3.0
                } else {
                    position.x * 0.5
                },
                [0, 255, 0, 255],
            ) * 2.0
                - 1.0
        } else {
            0.0
        };
    if field && shore {
        let seed = filtered((position.x - 1.0) / 3.0, [0, 255, 0, 255]) * 2.0 - 1.0;
        let force = glam::Vec2::new(1.0, 1.0 / 255.0);
        let direction = force.normalize();
        let strength = (force.x + force.y) * 0.25;
        velocity = 0.5 + (seed * strength.clamp(0.2, 1.0) - strength) * direction.x;
    }
    if shore {
        velocity *= 0.2;
    }
    let (phase, weight) = phases(time, 4.0);
    let fixed = 128.0 / 255.0 * 2.0 - 1.0;
    let normals = phase.map(|phase| {
        let uv = position.x * 0.5 + velocity * 4.0 * 0.5 * phase;
        let q = filtered(uv, [64, 192, 64, 192]) * 2.0 - 1.0;
        let xy = if right {
            glam::Vec2::new(fixed, -q + 2.0 / 255.0)
        } else {
            glam::Vec2::new(q, fixed)
        };
        let z = (1.0 - xy.length_squared()).clamp(0.0, 1.0).sqrt();
        if shore {
            xy.abs().extend(z)
        } else {
            xy.extend(z)
        }
    });
    let mut blended = normals[0] * weight[0] + normals[1] * weight[1];
    if shore {
        blended.x *= -0.5;
        blended.y *= -0.5;
        if field {
            blended.x *= (1.0 + 1.0 / 255.0) * 0.5;
            blended.y *= (1.0 + 1.0 / 255.0) * 0.5;
        }
    }
    blended.normalize()
}

fn check_phased_normal(bytes: &[u8], samples: u32, sample: u32, time: f32, options: (bool, bool)) {
    let (field, shore) = options;
    let offset = if samples == 1 {
        [0.5, 0.5]
    } else {
        [
            [0.375, 0.125],
            [0.875, 0.375],
            [0.125, 0.625],
            [0.625, 0.875],
        ][usize::try_from(sample).unwrap()]
    };
    let frame = FrameUniform::new(&fixture_view(), None).unwrap();
    let inverse = frame.view_projection.inverse();
    for (x, right) in [(12u16, false), (20, true)] {
        let unproject = |z| {
            let p = inverse
                * glam::Vec4::new(
                    (f32::from(x) + offset[0]) / 16.0 - 1.0,
                    1.0 - (16.0 + offset[1]) / 16.0,
                    z,
                    1.0,
                );
            p.truncate() / p.w
        };
        let a = unproject(0.0);
        let b = unproject(1.0);
        let position = a + (b - a) * (-a.z / (b.z - a.z));
        let expected = frame
            .view
            .transform_vector3(expected_normal(position, right, time, field, shore));
        let base = (16 * 32 + usize::from(x)) * 8;
        for (channel, target) in expected.to_array().into_iter().enumerate() {
            let index = base + channel * 2;
            let bits = u16::from_le_bytes(bytes[index..index + 2].try_into().unwrap());
            let value = numerical::positive_half(&(bits & 0x7fff).to_le_bytes())
                * if bits & 0x8000 == 0 { 1.0 } else { -1.0 };
            assert!(
                (value - target).abs() < 0.006,
                "two-phase normal {samples}x sample {sample}, t={time}, field={field}, x={x}, channel={channel}, world={position:?}: {value} versus {target}"
            );
        }
    }
}

fn phased_scene(field: bool, shore: bool) -> (Vec<SceneWaterBody>, RenderScene) {
    let (mut bodies, _) = authored_scene();
    for body in &mut bodies {
        body.flow_millimeters_per_second = [500, -250];
        body.material.flow_period_milliseconds = Some(4000);
        if !field {
            body.material.flow = None;
        }
    }
    bodies[0].surface = SceneWaterSurface::Basin {
        columns: 2,
        rows: 2,
        bed_millimeters: vec![-1000; 4].into_boxed_slice(),
    };
    bodies[1].surface = SceneWaterSurface::Mesh {
        positions_millimeters: vec![[0, -10000], [10000, -10000], [10000, 10000], [0, 10000]]
            .into_boxed_slice(),
        indices: vec![0, 1, 2, 0, 2, 3].into_boxed_slice(),
    };
    if field && shore {
        for body in &mut bodies {
            body.material.wind = Some(worldrunner_render_plan::SceneWaterWind {
                field: worldrunner_render_plan::ScenePlanarWindField::new(
                    TextureHandle::from_raw(3),
                    [0; 2],
                    [1000; 2],
                    [None; 2],
                )
                .unwrap(),
                band: worldrunner_render_plan::SceneWindBand::new(1_000_000, 0, 0).unwrap(),
                amplitude_micro: 1_000_000,
            });
            body.material.flow_coordinates =
                Some(worldrunner_render_plan::SceneWaterFlowCoordinates {
                    origin_millimeters: [1000, 0],
                    tile_millimeters: [3000, 2000],
                });
        }
    }
    if shore {
        for body in &mut bodies {
            body.material.shoreline = Some(SceneWaterShoreline {
                depth_millimeters: 1000,
                normal_gain_micro: -500_000,
                opacity_micro: 1_000_000,
            });
        }
    }
    let source = scene(
        bodies.clone(),
        vec![
            map(vec![64, 128, 192, 128, 64, 128, 192, 128]),
            map(vec![128, 192, 128, 64, 128, 192, 128, 64]),
            map(vec![0, 128, 255, 128, 0, 128, 255, 128]),
            map(vec![255, 128, 255, 128, 255, 128, 255, 128]),
        ],
    );
    (bodies, source)
}

#[test]
fn two_phase_normal_matches_cpu_for_constant_and_sampled_flow_on_basin_and_mesh() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    for samples in [1, 4] {
        for (field, shore) in [(false, false), (true, false), (false, true), (true, true)] {
            let (bodies, source) = phased_scene(field, shore);
            let pipelines = crate::renderer::ScenePipelines::with_samples(
                context.device(),
                wgpu::TextureFormat::Rgba16Float,
                samples,
            )
            .unwrap();
            let mut gpu = crate::draw::SceneGpu::upload(
                context.device(),
                context.submission(),
                &source,
                &pipelines,
            )
            .unwrap();
            let mut renderer = ConfiguredFrameRenderer::new(
                &context,
                configuration(samples, bodies),
                AllocationBudget(10_000_000),
            )
            .unwrap();
            let mut periodic = Vec::new();
            for (index, time) in [0.0, 0.375, 1.9999, 2.0, 2.0001, 3.9999, 4.0, 4.0001]
                .into_iter()
                .enumerate()
            {
                renderer.set_frame_time(time).unwrap();
                encode(&context, &mut renderer, &mut gpu, &pipelines);
                let (normal, _, _) = renderer.water_receivers().unwrap();
                for sample in 0..samples {
                    let bytes = lighting_tests::environment_tests::sample_read(
                        &context,
                        normal.texture(),
                        sample,
                    );
                    check_phased_normal(&bytes, samples, sample, time, (field, shore));
                    if sample == 0 && matches!(index, 0 | 6) {
                        periodic.push(bytes);
                    }
                }
            }
            assert_eq!(
                periodic[0], periodic[1],
                "full period preserves every receiver byte"
            );
            wait(context.device());
        }
    }
}
