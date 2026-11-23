//! A blit's material through the ABI, as a script drives it: set a texture property, blit, set the
//! property to another texture, blit. The snapshot holds the material each blit saw, which a host
//! reads when it maps the blit.
use worldrunner_disunity_ffi::{
    abi::*, graphics::*, graphics_offscreen::*, graphics_properties::*, graphics_snapshot::*,
};
use worldrunner_world_simulation::graphics::{
    GraphicsSnapshot, MaterialId, OffscreenWork, RenderTextureId, TextureRef, snapshot_wire,
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

fn snapshot(world: u64) -> GraphicsSnapshot {
    let mut length = 0;
    unsafe {
        assert_eq!(
            disunity_graphics_snapshot(world, std::ptr::null_mut(), 0, &mut length).code,
            0
        )
    };
    let mut bytes = vec![0_u8; length as usize];
    unsafe {
        assert_eq!(
            disunity_graphics_snapshot(world, bytes.as_mut_ptr(), length, &mut length).code,
            0
        )
    };
    snapshot_wire::decode(&bytes).unwrap()
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

#[test]
fn a_take_record_that_names_a_material_copy_resolves_to_the_state_the_blit_saw() {
    let world = new_world();
    let (first, second) = (render_texture(world), render_texture(world));
    let target = render_texture(world);
    let material_bits = material(world);
    set_texture(world, material_bits, first);
    blit(world, first, target, material_bits);
    set_texture(world, material_bits, second);

    let mut records = [DisunityOffscreenWork::default(); 1];
    let mut count = 0;
    unsafe {
        assert_eq!(
            disunity_offscreen_take(world, records.as_mut_ptr(), 1, &mut count).code,
            0
        )
    };
    assert_eq!(count, 1);
    let copy = records[0].material;
    assert_ne!(
        copy, material_bits,
        "the edit froze the blit's state into a copy"
    );

    let name = b"_PrimaryTex";
    let mut read = DisunityPropertyRead::default();
    let result = unsafe {
        disunity_shader_property_read(
            world,
            SCOPE_MATERIAL,
            copy,
            name.as_ptr(),
            name.len() as u32,
            &mut read,
        )
    };
    assert_eq!(result.code, 0, "the copy named by the record can be read");
    assert_eq!(read.texture, first);
}
