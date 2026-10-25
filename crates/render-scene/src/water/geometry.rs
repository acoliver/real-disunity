//! Independently generated Z-up water surfaces, clipped against authored basin beds.
use worldrunner_render_plan::{SceneWaterBody, SceneWaterSurface};
use worldrunner_render_post::FrameError;

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

fn basin(
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
