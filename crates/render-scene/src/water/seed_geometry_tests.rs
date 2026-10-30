use super::*;
use worldrunner_render_plan::{SceneWaterSeedCoordinates, SceneWaterSurface};

fn probe_body(origin: [i32; 2], extent: u32, offsets: &[[i32; 2]]) -> SceneWaterBody {
    let mut body = seed_tests::body(SceneWaterSeedCoordinates::EndpointClamped);
    let coordinates = body.material.flow_coordinates.as_mut().unwrap();
    coordinates.origin_millimeters = origin;
    coordinates.tile_millimeters = [extent; 2];
    body.east_min_millimeters = i64::from(origin[0]);
    body.north_min_millimeters = i64::from(origin[1]);
    body.east_extent_millimeters = u64::from(extent);
    body.north_extent_millimeters = u64::from(extent);
    body.surface = SceneWaterSurface::Mesh {
        positions_millimeters: offsets
            .iter()
            .map(|offset| {
                std::array::from_fn(|axis| i64::from(origin[axis]) + i64::from(offset[axis]))
            })
            .collect(),
        indices: (0..u32::try_from(offsets.len()).unwrap())
            .flat_map(|index| [index; 6])
            .collect(),
    };
    body
}

fn raster_source() -> String {
    // Only clip positions are replaced. World and seed varyings use uploaded geometry.
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
        "{source}\n{}",
        r"
fn probe_position(vertex: u32) -> vec4<f32> {
    let corners = array<vec2<f32>, 6>(vec2<f32>(0.0, -1.0), vec2<f32>(1.0, -1.0),
        vec2<f32>(1.0, 1.0), vec2<f32>(0.0, -1.0), vec2<f32>(1.0, 1.0), vec2<f32>(0.0, 1.0));
    let corner = corners[vertex % 6u];
    return vec4<f32>((f32(vertex / 6u) + corner.x) / 16.0 - 1.0, corner.y, 0.5, 1.0);
}"
    )
}

pub(super) fn probe_geometry(context: &GpuContext, body: SceneWaterBody) -> Vec<u8> {
    probe_source(context, body, &raster_source())
}

fn probe_source(context: &GpuContext, body: SceneWaterBody, source: &str) -> Vec<u8> {
    let mut renderer = regressions::renderer(context, 1);
    renderer.install_water(context.device(), body).unwrap();
    let water = renderer.water.as_mut().unwrap();
    seed_tests::install_field(context, water);
    let mask = terms::mask(&water.settings)
        | if endpoint::enabled(&water.settings[0]) {
            endpoint::SPECIALIZATION
        } else {
            0
        };
    let options = [0, 0, 0, 0, mask];
    let pipeline = pipeline_with_source(context.device(), 1, &water.layout, false, options, source);
    water
        .specializations
        .borrow_mut()
        .insert(options, [pipeline.clone(), pipeline]);
    seed_tests::render_probe(context, &mut renderer)
}

#[test]
fn endpoint_seed_large_origins_use_integer_geometry_before_float_narrowing() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    for origin in [
        [2_000_000_000, 1_900_000_000],
        [-2_000_000_000, -1_800_000_000],
    ] {
        for extent in [300, 100] {
            let extent_i32 = i32::try_from(extent).unwrap();
            let offsets = [
                [0, 0],
                [extent_i32, extent_i32],
                [50, 50],
                [extent_i32 / 2, extent_i32 / 4],
            ];
            let actual = probe_geometry(&context, probe_body(origin, extent, &offsets));
            for (index, offset) in offsets.into_iter().enumerate() {
                let point = offset.map(|value| f64::from(value) / f64::from(extent));
                for (channel, expected) in seed_tests::oracle(point).into_iter().enumerate() {
                    let start = (16 * 32 + index) * 8 + channel * 2;
                    let value = f64::from(numerical::positive_half(&actual[start..start + 2]));
                    let tolerance = 1.0 / 1024.0 + 128.0 * f64::from(f32::EPSILON);
                    assert!(
                        (value - expected).abs() <= tolerance,
                        "origin {origin:?}, extent {extent}, offset {offset:?}, channel {channel}: {value} vs {expected}"
                    );
                }
            }
        }
    }
}

#[test]
fn endpoint_seed_translated_rectangles_and_basins_interpolate_domain_coordinates() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    let source = raster_source().replace(
        "(f32(vertex / 6u) + corner.x) / 16.0 - 1.0",
        "corner.x * 2.0 - 1.0",
    );
    for origin in [
        [2_000_000_000, 1_900_000_000],
        [-2_000_000_000, -1_800_000_000],
    ] {
        for surface in [
            SceneWaterSurface::Rectangle,
            SceneWaterSurface::Basin {
                columns: 2,
                rows: 2,
                bed_millimeters: Box::new([-1000; 4]),
            },
        ] {
            let mut body = probe_body(origin, 300, &[]);
            body.surface = surface;
            let actual = probe_source(&context, body, &source);
            for x in [0u32, 7, 15, 16, 23, 31] {
                let point = [(f64::from(x) + 0.5) / 32.0, 1.0 - 16.5 / 32.0];
                for (channel, expected) in seed_tests::oracle(point).into_iter().enumerate() {
                    let start = (16 * 32 + usize::try_from(x).unwrap()) * 8 + channel * 2;
                    let value = f64::from(numerical::positive_half(&actual[start..start + 2]));
                    let tolerance = 1.0 / 1024.0 + 128.0 * f64::from(f32::EPSILON);
                    assert!(
                        (value - expected).abs() <= tolerance,
                        "origin {origin:?}, point {point:?}, channel {channel}: {value} vs {expected}"
                    );
                }
            }
        }
    }
}

#[test]
fn repeat_vertex_layout_has_no_endpoint_attribute_or_buffer() {
    let body = seed_tests::body(SceneWaterSeedCoordinates::Repeat);
    assert!(endpoint::coordinates(&body).is_none());
    let layouts = geometry::layouts(false);
    assert_eq!(layouts.len(), 1);
    let layout = layouts[0].as_ref().unwrap();
    assert_eq!(layout.array_stride, 12);
    assert_eq!(layout.step_mode, wgpu::VertexStepMode::Vertex);
    assert_eq!(layout.attributes, wgpu::vertex_attr_array![0 => Float32x3]);
    assert_eq!(geometry::layouts(true).len(), 2);
    let source = endpoint::source(&water_source(false));
    assert!(source.contains("flow_uv = input.seed;"));
    assert!(!source.contains("input.world.xy - water.flow_coordinates.xy"));
}
