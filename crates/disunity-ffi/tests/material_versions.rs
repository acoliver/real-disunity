//! A blit's material through the ABI, as a script drives it: set a texture property, blit, set the
//! property to another texture, blit. The snapshot holds the material each blit saw, which a host
//! reads when it maps the blit.
#[path = "common/frame.rs"]
mod frame;

use frame::snapshot;
use worldrunner_disunity_ffi::{
    abi::*, graphics::*, graphics_offscreen::*, graphics_properties::*,
};
use worldrunner_world_simulation::graphics::{
    MaterialId, OffscreenWork, RenderTextureId, TextureRef,
};

const SCOPE_MATERIAL: u32 = 0;
const TEXTURE_RENDER: u32 = 2;

fn new_world() -> u64 {
    let mut world = 0;
    unsafe { assert_eq!(disunity_world_create(&mut world).code, 0) };
    world
}

fn render_texture(world: u64) -> u64 {
    let desc = DisunityRenderTexture {
        width: 8,
        height: 8,
        format: 8,
        mip_count: 1,
        filter: 1,
        aniso_level: 1,
        dimension: 2,
        volume_depth: 1,
        ..Default::default()
    };
    let mut id = 0;
    unsafe {
        assert_eq!(
            disunity_render_texture_create(world, &desc, &mut id).code,
            0
        )
    };
    id
}

fn material(world: u64) -> u64 {
    let (mut shader, mut material) = (0, 0);
    unsafe {
        assert_eq!(
            disunity_shader_create(world, b"Post".as_ptr(), 4, &mut shader).code,
            0
        );
        assert_eq!(
            disunity_material_create(world, shader, &mut material).code,
            0
        );
    }
    material
}

fn set_texture(world: u64, material: u64, texture: u64) {
    let slot = DisunityTextureSlot {
        kind: TEXTURE_RENDER,
        texture,
        scale_x: 1.0,
        scale_y: 1.0,
        ..Default::default()
    };
    let name = b"_PrimaryTex";
    unsafe {
        let result = disunity_shader_texture_write(
            world,
            SCOPE_MATERIAL,
            material,
            name.as_ptr(),
            name.len() as u32,
            &slot,
        );
        assert_eq!(result.code, 0);
    }
}

fn blit(world: u64, source: u64, target: u64, material: u64) {
    let record = DisunityOffscreenWork {
        kind: 1,
        source_kind: TEXTURE_RENDER,
        source,
        target,
        material,
        pass: 0,
        ..Default::default()
    };
    unsafe { assert_eq!(disunity_offscreen_record(world, &record).code, 0) };
}

#[test]
fn each_blit_of_one_material_reads_the_texture_set_before_it_through_the_abi() {
    let world = new_world();
    let (first, second) = (render_texture(world), render_texture(world));
    let (target_a, target_b) = (render_texture(world), render_texture(world));
    let material_bits = material(world);
    set_texture(world, material_bits, first);
    blit(world, first, target_a, material_bits);
    set_texture(world, material_bits, second);
    blit(world, second, target_b, material_bits);
    let snapshot = snapshot(world);
    let seen: Vec<_> = snapshot
        .work
        .iter()
        .map(|record| match record {
            OffscreenWork::Blit {
                material: Some(material),
                ..
            } => {
                snapshot.materials[material]
                    .properties
                    .texture("_PrimaryTex")
                    .unwrap()
                    .texture
            }
            other => panic!("unexpected {other:?}"),
        })
        .collect();
    assert_eq!(
        seen,
        [
            TextureRef::Render(RenderTextureId::from_bits(first)),
            TextureRef::Render(RenderTextureId::from_bits(second))
        ]
    );
    assert_ne!(
        MaterialId::from_bits(material_bits),
        MaterialId::from_bits(0)
    );
}

/// The id a blit record names, which is a material of the snapshot.
fn blit_material(record: &OffscreenWork) -> MaterialId {
    match record {
        OffscreenWork::Blit {
            material: Some(material),
            ..
        } => *material,
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn a_blit_before_an_edit_names_a_copy_of_the_state_it_saw_and_the_snapshot_holds_the_copy() {
    let world = new_world();
    let (first, second) = (render_texture(world), render_texture(world));
    let target = render_texture(world);
    let live = MaterialId::from_bits(material(world));
    set_texture(world, live.to_bits(), first);
    blit(world, first, target, live.to_bits());
    set_texture(world, live.to_bits(), second);
    blit(world, second, target, live.to_bits());
    let snapshot = snapshot(world);
    let (before_edit, after_edit) = (
        blit_material(&snapshot.work[0]),
        blit_material(&snapshot.work[1]),
    );
    assert_eq!(
        before_edit.to_bits() >> 32,
        0,
        "the edit froze the first blit's state into a copy of generation 0"
    );
    assert_ne!(before_edit, live);
    assert_eq!(
        after_edit, live,
        "the blit after the last edit names the live material"
    );
    let texture = |id: MaterialId| snapshot.materials[&id].properties.texture("_PrimaryTex");
    assert_eq!(
        texture(before_edit).unwrap().texture,
        TextureRef::Render(RenderTextureId::from_bits(first))
    );
    assert_eq!(
        texture(live).unwrap().texture,
        TextureRef::Render(RenderTextureId::from_bits(second))
    );
}
