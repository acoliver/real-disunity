//! Independently generated Z-up water surfaces, clipped against authored basin beds.
use worldrunner_render_plan::{SceneWaterBody, SceneWaterSurface};
use worldrunner_render_post::FrameError;

pub(crate) fn allocations(
    body: &SceneWaterBody,
    set: &mut worldrunner_render_gpu::configuration::AllocationSet,
) {
    set.buffer("water staging", super::STAGING_BYTES, 3);
    set.buffer("water body uniforms", super::UNIFORM_BYTES, 3);
    let (positions, seeds) = allocation_sizes(body);
    set.buffer("water surface vertices", positions, 1);
    if let Some(bytes) = seeds {
        set.buffer("water endpoint seed coordinates", bytes, 1);
    }
}

fn seed_buffer_bytes(vertices: u64) -> u64 {
    vertices
        .saturating_mul(std::mem::size_of::<[f32; 2]>() as u64)
        .max(8)
}

pub(crate) fn allocation_sizes(body: &SceneWaterBody) -> (u64, Option<u64>) {
    let vertices = match &body.surface {
        SceneWaterSurface::Rectangle => 6,
        SceneWaterSurface::Mesh { indices, .. } => {
            u64::try_from(indices.len()).expect("bounded geometry")
        }
        // Each bed triangle clips to at most two surface triangles.
        SceneWaterSurface::Basin { columns, rows, .. } => u64::from(columns.saturating_sub(1))
            .saturating_mul(u64::from(rows.saturating_sub(1)))
            .saturating_mul(12),
    };
    (
        vertices.saturating_mul(12).max(12),
        super::endpoint::enabled(body).then(|| seed_buffer_bytes(vertices)),
    )
}

pub(super) fn vertices(body: &SceneWaterBody) -> Result<Vec<[f32; 3]>, FrameError> {
    super::validate_settings(body)?;
    let height = super::number(body.height_millimeters);
    let point = |x, y| [x, y, height];
    let x = super::number(body.east_min_millimeters);
    let y = super::number(body.north_min_millimeters);
    let east =
        super::number(i64::try_from(body.east_extent_millimeters).expect("validated extent"));
    let north =
        super::number(i64::try_from(body.north_extent_millimeters).expect("validated extent"));
    match &body.surface {
        SceneWaterSurface::Rectangle => Ok(vec![
            point(x, y),
            point(x + east, y),
            point(x + east, y + north),
            point(x, y),
            point(x + east, y + north),
            point(x, y + north),
        ]),
        SceneWaterSurface::Mesh {
            positions_millimeters,
            indices,
        } => Ok(indices
            .iter()
            .map(|index| {
                let [east, north] =
                    positions_millimeters[usize::try_from(*index).expect("validated index")];
                point(super::number(east), super::number(north))
            })
            .collect()),
        SceneWaterSurface::Basin {
            columns,
            rows,
            bed_millimeters,
        } => Ok(basin(
            [x, y, east, north],
            height,
            *columns,
            *rows,
            bed_millimeters,
        )),
    }
}

pub(super) fn basin(
    bounds: [f32; 4],
    height: f32,
    columns: u32,
    rows: u32,
    bed_millimeters: &[i64],
) -> Vec<[f32; 3]> {
    let [x, y, east, north] = bounds;
    let columns = usize::try_from(columns).expect("validated dimensions");
    let rows = usize::try_from(rows).expect("validated dimensions");
    let denominator = |n: usize| n.to_string().parse::<f32>().expect("bounded dimension");
    let sample = |column: usize, row: usize| {
        (
            [
                x + east * denominator(column) / denominator(columns - 1),
                y + north * denominator(row) / denominator(rows - 1),
            ],
            super::number(bed_millimeters[row * columns + column]),
        )
    };
    let mut output = Vec::new();
    for row in 0..rows - 1 {
        for column in 0..columns - 1 {
            let southwest = sample(column, row);
            let southeast = sample(column + 1, row);
            let northeast = sample(column + 1, row + 1);
            let northwest = sample(column, row + 1);
            for triangle in [
                [southwest, southeast, northeast],
                [southwest, northeast, northwest],
            ] {
                clip_triangle(&mut output, triangle, height);
            }
        }
    }
    output
}

fn clip_triangle(output: &mut Vec<[f32; 3]>, triangle: [([f32; 2], f32); 3], height: f32) {
    let point = |x, y| [x, y, height];
    let mut polygon = Vec::new();
    for edge in 0..3 {
        let (a, bed_a) = triangle[edge];
        let (b, bed_b) = triangle[(edge + 1) % 3];
        if bed_a < height {
            polygon.push(a);
        }
        if (bed_a < height) != (bed_b < height) {
            let t = (height - bed_a) / (bed_b - bed_a);
            polygon.push([a[0] + t * (b[0] - a[0]), a[1] + t * (b[1] - a[1])]);
        }
    }
    for index in 1..polygon.len().saturating_sub(1) {
        for p in [polygon[0], polygon[index], polygon[index + 1]] {
            output.push(point(p[0], p[1]));
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct Body {
    pub(super) buffer: wgpu::Buffer,
    pub(super) seed: Option<wgpu::Buffer>,
    pub(super) vertices: u32,
    pub(super) uniforms: [wgpu::Buffer; 3],
}
impl Body {
    pub(super) fn new(
        device: &wgpu::Device,
        vertices: &[[f32; 3]],
        seeds: Option<&[[f32; 2]]>,
    ) -> Self {
        use wgpu::util::DeviceExt;
        let bytes: Vec<_> = vertices
            .iter()
            .flatten()
            .flat_map(|v| v.to_le_bytes())
            .collect();
        Self {
            seed: seeds.map(|seeds| {
                assert_eq!(seeds.len(), vertices.len());
                let mut bytes: Vec<_> = seeds
                    .iter()
                    .flatten()
                    .flat_map(|v| v.to_le_bytes())
                    .collect();
                bytes.resize(
                    usize::try_from(seed_buffer_bytes(seeds.len() as u64))
                        .expect("bounded seed buffer"),
                    0,
                );
                device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("water endpoint seed coordinates"),
                    contents: &bytes,
                    usage: wgpu::BufferUsages::VERTEX,
                })
            }),
            buffer: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("water body surface"),
                contents: if bytes.is_empty() { &[0; 12] } else { &bytes },
                usage: wgpu::BufferUsages::VERTEX,
            }),
            vertices: u32::try_from(vertices.len()).expect("bounded water geometry"),
            uniforms: std::array::from_fn(|_| {
                device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("water body frame"),
                    size: super::UNIFORM_BYTES,
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                })
            }),
        }
    }
}

fn vertex_layout() -> wgpu::VertexBufferLayout<'static> {
    const ATTRIBUTES: [wgpu::VertexAttribute; 1] = wgpu::vertex_attr_array![0 => Float32x3];
    wgpu::VertexBufferLayout {
        array_stride: 12,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &ATTRIBUTES,
    }
}

pub(super) fn layouts(endpoint: bool) -> Vec<Option<wgpu::VertexBufferLayout<'static>>> {
    let mut layouts = vec![Some(vertex_layout())];
    if endpoint {
        layouts.push(Some(super::endpoint::layout()));
    }
    layouts
}
