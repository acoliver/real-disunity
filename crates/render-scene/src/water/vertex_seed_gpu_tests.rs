use super::*;
use worldrunner_render_plan::{
    SceneWaterSeedCoordinates as Address, SceneWaterSeedSampling as Stage,
};

fn lattice() -> Vec<u8> {
    (0u16..17)
        .flat_map(|y| {
            (0u16..17).flat_map(move |x| {
                [
                    u8::try_from((11 * x + 3 * y + 7 * x * y) % 251).unwrap(),
                    u8::try_from((2 * x + 13 * y + 11 * x * y) % 253).unwrap(),
                ]
            })
        })
        .collect()
}

// Independent tent-basis oracle, including periodic copies for Repeat addressing.
fn sample(bytes: &[u8], side: u16, point: [f64; 2], mode: Address) -> [f64; 2] {
    let n = f64::from(side);
    let coordinate = point.map(|v| match mode {
        Address::EndpointClamped => v.clamp(0.0, 1.0) * (n - 1.0),
        Address::Repeat => (v - v.floor()) * n - 0.5,
    });
    let weight = |axis: usize, index: u16| {
        let delta = coordinate[axis] - f64::from(index);
        let distance = match mode {
            Address::EndpointClamped => delta.abs(),
            Address::Repeat => delta.abs().min((delta + n).abs()).min((delta - n).abs()),
        };
        (1.0 - distance).max(0.0)
    };
    std::array::from_fn(|channel| {
        (0..side)
            .flat_map(|y| (0..side).map(move |x| (x, y)))
            .map(|(x, y)| {
                weight(0, x)
                    * weight(1, y)
                    * f64::from(
                        bytes[(usize::from(y) * usize::from(side) + usize::from(x)) * 2 + channel],
                    )
                    / 255.0
            })
            .sum()
    })
}

fn oracle(points: [[f64; 2]; 3], weights: [f64; 3], mode: Address) -> [f64; 2] {
    let bytes = lattice();
    std::array::from_fn(|channel| {
        points
            .into_iter()
            .zip(weights)
            .map(|(point, weight)| sample(&bytes, 17, point, mode)[channel] * weight)
            .sum()
    })
}

fn source(perspective: bool) -> String {
    let w = if perspective {
        "array<f32, 3>(1.0, 2.0, 4.0)[vertex % 3u]"
    } else {
        "1.0"
    };
    let source = water_source(false)
        .replace(
            "@vertex fn vs_water(",
            "@vertex fn vs_water(@builtin(vertex_index) vertex: u32, ",
        )
        .replace(
            "output.position = water.clip * vec4<f32>(world, 1.0);",
            "output.position = probe_position(vertex);",
        )
        .replace(
            "vec4<f32>(color, 1.0), receiver_normal",
            "vec4<f32>(flow_velocity(flow_uv) * 0.5 + 0.5, 0.0, 1.0), receiver_normal",
        );
    format!(
        "{source}\nfn probe_position(vertex: u32) -> vec4<f32> {{
        let xy = array<vec2<f32>, 3>(vec2<f32>(-0.56875, -0.63125),
            vec2<f32>(1.23125, -0.63125), vec2<f32>(-0.56875, 1.16875));
        let w = {w};
        return vec4<f32>(xy[vertex % 3u] * w, 0.5 * w, w);
    }}"
    )
}

fn probe(
    context: &GpuContext,
    points: [[i64; 2]; 3],
    mode: Address,
    stage: Stage,
    perspective: bool,
) -> [f64; 2] {
    probe_at_origin(context, points, mode, stage, perspective, [0; 2])
}

fn probe_at_origin(
    context: &GpuContext,
    points: [[i64; 2]; 3],
    mode: Address,
    stage: Stage,
    perspective: bool,
    origin: [i32; 2],
) -> [f64; 2] {
    let mut body = seed_tests::body(mode);
    body.material.flow_seed_sampling = stage;
    let domain = body.material.flow_coordinates.as_mut().unwrap();
    domain.origin_millimeters = origin;
    domain.tile_millimeters = [10_000; 2];
    body.surface = worldrunner_render_plan::SceneWaterSurface::Mesh {
        positions_millimeters: Box::new(points),
        indices: Box::new([0, 1, 2]),
    };
    let mut renderer = regressions::renderer(context, 1);
    renderer.install_water(context.device(), body).unwrap();
    let water = renderer.water.as_mut().unwrap();
    seed_tests::install_lattice(context, water, &lattice());
    let options = [
        0,
        0,
        0,
        0,
        terms::mask(&water.settings) | super::super::vertex_seeds::key(&water.settings[0]),
    ];
    let pipeline = pipeline_with_source(
        context.device(),
        1,
        &water.layout,
        false,
        options,
        &source(perspective),
    );
    water
        .specializations
        .borrow_mut()
        .insert(options, [pipeline.clone(), pipeline]);
    let bytes = seed_tests::render_probe(context, &mut renderer);
    std::array::from_fn(|channel| {
        let start = (16 * 32 + 16) * 8 + channel * 2;
        f64::from(numerical::positive_half(&bytes[start..start + 2]))
    })
}

