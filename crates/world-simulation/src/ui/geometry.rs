//! The mesh a canvas renderer submits: vertices with a position in the element's local space,
//! a first texture coordinate and a colour, plus the triangle indices of the first sub-mesh.
//! A graphic is drawn as the quads found among its triangles.
use crate::UiError;

/// The most vertices one renderer's mesh holds.
pub const MAX_UI_VERTICES: usize = 1 << 20;

/// The most indices one renderer's mesh holds.
pub const MAX_UI_INDICES: usize = 3 << 20;

/// How far two texture coordinates may differ and still count as the same edge of a quad.
const UV_TOLERANCE: f32 = 1e-5;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiVertex {
    pub position: [f32; 3],
    pub uv: [f32; 2],
    pub color: [f32; 4],
}

/// One quad found in a mesh.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GeometryQuad {
    /// Local positions in the order a sprite lists its corners. The first is the vertex whose
    /// texture coordinate is the rectangle's `u` minimum and `v` minimum (lower left in the
    /// image), then the lower right, upper right and upper left, taking the sides the other way
    /// round when the coordinates run backwards.
    pub positions: [[f32; 3]; 4],
    /// The presentation's `uv_rect`: the `u` of the first corner, the image row (counted from
    /// the top) of the third corner, the `u` of the third corner and the image row of the first.
    pub uv_rect: [f32; 4],
    /// The colour of the quad's first vertex.
    pub color: [f32; 4],
}

/// The quads of a mesh, and how many triangles belong to none.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GeometryQuads {
    pub quads: Vec<GeometryQuad>,
    pub left_out_triangles: usize,
}

/// A validated mesh: every index names a vertex, every number is finite, and the index count is
/// a whole number of triangles.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UiGeometry {
    vertices: Vec<UiVertex>,
    indices: Vec<u32>,
}

impl UiGeometry {
    /// Checks the mesh. A mesh above [`MAX_UI_VERTICES`] or [`MAX_UI_INDICES`], an index count
    /// that is not a multiple of three, an index past the vertices, or a non-finite position or
    /// texture coordinate or a NaN colour is rejected.
    pub fn new(vertices: Vec<UiVertex>, indices: Vec<u32>) -> Result<Self, UiError> {
        let finite = |vertex: &UiVertex| {
            vertex
                .position
                .iter()
                .chain(&vertex.uv)
                .all(|value| value.is_finite())
                && vertex.color.iter().all(|value| !value.is_nan())
        };
        if vertices.len() > MAX_UI_VERTICES
            || indices.len() > MAX_UI_INDICES
            || !indices.len().is_multiple_of(3)
            || indices
                .iter()
                .any(|&index| index as usize >= vertices.len())
            || !vertices.iter().all(finite)
        {
            return Err(UiError::InvalidCanvasRenderer);
        }
        Ok(Self { vertices, indices })
    }

    #[must_use]
    pub fn vertices(&self) -> &[UiVertex] {
        &self.vertices
    }

    #[must_use]
    pub fn indices(&self) -> &[u32] {
        &self.indices
    }

    /// Finds the quads. A quad is two consecutive triangles that share two vertices and whose
    /// four distinct vertices carry the texture coordinates of an upright or flipped rectangle,
    /// as the usual UI quad (triangles 0 1 2 and 2 3 0) does, in either winding. The triangles
    /// of a mesh that do not form such pairs are counted in `left_out_triangles`.
    #[must_use]
    pub fn quads(&self) -> GeometryQuads {
        let mut found = GeometryQuads::default();
        let triangles: Vec<[u32; 3]> = self
            .indices
            .chunks_exact(3)
            .map(|triangle| [triangle[0], triangle[1], triangle[2]])
            .collect();
        let mut next = 0;
        while next < triangles.len() {
            let quad = triangles
                .get(next + 1)
                .and_then(|second| self.quad_of(triangles[next], *second));
            if let Some(quad) = quad {
                found.quads.push(quad);
                next += 2;
            } else {
                found.left_out_triangles += 1;
                next += 1;
            }
        }
        found
    }

    fn quad_of(&self, first: [u32; 3], second: [u32; 3]) -> Option<GeometryQuad> {
        let cycle = quad_cycle(first, second)?;
        let [v0, v1, v2, v3] = cycle.map(|index| self.vertices[index as usize]);
        let same = |a: f32, b: f32| (a - b).abs() <= UV_TOLERANCE;
        let corners = if same(v1.uv[1], v0.uv[1])
            && same(v1.uv[0], v2.uv[0])
            && same(v3.uv[0], v0.uv[0])
            && same(v3.uv[1], v2.uv[1])
        {
            [v0, v1, v2, v3]
        } else if same(v3.uv[1], v0.uv[1])
            && same(v3.uv[0], v2.uv[0])
            && same(v1.uv[0], v0.uv[0])
            && same(v1.uv[1], v2.uv[1])
        {
            [v0, v3, v2, v1]
        } else {
            return None;
        };
        Some(GeometryQuad {
            positions: corners.map(|vertex| vertex.position),
            uv_rect: [
                corners[0].uv[0],
                1. - corners[2].uv[1],
                corners[2].uv[0],
                1. - corners[0].uv[1],
            ],
            color: corners[0].color,
        })
    }
}

/// The four vertices of the quad two triangles make, in order around it, or `None` when the
/// triangles repeat a vertex or do not share exactly two.
fn quad_cycle(first: [u32; 3], second: [u32; 3]) -> Option<[u32; 4]> {
    let distinct = |t: [u32; 3]| t[0] != t[1] && t[1] != t[2] && t[0] != t[2];
    if !distinct(first) || !distinct(second) {
        return None;
    }
    let shared = first.iter().filter(|v| second.contains(v)).count();
    if shared != 2 {
        return None;
    }
    let apex = first.iter().position(|v| !second.contains(v))?;
    let far = *second.iter().find(|v| !first.contains(v))?;
    let mut around = [
        first[apex],
        first[(apex + 1) % 3],
        far,
        first[(apex + 2) % 3],
    ];
    let start = around.iter().position(|v| *v == first[0])?;
    around.rotate_left(start);
    Some(around)
}
