use super::*;
use glam::{DVec2, Vec3};
use worldrunner_render_scene::Viewport;

fn silhouette_count(products: &RetainedCaptureProducts) -> usize {
    products
        .normals
        .as_ref()
        .unwrap()
        .pixels
        .iter()
        .filter(|normal| normal[3] > 0.0)
        .count()
}

pub(super) fn assert_depth_mirrored(
    right: &RetainedCaptureProducts,
    left: &RetainedCaptureProducts,
    scene: &RenderScene,
) {
    let silhouettes = [right, left].map(silhouette_count);
    let right = right.depth.as_ref().unwrap();
    let left = left.depth.as_ref().unwrap();
    let camera = crate::tests::options().camera;
    let near = f64::from(camera.near_plane_millimeters) / 1000.0;
    let far = f64::from(camera.far_plane_millimeters) / 1000.0;
    let depth_coefficient = near * far / (far - near);
    let width = right.viewport.width;
    let geometry = [
        ViewHandedness::RightHanded,
        ViewHandedness::DirectXLeftHanded,
    ]
    .map(|handedness| RasterGeometry::new(scene, right.viewport, handedness));
    let mut interiors = 0;
    let mut edges = 0;
    let mut maximum = (0.0_f32, 0, 0);
    let mut maximum_edge = (0.0_f32, 0, 0);
    for y in 0..right.viewport.height {
        for x in 0..width {
            let depth = right.pixels[usize::try_from(y * width + x).unwrap()];
            let actual = left.pixels[usize::try_from(y * width + width - 1 - x).unwrap()];
            let difference = (depth - actual).abs();
            let a = geometry[0].sample(x, y);
            let b = geometry[1].sample(width - 1 - x, y);
            edges += usize::from(a.edge || b.edge);
            if difference > maximum_edge.0 {
                maximum_edge = (difference, x, y);
            }
            if let (Some(a), Some(b)) = (a.interior, b.interior) {
                assert_eq!(
                    a, b,
                    "different interior triangle at {x},{y}, depth difference {difference}"
                );
                // Three weighted vertex depths plus two sums give 5u per plane,
                // or 10u=5*EPSILON across conventions. Captures store metres Z:
                // d=A-B/Z, so delta(d)=B*delta(Z)/(Z1*Z2), not delta(Z).
                let device_difference = depth_coefficient * f64::from(difference)
                    / (f64::from(depth) * f64::from(actual));
                assert!(
                    device_difference <= 5.0 * f64::from(f32::EPSILON),
                    "interior depth mismatch at {x},{y} (mirror {},{y}): {depth} versus {actual}, difference {difference} metres, device difference {device_difference}",
                    width - 1 - x,
                );
                interiors += 1;
                if difference > maximum.0 {
                    maximum = (difference, x, y);
                }
            }
        }
    }
    assert!(
        interiors > width,
        "only {interiors} geometrically interior pixels"
    );
    assert!(
        silhouettes[0].abs_diff(silhouettes[1]) <= edges,
        "silhouette counts {silhouettes:?} differ beyond {edges} geometric edge pixels"
    );
    eprintln!(
        "mirrored depth: {interiors} interiors, {edges} edges, silhouettes {silhouettes:?}, maximum interior {maximum:?}, maximum overall {maximum_edge:?}"
    );
}

pub(super) struct RasterGeometry {
    triangles: Vec<[Vec3; 3]>,
}

impl RasterGeometry {
    pub(super) fn new(scene: &RenderScene, viewport: Viewport, handedness: ViewHandedness) -> Self {
        let options = crate::tests::options();
        let view = worldrunner_render_scene::SceneView::with_convention(
            options.pose,
            options.camera,
            viewport,
            convention(handedness),
        )
        .unwrap();
        let project = |point| view.resolved_view().world_to_screen(point).unwrap();
        let mut triangles = Vec::new();
        for object in scene.geometry().objects() {
            let matrix = worldrunner_render_scene::geometry::model_matrix(object.transform());
            let lod =
                &scene.geometry().meshes()[usize::try_from(object.mesh().raw()).unwrap()].lods()[0];
            let vertices: Vec<_> = lod
                .vertices()
                .iter()
                .map(|vertex| {
                    let point = vertex.position_millimeters.map(|v| {
                        f32::from(i16::try_from(v).expect("small fixture coordinates")) / 1000.0
                    });
                    project(matrix.transform_point3(Vec3::from_array(point)))
                })
                .collect();
            for indices in lod.indices().chunks_exact(3) {
                triangles.push(std::array::from_fn(|i| {
                    vertices[usize::try_from(indices[i]).unwrap()]
                }));
            }
        }
        if let Some(terrain) = scene.terrain() {
            let mesh = worldrunner_render_scene::terrain::build_terrain_mesh(terrain).unwrap();
            let stride =
                usize::try_from(worldrunner_render_scene::geometry::VERTEX_STRIDE).unwrap();
            let vertices: Vec<_> = mesh
                .vertices
                .chunks_exact(stride)
                .map(|vertex| {
                    project(Vec3::from_array(std::array::from_fn(|i| {
                        f32::from_le_bytes(vertex[i * 4..i * 4 + 4].try_into().unwrap())
                    })))
                })
                .collect();
            for triangle in mesh.indices.chunks_exact(12) {
                triangles.push(std::array::from_fn(|i| {
                    let index = u32::from_le_bytes(triangle[i * 4..i * 4 + 4].try_into().unwrap());
                    vertices[usize::try_from(index).unwrap()]
                }));
            }
        }
        Self { triangles }
    }

    pub(super) fn sample(&self, x: u32, y: u32) -> Sample {
        let point = DVec2::new(f64::from(x) + 0.5, f64::from(y) + 0.5);
        let mut sample = Sample {
            interior: None,
            edge: false,
        };
        let mut nearest = f64::INFINITY;
        for (index, triangle) in self.triangles.iter().enumerate() {
            let vertices = triangle.map(|v| v.truncate().as_dvec2());
            let area = (vertices[1] - vertices[0]).perp_dot(vertices[2] - vertices[0]);
            let mut inside = true;
            let mut inset = f64::INFINITY;
            let mut weights = [0.0; 3];
            for i in 0..3 {
                let a = vertices[(i + 1) % 3];
                let b = vertices[(i + 2) % 3];
                let edge = b - a;
                let signed = edge.perp_dot(point - a) * area.signum() / edge.length();
                weights[i] = edge.perp_dot(point - a) / area;
                inside &= signed >= 0.0;
                inset = inset.min(signed);
                let along = ((point - a).dot(edge) / edge.length_squared()).clamp(0.0, 1.0);
                sample.edge |= point.distance(a + along * edge) < 1.0;
            }
            let depth: f64 = weights
                .iter()
                .zip(triangle)
                .map(|(w, v)| w * f64::from(v.z))
                .sum();
            if inside && (0.0..1.0).contains(&depth) && depth < nearest {
                nearest = depth;
                sample.interior = (inset >= 1.0).then_some(index);
            }
        }
        if sample.edge {
            sample.interior = None;
        }
        sample
    }
}

pub(super) struct Sample {
    pub(super) interior: Option<usize>,
    pub(super) edge: bool,
}
