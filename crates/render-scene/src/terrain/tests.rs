//! Behavioural claims about tessellating a governed heightfield.
//!
//! Every claim here is checked without an adapter, which is the point of
//! keeping the geometry in a module that names no `wgpu` type: a mirrored,
//! transposed or seam-ridden terrain is nearly invisible in a screenshot of
//! unfamiliar ground and immediate in an assertion about where a sample landed.

use worldrunner_render_plan::{
    MaterialHandle, SceneTerrain, SceneTerrainAppearance, SceneTerrainParts, SceneTerrainSurface,
    TerrainLodPolicy,
};

use super::{TERRAIN_METERS_PER_TILE, TerrainMesh, axis_samples, build_terrain_mesh, lod_stride};

/// Bytes occupied by one packed vertex, mirroring the geometry layout.
const VERTEX_BYTES: usize = 40;

/// Bytes occupied by one packed index.
const INDEX_BYTES: usize = 4;

/// Tolerance for comparing metre positions that came through `f64`.
const EPSILON: f32 = 1e-6;

/// Reads one little-endian `f32` out of packed bytes.
fn f32_at(bytes: &[u8], offset: usize) -> f32 {
    f32::from_le_bytes(
        bytes[offset..offset + 4]
            .try_into()
            .expect("a four byte window is a f32"),
    )
}

/// Reads one little-endian `u32` out of packed bytes.
fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(
        bytes[offset..offset + 4]
            .try_into()
            .expect("a four byte window is a u32"),
    )
}

/// Returns the position of one packed vertex.
fn position(mesh: &TerrainMesh, vertex: usize) -> [f32; 3] {
    let base = vertex * VERTEX_BYTES;
    [
        f32_at(&mesh.vertices, base),
        f32_at(&mesh.vertices, base + 4),
        f32_at(&mesh.vertices, base + 8),
    ]
}

/// Returns the normal of one packed vertex.
fn normal(mesh: &TerrainMesh, vertex: usize) -> [f32; 3] {
    let base = vertex * VERTEX_BYTES + 12;
    [
        f32_at(&mesh.vertices, base),
        f32_at(&mesh.vertices, base + 4),
        f32_at(&mesh.vertices, base + 8),
    ]
}

/// Returns the texture coordinate of one packed vertex.
fn texcoord(mesh: &TerrainMesh, vertex: usize) -> [f32; 2] {
    let base = vertex * VERTEX_BYTES + 24;
    [
        f32_at(&mesh.vertices, base),
        f32_at(&mesh.vertices, base + 4),
    ]
}

/// Returns the packed indices as numbers.
fn indices(mesh: &TerrainMesh) -> Vec<u32> {
    (0..mesh.indices.len() / INDEX_BYTES)
        .map(|index| u32_at(&mesh.indices, index * INDEX_BYTES))
        .collect()
}

/// Returns how many vertices one mesh carries.
fn vertex_count(mesh: &TerrainMesh) -> usize {
    mesh.vertices.len() / VERTEX_BYTES
}

/// Reports whether two positions agree to within the metre tolerance.
fn close(left: [f32; 3], right: [f32; 3]) -> bool {
    left.iter()
        .zip(right)
        .all(|(left, right)| (left - right).abs() < EPSILON)
}

/// Builds `count` distinct neutral surfaces, one per material handle.
fn surfaces(count: u32) -> Vec<SceneTerrainSurface> {
    (0..count)
        .map(|material| {
            SceneTerrainSurface::new(
                MaterialHandle::from_raw(material),
                SceneTerrainAppearance::NEUTRAL,
            )
        })
        .collect()
}

/// Builds one terrain with an explicit grid, origin and spacing.
fn terrain(
    columns: u32,
    rows: u32,
    heights: Vec<i32>,
    indices: Vec<u8>,
    policy: TerrainLodPolicy,
) -> SceneTerrain {
    let entries = indices.iter().copied().max().unwrap_or(0) + 1;
    SceneTerrain::new(SceneTerrainParts {
        columns,
        rows,
        origin_east_millimeters: 1_000,
        origin_north_millimeters: -2_000,
        sample_spacing_millimeters: 500,
        heights_millimeters: heights,
        surface_indices: indices,
        surfaces: surfaces(u32::from(entries)),
        lod_policy: policy,
    })
    .expect("the fixture terrain is valid")
}

/// Builds a flat single-material terrain of the given dimensions.
fn flat(columns: u32, rows: u32, policy: TerrainLodPolicy) -> SceneTerrain {
    let samples = usize::try_from(columns * rows).expect("the fixture grid is small");
    terrain(columns, rows, vec![0; samples], vec![0; samples], policy)
}

