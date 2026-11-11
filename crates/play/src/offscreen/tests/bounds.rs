//! The extent of a mesh and what planes and spheres do to it.

use glam::{Mat4, Vec3};
use worldrunner_render_program::unity::offscreen::frame::{
    ChannelFormat, Mesh, MeshChannel, MeshIndices, SubMesh,
};

use super::super::assets::{POSITION, TEXTURE_COORDINATES};
use super::super::bounds::{Bounds, enclosing_sphere, outside_planes, touches_sphere};

fn floats(values: &[f32]) -> Vec<u8> {
    values
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect()
}

fn mesh_of(positions: &[f32]) -> Mesh {
    Mesh {
        vertex_count: u32::try_from(positions.len() / 3).unwrap(),
        channels: vec![MeshChannel {
            source: POSITION,
            format: ChannelFormat::Float32x3,
            bytes: floats(positions),
        }],
        indices: MeshIndices::None,
        sub_meshes: vec![SubMesh {
            first: 0,
            count: 3,
            base_vertex: 0,
        }],
    }
}

#[test]
fn the_bounds_of_a_mesh_are_the_extremes_of_its_positions() {
    let mesh = mesh_of(&[1.0, -2.0, 3.0, -4.0, 5.0, 0.5, 0.0, 0.0, -6.0]);
    let bounds = Bounds::of_mesh(&mesh).unwrap();
    assert_eq!(
        bounds.min.map(f32::to_bits),
        [-4.0_f32, -2.0, -6.0].map(f32::to_bits)
    );
    assert_eq!(
        bounds.max.map(f32::to_bits),
        [1.0_f32, 5.0, 3.0].map(f32::to_bits)
    );
}

#[test]
fn a_mesh_with_no_positions_of_three_floats_has_no_bounds() {
    let mut mesh = mesh_of(&[0.0; 9]);
    mesh.channels[0].source = TEXTURE_COORDINATES;
    assert_eq!(Bounds::of_mesh(&mesh), None);
    let mut flat = mesh_of(&[0.0; 9]);
    flat.channels[0].format = ChannelFormat::Float32x2;
    assert_eq!(Bounds::of_mesh(&flat), None);
    let mut empty = mesh_of(&[]);
    empty.vertex_count = 0;
    assert_eq!(Bounds::of_mesh(&empty), None);
}

#[test]
fn the_corners_of_the_bounds_follow_the_matrix() {
    let bounds = Bounds {
        min: [-1.0, -2.0, -3.0],
        max: [1.0, 2.0, 3.0],
    };
    let matrix = Mat4::from_translation(Vec3::new(10.0, 0.0, 0.0));
    let corners = bounds.corners(&matrix.to_cols_array());
    let xs: Vec<f32> = corners.iter().map(|corner| corner.x).collect();
    assert_eq!(xs, [9.0, 11.0, 9.0, 11.0, 9.0, 11.0, 9.0, 11.0]);
    let (centre, radius) = enclosing_sphere(&corners);
    assert_eq!(centre, Vec3::new(10.0, 0.0, 0.0));
    assert_eq!(radius.to_bits(), 14.0_f32.sqrt().to_bits());
}

#[test]
fn a_body_is_outside_a_plane_only_when_every_point_is_behind_it() {
    let points = [Vec3::new(4.0, 0.0, 0.0), Vec3::new(6.0, 0.0, 0.0)];
    // Inside where x - 5 is not negative: the point at 6 is inside, so the body is not outside.
    assert!(!outside_planes(&[[1.0, 0.0, 0.0, -5.0]], &points));
    // Inside where x - 7 is not negative: both are behind.
    assert!(outside_planes(&[[1.0, 0.0, 0.0, -7.0]], &points));
    // One plane that holds the body and one that does not: outside.
    assert!(outside_planes(
        &[[1.0, 0.0, 0.0, -5.0], [1.0, 0.0, 0.0, -7.0]],
        &points
    ));
    assert!(!outside_planes(&[], &points));
}

#[test]
fn a_sphere_touches_another_within_the_sum_of_the_radii_and_no_radius_is_no_limit() {
    let centre = Vec3::new(10.0, 0.0, 0.0);
    assert!(touches_sphere([0.0, 0.0, 0.0, 8.0], centre, 2.0));
    assert!(!touches_sphere([0.0, 0.0, 0.0, 7.5], centre, 2.0));
    assert!(touches_sphere([0.0; 4], centre, 0.5));
}
