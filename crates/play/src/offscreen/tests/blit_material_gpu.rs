//! Two blits of one material in one flush, with the material's texture changed between them, on
//! a real adapter (lavapipe in CI). Each blit samples the texture the material held when the
//! script recorded it; a host that reads the material once, at the end of the frame, would
//! draw both blits from the last texture. Every expected texel is the colour the script cleared
//! the sampled texture to.

use worldrunner_world_simulation::World;
use worldrunner_world_simulation::graphics::{
    MaterialId, OffscreenWork, PropertyValue, RenderTextureId, TargetRef, TextureRef,
};

use super::gpu::{Rig, SIDE, shader, vector};
use super::support::{render_texture, set, texture_slot, world};

const RED: [u8; 4] = [255, 0, 0, 255];
const GREEN: [u8; 4] = [0, 255, 0, 255];

/// Clears `texture` to `value`.
pub(super) fn fill(world: &mut World, texture: RenderTextureId, value: [f32; 4]) {
    for work in [
        OffscreenWork::SetRenderTarget {
            color: vec![TargetRef::Render(texture)],
            depth: None,
            mip: 0,
            face: -1,
            slice: -1,
        },
        OffscreenWork::ClearRenderTarget {
            color: true,
            depth: false,
            stencil: false,
            value,
            depth_value: 1.0,
            stencil_value: 0,
        },
    ] {
        world.record_offscreen(work).unwrap();
    }
}

fn blit(world: &mut World, source: RenderTextureId, target: RenderTextureId, material: MaterialId) {
    world
        .record_offscreen(OffscreenWork::Blit {
            source: TextureRef::Render(source),
            target: TargetRef::Render(target),
            material: Some(material),
            pass: 0,
        })
        .unwrap();
}

fn every_texel(bytes: &[u8]) -> Vec<[u8; 4]> {
    bytes
        .chunks_exact(4)
        .map(|texel| texel.try_into().unwrap())
        .collect()
}

#[test]
fn two_blits_of_one_material_sample_the_texture_it_held_when_each_was_recorded() {
    let mut rig = Rig::new();
    let mut world = world();
    let side = [SIDE, SIDE];
    let (red, green) = (
        render_texture(&mut world, side),
        render_texture(&mut world, side),
    );
    let (first, second) = (
        render_texture(&mut world, side),
        render_texture(&mut world, side),
    );
    fill(&mut world, red, [1.0, 0.0, 0.0, 1.0]);
    fill(&mut world, green, [0.0, 1.0, 0.0, 1.0]);
    let primary = shader(&mut world, "placed_primary");
    let material = world.create_material(primary).unwrap();
    vector(&mut world, material, "_Tint", [1.0; 4]);
    for (sampled, target) in [(red, first), (green, second)] {
        set(
            &mut world,
            material,
            PropertyValue::Texture(
                "_PrimaryTex".into(),
                texture_slot(TextureRef::Render(sampled)),
            ),
        );
        blit(&mut world, sampled, target, material);
    }
    let (adapted, operations, reads) = rig.frame(&mut world, &[first, second]);
    assert!(
        adapted.report.failures.is_empty(),
        "{:?}",
        adapted.report.failures
    );
    rig.run(operations);
    let drawn: Vec<_> = reads
        .into_iter()
        .map(|read| every_texel(&read.bytes(&rig.device)))
        .collect();
    let texels = (SIDE * SIDE) as usize;
    assert_eq!(drawn[0], vec![RED; texels], "the blit recorded with red");
    assert_eq!(
        drawn[1],
        vec![GREEN; texels],
        "the blit recorded with green"
    );
}