fn normalized(points: [[i64; 2]; 3]) -> [[f64; 2]; 3] {
    points.map(|point| point.map(|v| f64::from(i32::try_from(v).unwrap()) / 10_000.0))
}

fn assert_seed(actual: [f64; 2], expected: [f64; 2]) {
    let tolerance = 1.0 / 1024.0 + 128.0 * f64::from(f32::EPSILON);
    for channel in 0..2 {
        assert!(
            (actual[channel] - expected[channel]).abs() <= tolerance,
            "channel {channel}: {actual:?} vs {expected:?}"
        );
    }
}

#[test]
fn vertex_seed_centroid_matches_interpolated_oracle_not_fragment_sampling() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    let points = [[0, 0], [600, 0], [0, 600]];
    let expected = oracle(normalized(points), [1.0 / 3.0; 3], Address::EndpointClamped);
    let sampled = sample(&lattice(), 17, [0.02, 0.02], Address::EndpointClamped);
    assert!(((sampled[0] - expected[0]).abs() - 0.002_810_98).abs() < 0.000_001);
    let vertex = probe(
        &context,
        points,
        Address::EndpointClamped,
        Stage::Vertex,
        false,
    );
    let fragment = probe(
        &context,
        points,
        Address::EndpointClamped,
        Stage::Fragment,
        false,
    );
    assert_seed(vertex, expected);
    assert_seed(fragment, sampled);
    assert!((vertex[0] - fragment[0]).abs() > 0.0025);
    let vertex_samples = points.map(|point| {
        probe(
            &context,
            [point; 3],
            Address::EndpointClamped,
            Stage::Vertex,
            false,
        )
    });
    assert_seed(
        vertex,
        std::array::from_fn(|channel| vertex_samples.iter().map(|v| v[channel]).sum::<f64>() / 3.0),
    );
}

#[test]
fn vertex_seed_addressing_covers_corners_clamps_and_independent_tile_edges() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    for mode in [Address::Repeat, Address::EndpointClamped] {
        for points in [
            [[0, 0], [600, 0], [0, 600]],
            [[0, 0]; 3],
            [[10_000, 0]; 3],
            [[0, 10_000]; 3],
            [[10_000, 10_000]; 3],
            [[-600, 3000], [0, 3600], [600, 3000]],
            [[10_000, 0], [10_600, 0], [10_000, 600]],
        ] {
            assert_seed(
                probe(&context, points, mode, Stage::Vertex, false),
                oracle(normalized(points), [1.0 / 3.0; 3], mode),
            );
        }
    }
}

#[test]
fn vertex_seed_varying_is_perspective_correct() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    let points = [[0, 0], [600, 0], [0, 600]];
    let weights = [4.0 / 7.0, 2.0 / 7.0, 1.0 / 7.0];
    assert_seed(
        probe(
            &context,
            points,
            Address::EndpointClamped,
            Stage::Vertex,
            true,
        ),
        oracle(normalized(points), weights, Address::EndpointClamped),
    );
}

#[test]
fn vertex_seed_adjacent_tiles_address_the_shared_edge_independently() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    let edge = [[10_000, 5000]; 3];
    let west = probe_at_origin(
        &context,
        edge,
        Address::EndpointClamped,
        Stage::Vertex,
        false,
        [0; 2],
    );
    let east = probe_at_origin(
        &context,
        edge,
        Address::EndpointClamped,
        Stage::Vertex,
        false,
        [10_000, 0],
    );
    assert_seed(
        west,
        oracle([[1.0, 0.5]; 3], [1.0 / 3.0; 3], Address::EndpointClamped),
    );
    assert_seed(
        east,
        oracle([[0.0, 0.5]; 3], [1.0 / 3.0; 3], Address::EndpointClamped),
    );
    assert!((west[0] - east[0]).abs() > 0.1);
    assert!((west[1] - east[1]).abs() > 0.1);
}

#[test]
fn vertex_seed_cpu_oracle_samples_before_barycentric_interpolation() {
    let points = [[0.0, 0.0], [0.06, 0.0], [0.0, 0.06]];
    let expected = oracle(points, [1.0 / 3.0; 3], Address::EndpointClamped);
    let manual = [
        0.96 * 11.0 / 3.0 / 255.0 + 0.96 * 3.0 / 3.0 / 255.0,
        0.96 * 2.0 / 3.0 / 255.0 + 0.96 * 13.0 / 3.0 / 255.0,
    ];
    for channel in 0..2 {
        assert!((expected[channel] - manual[channel]).abs() < 1e-14);
    }
    let centroid = sample(&lattice(), 17, [0.02, 0.02], Address::EndpointClamped);
    assert!(((centroid[0] - expected[0]) - 0.002_810_980_392_156_863).abs() < 1e-14);
}
