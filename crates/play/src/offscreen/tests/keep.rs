//! A sampler edit between frames keeps what a camera drew into a render texture. The edit raises
//! the world texture's generation, which the frame's spec carries, and the sampler the draws
//! read comes from the texture's description in each frame's snapshot.

use worldrunner_world_simulation::graphics::OffscreenWork;
use worldrunner_world_simulation::graphics::TargetRef;
use worldrunner_world_simulation::graphics::TextureRef;

use super::gpu::Rig;
use super::gpu::SIDE;
use super::gpu::camera_render;
use super::support::render_texture;
use super::support::world;

const RED: [u8; 4] = [255, 0, 0, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];

fn texel(bytes: &[u8], (x, y): (usize, usize)) -> [u8; 4] {
    bytes[(y * SIDE as usize + x) * 4..][..4]
        .try_into()
        .unwrap()
}

#[test]
fn a_filter_change_between_frames_keeps_what_the_camera_drew() {
    let mut rig = Rig::new();
    let mut world = world();
    let (drawn, copy) = (
        render_texture(&mut world, [SIDE, SIDE]),
        render_texture(&mut world, [SIDE, SIDE]),
    );
    camera_render(&mut world, drawn);
    let (first, operations, reads) = rig.frame(&mut world, &[drawn]);
    let before = first
        .frame
        .render_textures
        .iter()
        .map(|spec| spec.generation)
        .max();
    rig.run(operations);
    let camera = reads.into_iter().next().unwrap().bytes(&rig.device);
    assert_eq!(texel(&camera, (0, 0)), RED);
    assert_eq!(texel(&camera, (3, 3)), BLUE);

    // What Texture.filterMode does on the managed side: the description is written again with the
    // new sampler, so its generation rises and its storage stays as it was.
    let mut description = *world.render_texture(drawn).unwrap();
    description.sampler.filter = 0;
    world.update_render_texture(drawn, description).unwrap();
    world
        .record_offscreen(OffscreenWork::CopyTexture {
            source: TextureRef::Render(drawn),
            source_element: 0,
            source_mip: 0,
            target: TargetRef::Render(copy),
            target_element: 0,
            target_mip: 0,
        })
        .unwrap();
    let (second, operations, reads) = rig.frame(&mut world, &[copy]);
    let spec = second
        .frame
        .render_textures
        .iter()
        .find(|spec| spec.id == super::super::ids::render_texture(drawn))
        .unwrap();
    assert!(
        Some(spec.generation) > before,
        "the edit raised the generation"
    );
    rig.run(operations);
    let copied = reads.into_iter().next().unwrap().bytes(&rig.device);
    assert_eq!(copied, camera, "the copy reads what the camera drew");
}
