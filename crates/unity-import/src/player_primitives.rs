//! The meshes of the built-in primitives, which a player's objects reference by number in the
//! default resources file the engine does not read.
//!
//! The geometry is generated here, in the engine's left-handed space: a triangle runs clockwise
//! seen from the side its normals point to. A quad is one unit square facing back along z, a
//! plane is ten units across in ten cells facing up, a cube is one unit with its own four
//! vertices per face, and a sphere has a radius of one half. The cell counts of the plane and
//! the sphere, and the texture coordinates, are the engine's choice.

use std::f32::consts::PI;

use crate::{
    player_meshes::{DecodedMesh, PlanBounds, PlanMesh, PlanMeshChannel, PlanSubMesh},
    player_resources::ObjectKey,
};

/// The file name of the default resources, as an external reference names it.
const DEFAULT_RESOURCES: &str = "unity default resources";

const CUBE: i64 = 10202;
const SPHERE: i64 = 10207;
const PLANE: i64 = 10209;
const QUAD: i64 = 10210;

const PLANE_CELLS: u32 = 10;
const SPHERE_SEGMENTS: u32 = 24;
const SPHERE_RINGS: u32 = 16;

type Vec3 = [f32; 3];

/// Vertex attributes and triangle indices, before they are written as a resource.
#[derive(Default)]
struct Geometry {
    positions: Vec<Vec3>,
    normals: Vec<Vec3>,
    uvs: Vec<[f32; 2]>,
    indices: Vec<u32>,
}

/// The mesh the default resources hold as `key`, when the engine generates it.
pub(crate) fn primitive_mesh(key: &ObjectKey) -> Option<DecodedMesh> {
    let (path, path_id) = key;
    if path.file_name().and_then(|name| name.to_str()) != Some(DEFAULT_RESOURCES) {
        return None;
    }
    let (name, geometry) = match *path_id {
        QUAD => ("Quad", quad()),
        PLANE => ("Plane", plane()),
        CUBE => ("Cube", cube()),
        SPHERE => ("Sphere", sphere()),
        _ => return None,
    };
    Some(geometry.into_mesh(name))
}

fn quad() -> Geometry {
    let mut geometry = Geometry::default();
    face(
        &mut geometry,
        [0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, 0.0, -1.0],
    );
    geometry
}

fn plane() -> Geometry {
    let mut geometry = Geometry::default();
    let side = PLANE_CELLS + 1;
    for z in 0..side {
        for x in 0..side {
            let (u, v) = (
                to_f32(x) / to_f32(PLANE_CELLS),
                to_f32(z) / to_f32(PLANE_CELLS),
            );
            geometry
                .positions
                .push([(u - 0.5) * 10.0, 0.0, (v - 0.5) * 10.0]);
            geometry.normals.push([0.0, 1.0, 0.0]);
            geometry.uvs.push([u, v]);
        }
    }
    for z in 0..PLANE_CELLS {
        for x in 0..PLANE_CELLS {
            let bottom_left = z * side + x;
            let top_left = bottom_left + side;
            geometry
                .indices
                .extend([bottom_left, top_left, bottom_left + 1]);
            geometry
                .indices
                .extend([bottom_left + 1, top_left, top_left + 1]);
        }
    }
    geometry
}

fn cube() -> Geometry {
    let mut geometry = Geometry::default();
    // Each face is described by its right and up axes; right x up points into the cube.
    let faces: [(Vec3, Vec3, Vec3); 6] = [
        ([1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, -1.0]),
        ([-1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]),
        ([0.0, 0.0, 1.0], [0.0, 1.0, 0.0], [1.0, 0.0, 0.0]),
        ([0.0, 0.0, -1.0], [0.0, 1.0, 0.0], [-1.0, 0.0, 0.0]),
        ([1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 1.0, 0.0]),
        ([1.0, 0.0, 0.0], [0.0, 0.0, -1.0], [0.0, -1.0, 0.0]),
    ];
    for (right, up, normal) in faces {
        let centre = normal.map(|c| c * 0.5);
        face(&mut geometry, centre, right, up, normal);
    }
    geometry
}

