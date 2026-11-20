//! A temporary render texture drawn into, read by a blit and copied out, on a real adapter
//! (lavapipe in CI), with predicted pixels. The scene is `gpu.rs`'s: a camera render of a red
//! quad over the left half of a blue clear, and the synthetic blit shader whose third pass
//! inverts the source.

use worldrunner_world_simulation::World;
use worldrunner_world_simulation::graphics::{
    OffscreenWork, RenderTextureId, TargetRef, TextureRef, WorldRenderTexture, WorldSampler,
};

use super::gpu::{Rig, SIDE, camera_render, shader, vector};
use super::support::{render_texture, world};

const NAME_ID: i32 = 41;
const RED: [u8; 4] = [255, 0, 0, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];
const CYAN: [u8; 4] = [0, 255, 255, 255];
const YELLOW: [u8; 4] = [255, 255, 0, 255];
/// The blit shader's pass that writes one minus the source.
const INVERT: i32 = 2;

fn allocate(world: &mut World) {
    world
        .record_offscreen(OffscreenWork::AllocateTemporary {
            name_id: NAME_ID,
            desc: WorldRenderTexture {
                width: SIDE,
                height: SIDE,
                format: 8,
                depth_bits: 0,
                mip_count: 1,
                srgb: false,
                dimension: 2,
                volume_depth: 1,
                auto_generate_mips: false,
                sampler: WorldSampler::default(),
                generation: 0,
            },
        })
        .unwrap();
}

/// The camera's image into the temporary, the temporary inverted into `inverted` and copied into
/// `copied`.
fn through_a_temporary(world: &mut World, (inverted, copied): (RenderTextureId, RenderTextureId)) {
    let camera_image = render_texture(world, [SIDE, SIDE]);
    camera_render(world, camera_image);
    let blit = shader(world, "blit");
    let material = world.create_material(blit).unwrap();
    vector(world, material, "_Tint", [1.0; 4]);
    allocate(world);
    let temporary = TargetRef::Temporary(NAME_ID);
    let work = [
        OffscreenWork::Blit {
            source: TextureRef::Render(camera_image),
            target: temporary,
            material: None,
            pass: -1,
        },
        OffscreenWork::Blit {
            source: TextureRef::Temporary(NAME_ID),
            target: TargetRef::Render(inverted),
            material: Some(material),
            pass: INVERT,
        },
        OffscreenWork::CopyTexture {
            source: TextureRef::Temporary(NAME_ID),
            source_element: 0,
            source_mip: 0,
            target: TargetRef::Render(copied),
            target_element: 0,
            target_mip: 0,
        },
        OffscreenWork::ReleaseTemporary { name_id: NAME_ID },
    ];
    for record in work {
        world.record_offscreen(record).unwrap();
    }
}

/// Whether every pixel of a `SIDE` square image is `left` in columns 0 and 1 and `right` in 2 and 3.
fn halves(bytes: &[u8], (left, right): ([u8; 4], [u8; 4])) {
    for y in 0..SIDE as usize {
        for x in 0..SIDE as usize {
            let expected = if x < 2 { left } else { right };
            let at = (y * SIDE as usize + x) * 4;
            assert_eq!(bytes[at..at + 4], expected, "pixel ({x}, {y})");
        }
    }
}

#[test]
fn a_temporary_drawn_into_is_read_by_a_blit_and_a_copy() {
    let mut rig = Rig::new();
    let mut world = world();
    let inverted = render_texture(&mut world, [SIDE, SIDE]);
    let copied = render_texture(&mut world, [SIDE, SIDE]);
    through_a_temporary(&mut world, (inverted, copied));
    let (adapted, operations, reads) = rig.frame(&mut world, &[inverted, copied]);
    assert!(
        adapted.report.failures.is_empty(),
        "{:?}",
        adapted.report.failures
    );
    assert!(
        adapted.report.skipped.is_empty(),
        "{:?}",
        adapted.report.skipped
    );
    rig.run(operations);
    let mut reads = reads.into_iter();
    // The camera image is red in columns 0 and 1 and blue in 2 and 3; inverted, cyan and yellow.
    halves(&reads.next().unwrap().bytes(&rig.device), (CYAN, YELLOW));
    halves(&reads.next().unwrap().bytes(&rig.device), (RED, BLUE));
}
