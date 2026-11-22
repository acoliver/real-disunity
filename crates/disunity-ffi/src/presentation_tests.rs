use super::*;
use worldrunner_world_simulation::{
    ObjectDesc,
    presentation::{Camera, Sprite},
    tilemap::{Tile, TileCell, TileLayout, Tilemap},
};

#[test]
fn prepared_native_frame_keeps_tiles_camera_clear_and_releases_on_unload() {
    let world = fixture();
    let mut header = DisunityPresentation::default();
    assert_eq!(
        unsafe { disunity_presentation_prepare(world, 80, 40, 18, &mut header) }.code,
        0
    );
    assert_eq!((header.present, header.quads, header.textures), (1, 1, 0));
    assert_eq!(
        [header.r, header.g, header.b, header.a],
        [0.1, 0.2, 0.3, 0.25]
    );
    let mut quad = DisunityQuad::default();
    assert_eq!(
        unsafe { disunity_presentation_quad(world, 0, &mut quad) }.code,
        0
    );
    assert_eq!(quad.texture, 17);
    assert_eq!([quad.p0_x, quad.p0_y, quad.p0_z], [2.5, 0., 2.5]);
    assert_eq!([quad.r, quad.g, quad.b, quad.a], [0.2, 0.4, 0.6, 0.5]);
    crate::store::destroy_world(&mut crate::lock_store(), world).unwrap();
    assert_ne!(
        unsafe { disunity_presentation_quad(world, 0, &mut quad) }.code,
        0
    );
}

fn fixture() -> u64 {
    {
        let mut store = crate::lock_store();
        let world = crate::store::create_world(&mut store).unwrap();
        let session = store.session_mut(world).unwrap();
        let camera = session.native.create_object(ObjectDesc::default()).unwrap();
        session
            .native
            .add_component(
                camera,
                ComponentDesc::Camera(Camera {
                    background: [0.1, 0.2, 0.3, 0.25],
                    ..Default::default()
                }),
            )
            .unwrap();
        let object = session.native.create_object(ObjectDesc::default()).unwrap();
        session
            .native
            .add_component(
                object,
                ComponentDesc::Tilemap(Tilemap {
                    layout: TileLayout {
                        tile_anchor: glam::Vec2::ZERO,
                        ..Default::default()
                    },
                    tiles: vec![Tile {
                        sprite: Some(Sprite {
                            identity: 0,
                            texture: 17,
                            rect: [0., 0., 2., 2.],
                            pivot: [0.25, 0.75],
                            pixels_per_unit: 1.,
                            texture_size: [2, 2],
                        }),
                        color: [0.2, 0.4, 0.6, 0.5],
                        ..Default::default()
                    }],
                    cells: vec![TileCell {
                        position: [3, 4],
                        tile: 0,
                    }],
                    ..Default::default()
                }),
            )
            .unwrap();
        world
    }
}

fn quad_with(
    texture: Option<u64>,
) -> worldrunner_world_simulation::presentation::extraction::SpriteQuad {
    use worldrunner_world_simulation::presentation::{SpriteLighting, extraction::SpriteQuad};
    SpriteQuad {
        lighting: SpriteLighting::Unlit,
        overlay: false,
        corners: [[0.; 3]; 4],
        uv_rect: [0., 0., 1., 1.],
        color: [1.; 4],
        texture,
        sorting_layer: 0,
        sorting_order: 0,
    }
}

#[test]
fn glyph_textures_start_above_scene_textures_and_below_the_player_range() {
    let player = crate::textures::FIRST_PLAYER_TEXTURE;
    assert_eq!(next_glyph_texture(&[], 18), Some(18));
    assert_eq!(next_glyph_texture(&[quad_with(Some(40))], 18), Some(41));
    assert_eq!(next_glyph_texture(&[quad_with(Some(3))], 18), Some(18));
    let shown = [
        quad_with(Some(40)),
        quad_with(Some(player + 5)),
        quad_with(None),
    ];
    assert_eq!(
        next_glyph_texture(&shown, 18),
        Some(41),
        "a player texture does not raise it"
    );
    assert_eq!(
        next_glyph_texture(&[quad_with(Some(player + 5))], 18),
        Some(18)
    );
    assert_eq!(
        next_glyph_texture(&[quad_with(Some(player - 1))], 18),
        Some(player)
    );
}

