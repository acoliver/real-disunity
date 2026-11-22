//! A blit through a pass whose vertex stage places the quad by `unity_ObjectToWorld` and
//! `unity_MatrixVP`, drawn on a real adapter (lavapipe in CI). `Graphics.Blit` sets those
//! itself, so the host supplies them; with zero matrices the quad would collapse and the target
//! would stay empty. Every expected texel comes from what the camera drew into the source.

use worldrunner_world_simulation::graphics::{OffscreenWork, TargetRef, TextureRef};

use super::gpu::{Rig, SIDE, camera_render, shader, vector};
use super::support::{render_texture, world};

const RED: [u8; 4] = [255, 0, 0, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];

fn pixel(bytes: &[u8], (x, y): (usize, usize)) -> [u8; 4] {
    bytes[(y * SIDE as usize + x) * 4..][..4]
        .try_into()
        .unwrap()
}

#[test]
fn a_blit_pass_that_places_its_quad_by_the_builtin_matrices_covers_the_target() {
    let mut rig = Rig::new();
    let mut world = world();
    let (source, target) = (
        render_texture(&mut world, [SIDE, SIDE]),
        render_texture(&mut world, [SIDE, SIDE]),
    );
    // The camera draws red over the left half of the source and clears the rest to blue.
    camera_render(&mut world, source);
    let placed = shader(&mut world, "placed_blit");
    let material = world.create_material(placed).unwrap();
    vector(&mut world, material, "_Tint", [1.0; 4]);
    world
        .record_offscreen(OffscreenWork::Blit {
            source: TextureRef::Render(source),
            target: TargetRef::Render(target),
            material: Some(material),
            pass: 0,
        })
        .unwrap();
    let (adapted, operations, reads) = rig.frame(&mut world, &[target]);
    assert!(
        adapted.report.failures.is_empty(),
        "{:?}",
        adapted.report.failures
    );
    rig.run(operations);
    let drawn = reads.into_iter().next().unwrap().bytes(&rig.device);
    for y in 0..SIDE as usize {
        assert_eq!(pixel(&drawn, (0, y)), RED, "left ({y})");
        assert_eq!(pixel(&drawn, (SIDE as usize - 1, y)), BLUE, "right ({y})");
    }
}
