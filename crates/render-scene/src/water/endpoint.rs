//! Endpoint seeds are relative to the integer domain before float narrowing.
use worldrunner_render_plan::{SceneWaterBody, SceneWaterSeedCoordinates, SceneWaterSurface};

pub(super) const SPECIALIZATION: u32 = 256;
pub(super) fn specialize(source: &str, terms: u32) -> std::borrow::Cow<'_, str> {
    if terms & SPECIALIZATION != 0 {
        std::borrow::Cow::Owned(self::source(source))
    } else {
        std::borrow::Cow::Borrowed(source)
    }
}

pub(super) fn enabled(body: &SceneWaterBody) -> bool {
    body.material
        .flow_coordinates
        .is_some_and(|coordinates| coordinates.mode == SceneWaterSeedCoordinates::EndpointClamped)
}

fn scalar(value: i128) -> f64 {
    value
        .to_string()
        .parse()
        .expect("integer is a finite float")
}

fn narrow(value: f64) -> f32 {
    value.to_string().parse().expect("bounded seed coordinate")
}

pub(super) fn coordinates(body: &SceneWaterBody) -> Option<Vec<[f32; 2]>> {
    if !enabled(body) {
        return None;
    }
    let domain = body.material.flow_coordinates.expect("endpoint domain");
    let point = |position: [i64; 2]| {
        std::array::from_fn(|axis| {
            narrow(
                scalar(i128::from(position[axis]) - i128::from(domain.origin_millimeters[axis]))
                    / f64::from(domain.tile_millimeters[axis]),
            )
        })
    };
    let minimum = [body.east_min_millimeters, body.north_min_millimeters];
    let extent = [body.east_extent_millimeters, body.north_extent_millimeters];
    let maximum = std::array::from_fn(|axis| {
        minimum[axis] + i64::try_from(extent[axis]).expect("validated extent")
    });
    Some(match &body.surface {
        SceneWaterSurface::Rectangle => {
            let [x, y] = point(minimum);
            let [east, north] = point(maximum);
            rectangle([x, y], [east, north])
        }
        SceneWaterSurface::Mesh {
            positions_millimeters,
            indices,
        } => indices
            .iter()
            .map(|index| {
                point(positions_millimeters[usize::try_from(*index).expect("validated index")])
            })
            .collect(),
        SceneWaterSurface::Basin {
            columns,
            rows,
            bed_millimeters,
        } => {
            let [x, y] = point(minimum);
            let [east, north] = std::array::from_fn(|axis| {
                narrow(scalar(i128::from(extent[axis])) / f64::from(domain.tile_millimeters[axis]))
            });
            super::geometry::basin(
                [x, y, east, north],
                super::number(body.height_millimeters),
                *columns,
                *rows,
                bed_millimeters,
            )
            .into_iter()
            .map(|p| [p[0], p[1]])
            .collect()
        }
    })
}

fn rectangle([x, y]: [f32; 2], [east, north]: [f32; 2]) -> Vec<[f32; 2]> {
    vec![
        [x, y],
        [east, y],
        [east, north],
        [x, y],
        [east, north],
        [x, north],
    ]
}

pub(super) fn layout() -> wgpu::VertexBufferLayout<'static> {
    const ATTRIBUTES: [wgpu::VertexAttribute; 1] = wgpu::vertex_attr_array![1 => Float32x2];
    wgpu::VertexBufferLayout {
        array_stride: 8,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &ATTRIBUTES,
    }
}

pub(super) fn source(source: &str) -> String {
    source.replace("@location(1) wind: vec2<f32>,", "@location(1) wind: vec2<f32>, @location(2) @interpolate(perspective, sample) seed: vec2<f32>,")
        .replace("world: vec3<f32>) -> WaterVertex", "world: vec3<f32>, @location(1) seed: vec2<f32>) -> WaterVertex")
        .replace("output.world = world;", "output.world = world;\n    output.seed = seed;")
        .replace("if (WATER_TERMS & 64u) != 0u && water.composition.z != 0.0 {\n        flow_uv = (input.world.xy - water.flow_coordinates.xy) * water.flow_coordinates.zw;\n    }",
            "flow_uv = input.seed;")
}