#[test]
fn sample_column_and_row_indices_advance_east_and_north_from_the_origin() {
    // The height grid encodes its own coordinates, so a transposed or
    // mirrored read shows up as a vertex standing at the wrong altitude
    // rather than as terrain that merely looks unfamiliar.
    let heights = vec![0, 100, 200, 1_000, 1_100, 1_200];
    let surface = terrain(3, 2, heights, vec![0; 6], TerrainLodPolicy::FullResolution);
    let mesh = &build_terrain_mesh(&surface).expect("a small grid tessellates");
    let seen: Vec<[f32; 3]> = (0..vertex_count(mesh))
        .map(|vertex| position(mesh, vertex))
        .collect();
    assert_eq!(seen.len(), 6);
    for expected in [
        [1.0, -2.0, 0.0],
        [1.5, -2.0, 0.1],
        [2.0, -2.0, 0.2],
        [1.0, -1.5, 1.0],
        [1.5, -1.5, 1.1],
        [2.0, -1.5, 1.2],
    ] {
        assert!(
            seen.iter().any(|found| close(*found, expected)),
            "{expected:?} is missing from {seen:?}"
        );
    }
}

#[test]
fn the_surface_tessellates_identically_however_many_materials_it_names() {
    // The same four quads, once with every corner naming a different surface
    // and once with all of them naming the same one. The geometry must not
    // notice: material selection now happens per fragment, so a grid that
    // still split by material would show up here as a differing index count.
    let varied = terrain(
        3,
        3,
        vec![0; 9],
        vec![0, 1, 3, 2, 3, 3, 3, 3, 3],
        TerrainLodPolicy::FullResolution,
    );
    let uniform = terrain(
        3,
        3,
        vec![0; 9],
        vec![0; 9],
        TerrainLodPolicy::FullResolution,
    );
    let varied = build_terrain_mesh(&varied).expect("a small grid tessellates");
    let uniform = build_terrain_mesh(&uniform).expect("a small grid tessellates");
    assert_eq!(varied.index_count, 4 * 6);
    assert_eq!(varied.index_count, uniform.index_count);
    assert_eq!(indices(&varied), indices(&uniform));
    assert_eq!(vertex_count(&varied), 9);
}

#[test]
fn samples_shared_by_neighbouring_quads_are_stored_once() {
    // Two quads side by side name different surfaces and share an edge. The
    // old per-material split gave each of them its own copy of that edge,
    // which is where the duplicated vertices and the hard seam both came from.
    // One mesh over the whole grid stores six samples, not eight.
    let surface = terrain(
        3,
        2,
        vec![0; 6],
        vec![0, 1, 1, 0, 1, 1],
        TerrainLodPolicy::FullResolution,
    );
    let mesh = &build_terrain_mesh(&surface).expect("a small grid tessellates");
    assert_eq!(vertex_count(mesh), 6);
    let mut used = indices(mesh);
    used.sort_unstable();
    used.dedup();
    assert_eq!(used, (0..6).collect::<Vec<u32>>());
}

#[test]
fn quad_triangles_wind_counter_clockwise_seen_from_above() {
    // The pipelines declare a counter-clockwise front face, so terrain wound
    // the other way would be culled away entirely by the single-sided variant.
    let surface = flat(2, 2, TerrainLodPolicy::FullResolution);
    let mesh = &build_terrain_mesh(&surface).expect("a small grid tessellates");
    // Row-major vertices, so the quad's four corners are 0, 1, 3, 2 read
    // anticlockwise from the near-west sample rather than 0, 1, 2, 3.
    let emitted = indices(mesh);
    assert_eq!(emitted, vec![0, 1, 3, 0, 3, 2]);
    for triangle in emitted.as_chunks::<3>().0 {
        let [first, second, third] =
            [triangle[0], triangle[1], triangle[2]].map(|vertex| position(mesh, vertex as usize));
        let east = [second[0] - first[0], second[1] - first[1]];
        let north = [third[0] - first[0], third[1] - first[1]];
        let winding = east[0].mul_add(north[1], -(east[1] * north[0]));
        assert!(winding > 0.0, "triangle {triangle:?} winds clockwise");
    }
}

#[test]
fn a_known_grid_yields_two_triangles_for_every_quad() {
    let surface = flat(5, 4, TerrainLodPolicy::FullResolution);
    let mesh = &build_terrain_mesh(&surface).expect("a small grid tessellates");
    assert_eq!(mesh.index_count, 4 * 3 * 6);
    assert_eq!(vertex_count(mesh), 20);
}

#[test]
fn a_flat_region_shades_with_normals_pointing_straight_up() {
    let surface = flat(4, 4, TerrainLodPolicy::FullResolution);
    let mesh = &build_terrain_mesh(&surface).expect("a small grid tessellates");
    for vertex in 0..vertex_count(mesh) {
        let found = normal(mesh, vertex);
        assert!(found[0].abs() < EPSILON, "found {found:?}");
        assert!(found[1].abs() < EPSILON, "found {found:?}");
        assert!((found[2] - 1.0).abs() < EPSILON, "found {found:?}");
    }
}