mod mesh_objects {
    use super::*;
    use crate::graphics::{
        disunity_material_create, disunity_mesh_renderer_materials_write, disunity_shader_create,
    };
    use crate::graphics_properties::{
        DisunityTextureSlot, disunity_shader_texture_write, disunity_shader_vector_write,
    };
    use crate::mesh_renderer::{
        DisunityDrawnMaterial, DisunityMeshRenderer, disunity_mesh_filter_create,
        disunity_mesh_filter_drawn_write, disunity_mesh_renderer_create,
        disunity_mesh_renderer_read, disunity_mesh_renderer_write,
    };
    use worldrunner_world_simulation::{
        LocalTransform, MeshFilter, MeshRenderer, presentation::Camera,
    };

    const MESH: u64 = (1 << 41) + 2;

    /// A world with a camera and one mesh object known to the ABI as object 7.
    fn world_with_mesh_object() -> u64 {
        let mut store = crate::lock_store();
        let world = crate::store::create_world(&mut store).unwrap();
        let session = store.session_mut(world).unwrap();
        let camera = session.native.create_object(ObjectDesc::default()).unwrap();
        let camera_desc = ComponentDesc::Camera(Camera::default());
        session.native.add_component(camera, camera_desc).unwrap();
        let object = session
            .native
            .create_object(ObjectDesc {
                local_transform: LocalTransform {
                    translation: glam::Vec3::new(1.0, 2.0, 3.0),
                    ..Default::default()
                },
                ..Default::default()
            })
            .unwrap();
        session.objects.insert(7, object);
        let filter = MeshFilter {
            mesh: 4,
            drawn_mesh: MESH,
        };
        let renderer = MeshRenderer {
            material_count: 2,
            ..Default::default()
        };
        let session = store.session_mut(world).unwrap();
        for desc in [
            ComponentDesc::MeshFilter(filter),
            ComponentDesc::MeshRenderer(renderer),
        ] {
            session.native.add_component(object, desc).unwrap();
        }
        world
    }

    fn prepare(world: u64) -> DisunityPresentation {
        let mut header = DisunityPresentation::default();
        let result = unsafe { disunity_presentation_prepare(world, 80, 40, 18, &mut header) };
        assert_eq!(result.code, 0);
        header
    }

    fn mesh_object(
        world: u64,
        index: u32,
        capacity: usize,
    ) -> (
        DisunityResult,
        DisunityMeshObject,
        DisunityMatrix,
        Vec<DisunityDrawnMaterial>,
    ) {
        let mut object = DisunityMeshObject::default();
        let mut matrix = DisunityMatrix::default();
        let mut slots = vec![DisunityDrawnMaterial::default(); capacity];
        let pointer = if capacity == 0 {
            std::ptr::null_mut()
        } else {
            slots.as_mut_ptr()
        };
        let result = unsafe {
            disunity_presentation_mesh_object(
                world,
                index,
                &mut object,
                &mut matrix,
                pointer,
                u32::try_from(capacity).unwrap(),
            )
        };
        (result, object, matrix, slots)
    }

    #[test]
    fn a_prepared_frame_lists_its_mesh_objects_with_matrix_and_slots() {
        let world = world_with_mesh_object();
        assert_eq!(prepare(world).mesh_objects, 1);
        let (result, object, matrix, slots) = mesh_object(world, 0, 2);
        assert_eq!(result.code, 0);
        assert_eq!(
            (object.object, object.mesh, object.slot_count),
            (7, MESH, 2)
        );
        assert_eq!([matrix.m12, matrix.m13, matrix.m14], [1.0, -3.0, 2.0]);
        for slot in slots {
            let white = DisunityDrawnMaterial {
                r: 1.0,
                g: 1.0,
                b: 1.0,
                a: 1.0,
                texture: 0,
            };
            assert_eq!(slot, white, "a slot with no world material draws white");
        }
    }