/// A unit square centred on `centre`, spanned by `right` and `up`, as four vertices and two
/// triangles that run clockwise seen from `normal`.
fn face(geometry: &mut Geometry, centre: Vec3, right: Vec3, up: Vec3, normal: Vec3) {
    let first = u32::try_from(geometry.positions.len()).expect("few vertices");
    for (u, v) in [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)] {
        let (a, b) = (u - 0.5, v - 0.5);
        geometry.positions.push([
            centre[0] + right[0] * a + up[0] * b,
            centre[1] + right[1] * a + up[1] * b,
            centre[2] + right[2] * a + up[2] * b,
        ]);
        geometry.normals.push(normal);
        geometry.uvs.push([u, v]);
    }
    geometry
        .indices
        .extend([first, first + 3, first + 1, first + 3, first, first + 2]);
}

fn sphere() -> Geometry {
    let mut geometry = Geometry::default();
    for ring in 0..=SPHERE_RINGS {
        let polar = PI * to_f32(ring) / to_f32(SPHERE_RINGS);
        for segment in 0..=SPHERE_SEGMENTS {
            let turn = 2.0 * PI * to_f32(segment) / to_f32(SPHERE_SEGMENTS);
            let direction = [
                polar.sin() * turn.cos(),
                polar.cos(),
                polar.sin() * turn.sin(),
            ];
            geometry.positions.push(direction.map(|c| c * 0.5));
            geometry.normals.push(direction);
            geometry.uvs.push([
                to_f32(segment) / to_f32(SPHERE_SEGMENTS),
                1.0 - to_f32(ring) / to_f32(SPHERE_RINGS),
            ]);
        }
    }
    let row = SPHERE_SEGMENTS + 1;
    for ring in 0..SPHERE_RINGS {
        for segment in 0..SPHERE_SEGMENTS {
            let upper = ring * row + segment;
            let lower = upper + row;
            // The first ring meets at the top and the last at the bottom, where one of the two
            // triangles of a cell would have no area.
            if ring > 0 {
                geometry.indices.extend([upper, upper + 1, lower]);
            }
            if ring + 1 < SPHERE_RINGS {
                geometry.indices.extend([upper + 1, lower + 1, lower]);
            }
        }
    }
    geometry
}

fn to_f32(count: u32) -> f32 {
    f32::from(u16::try_from(count).expect("small counts"))
}

impl Geometry {
    fn into_mesh(self, name: &str) -> DecodedMesh {
        let (mut low, mut high) = ([f32::MAX; 3], [f32::MIN; 3]);
        for position in &self.positions {
            for axis in 0..3 {
                low[axis] = low[axis].min(position[axis]);
                high[axis] = high[axis].max(position[axis]);
            }
        }
        let vertex_count = u32::try_from(self.positions.len()).expect("few vertices");
        let index_count = u32::try_from(self.indices.len()).expect("few indices");
        let floats = self
            .positions
            .iter()
            .flatten()
            .chain(self.normals.iter().flatten())
            .chain(self.uvs.iter().flatten());
        let mut bytes: Vec<u8> = floats.flat_map(|value| value.to_le_bytes()).collect();
        bytes.extend(self.indices.iter().flat_map(|index| index.to_le_bytes()));
        let channel = |name: &str, dimension| PlanMeshChannel {
            name: name.to_owned(),
            dimension,
        };
        DecodedMesh {
            plan: PlanMesh {
                name: name.to_owned(),
                vertex_count,
                channels: vec![
                    channel("position", 3),
                    channel("normal", 3),
                    channel("uv0", 2),
                ],
                sub_meshes: vec![PlanSubMesh {
                    first_index: 0,
                    index_count,
                    topology: 0,
                    base_vertex: 0,
                    first_vertex: 0,
                    vertex_count,
                }],
                index_count,
                index_format: 1,
                bounds: PlanBounds {
                    center: [0, 1, 2].map(|axis| f32::midpoint(low[axis], high[axis])),
                    extent: [0, 1, 2].map(|axis| (high[axis] - low[axis]) / 2.0),
                },
                resource: String::new(),
            },
            bytes,
            notes: Vec::new(),
        }
    }
}