#[test]
fn a_slope_tilts_its_normal_away_from_the_rising_axis() {
    // Heights rise eastward by half a metre per half-metre step, a forty-five
    // degree ramp, so the normal leans west by exactly as much as it leans up.
    let surface = terrain(
        3,
        2,
        vec![0, 500, 1_000, 0, 500, 1_000],
        vec![0; 6],
        TerrainLodPolicy::FullResolution,
    );
    let mesh = &build_terrain_mesh(&surface).expect("a small grid tessellates");
    let found = normal(mesh, 0);
    let root_half = 0.5_f32.sqrt();
    assert!((found[0] + root_half).abs() < 1e-5, "found {found:?}");
    assert!(found[1].abs() < 1e-5, "found {found:?}");
    assert!((found[2] - root_half).abs() < 1e-5, "found {found:?}");
}

#[test]
fn texture_coordinates_tile_in_world_space_rather_than_by_grid_index() {
    let surface = flat(2, 2, TerrainLodPolicy::FullResolution);
    let mesh = &build_terrain_mesh(&surface).expect("a small grid tessellates");
    for vertex in 0..vertex_count(mesh) {
        let place = position(mesh, vertex);
        let found = texcoord(mesh, vertex);
        assert!((found[0] - place[0] / TERRAIN_METERS_PER_TILE).abs() < EPSILON);
        assert!((found[1] - place[1] / TERRAIN_METERS_PER_TILE).abs() < EPSILON);
    }
}

#[test]
fn uniform_steps_skip_two_to_the_power_of_steps_samples_on_both_axes() {
    assert_eq!(lod_stride(TerrainLodPolicy::FullResolution), 1);
    assert_eq!(lod_stride(TerrainLodPolicy::UniformSteps { steps: 1 }), 2);
    assert_eq!(lod_stride(TerrainLodPolicy::UniformSteps { steps: 3 }), 8);
}

#[test]
fn a_reduced_grid_keeps_its_samples_geometrically_where_they_were() {
    // Five columns reduced by one step select columns 0, 2 and 4, so the
    // effective spacing doubles and the terrain keeps its full extent.
    let surface = flat(5, 5, TerrainLodPolicy::UniformSteps { steps: 1 });
    let mesh = &build_terrain_mesh(&surface).expect("a small grid tessellates");
    assert_eq!(vertex_count(mesh), 9);
    assert_eq!(mesh.index_count, 2 * 2 * 6);
    let easts: Vec<f32> = (0..vertex_count(mesh))
        .map(|vertex| position(mesh, vertex)[0])
        .collect();
    assert!(easts.contains(&1.0));
    assert!(easts.contains(&2.0));
    assert!(easts.contains(&3.0));
}

#[test]
fn a_stride_that_does_not_divide_the_grid_still_reaches_the_far_edge() {
    // Six samples at stride four select 0 and 4 by stepping, then the far
    // edge at 5, so the last interval is shorter rather than absent. Dropping
    // it would shrink the world by up to a whole stride of spacing.
    assert_eq!(axis_samples(6, 4), vec![0, 4, 5]);
    assert_eq!(axis_samples(5, 2), vec![0, 2, 4]);
    assert_eq!(axis_samples(2, 8), vec![0, 1]);
    let surface = flat(6, 6, TerrainLodPolicy::UniformSteps { steps: 2 });
    let mesh = &build_terrain_mesh(&surface).expect("a small grid tessellates");
    assert_eq!(vertex_count(mesh), 9);
    let easts: Vec<f32> = (0..vertex_count(mesh))
        .map(|vertex| position(mesh, vertex)[0])
        .collect();
    let far = 1.0 + 5.0 * 0.5;
    assert!(
        easts.iter().any(|east| (east - far).abs() < 1e-6),
        "the far edge at {far} is missing from {easts:?}"
    );
}

#[test]
fn reducing_the_geometry_does_not_reduce_the_material_authority() {
    // A three by three grid at stride two keeps four of its nine samples as
    // vertices. The surface indices are untouched by that: the fragment shader
    // reads the grid at full resolution, so the one cell here that names a
    // different surface still reaches the screen even though no vertex of the
    // reduced mesh stands on it. Decimating the grid alongside the geometry is
    // precisely the mistake this renderer used to make.
    let surface = terrain(
        3,
        3,
        vec![0; 9],
        vec![1, 0, 1, 1, 1, 1, 1, 1, 1],
        TerrainLodPolicy::UniformSteps { steps: 1 },
    );
    let mesh = &build_terrain_mesh(&surface).expect("a small grid tessellates");
    assert_eq!(vertex_count(mesh), 4);
    assert_eq!(mesh.index_count, 6);
    assert_eq!(surface.surface_indices().len(), 9);
    assert_eq!(surface.surface_indices()[1], 0);
}