    #[test]
    fn a_zero_capacity_reports_the_slot_count_and_a_small_one_is_rejected() {
        let world = world_with_mesh_object();
        prepare(world);
        let (result, object, _, _) = mesh_object(world, 0, 0);
        assert_eq!((result.code, object.slot_count), (0, 2));
        assert_ne!(mesh_object(world, 0, 1).0.code, 0);
    }

    #[test]
    fn an_index_past_the_frame_or_before_any_frame_is_rejected() {
        let world = world_with_mesh_object();
        assert_ne!(mesh_object(world, 0, 0).0.code, 0, "nothing prepared yet");
        prepare(world);
        assert_ne!(mesh_object(world, 1, 0).0.code, 0);
    }

    /// A world, built through the ABI, with one renderer of one slot whose world material holds no
    /// value yet. Returns the world and the material.
    fn abi_world_with_material_slot() -> (u64, u64) {
        let (mut world, mut object, mut filter, mut renderer) = (0, 0, 0, 0);
        let (mut shader, mut material) = (0, 0);
        unsafe {
            assert_eq!(crate::abi::disunity_world_create(&mut world).code, 0);
            assert_eq!(
                crate::abi::disunity_gameobject_create(world, &mut object).code,
                0
            );
            let made = disunity_mesh_filter_create(world, object, &mut filter);
            assert_eq!(made.code, 0);
            assert_eq!(
                disunity_mesh_filter_drawn_write(world, filter, MESH).code,
                0
            );
            let made = disunity_mesh_renderer_create(world, object, 0, &mut renderer);
            assert_eq!(made.code, 0);
            let mut value: DisunityMeshRenderer = std::mem::zeroed();
            assert_eq!(
                disunity_mesh_renderer_read(world, renderer, &mut value).code,
                0
            );
            value.material_count = 1;
            assert_eq!(
                disunity_mesh_renderer_write(world, renderer, &value).code,
                0
            );
            let made = disunity_shader_create(world, b"Lit".as_ptr(), 3, &mut shader);
            assert_eq!(made.code, 0);
            assert_eq!(
                disunity_material_create(world, shader, &mut material).code,
                0
            );
            let slots = [material];
            let linked = disunity_mesh_renderer_materials_write(world, renderer, slots.as_ptr(), 1);
            assert_eq!(linked.code, 0);
        }
        (world, material)
    }

    fn set_material_vector(world: u64, material: u64, name: &str, value: [f32; 4]) {
        let result = unsafe {
            disunity_shader_vector_write(world, 0, material, name.as_ptr(), 6, value.as_ptr())
        };
        assert_eq!(result.code, 0);
    }

    fn set_material_texture(world: u64, material: u64, texture: u64) {
        let slot = DisunityTextureSlot {
            kind: 1,
            texture,
            scale_x: 1.0,
            scale_y: 1.0,
            ..Default::default()
        };
        let name = "_MainTex";
        let result =
            unsafe { disunity_shader_texture_write(world, 0, material, name.as_ptr(), 8, &slot) };
        assert_eq!(result.code, 0);
    }

    #[test]
    fn a_material_changed_after_the_slot_took_it_is_what_the_frame_draws() {
        let (world, material) = abi_world_with_material_slot();
        set_material_vector(world, material, "_Color", [0.2, 0.4, 0.6, 0.8]);
        set_material_texture(world, material, (1 << 40) + 9);
        assert_eq!(prepare(world).mesh_objects, 1);
        let (result, _, _, slots) = mesh_object(world, 0, 1);
        assert_eq!(result.code, 0);
        let expected = DisunityDrawnMaterial {
            r: 0.2,
            g: 0.4,
            b: 0.6,
            a: 0.8,
            texture: (1 << 40) + 9,
        };
        assert_eq!(slots[0], expected);

        set_material_vector(world, material, "_Color", [0.9, 0.8, 0.7, 0.6]);
        prepare(world);
        let (_, _, _, slots) = mesh_object(world, 0, 1);
        assert_eq!((slots[0].r, slots[0].a), (0.9, 0.6));
    }

    #[test]
    fn a_frame_without_mesh_objects_lists_none() {
        let world = fixture();
        assert_eq!(prepare(world).mesh_objects, 0);
    }
}
