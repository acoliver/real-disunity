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
    use crate::mesh_renderer::DisunityDrawnMaterial;
    use worldrunner_world_simulation::{
        DrawnMaterial, LocalTransform, MeshFilter, MeshRenderer, presentation::Camera,
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
            drawn: vec![DrawnMaterial {
                color: [0.1, 0.2, 0.3, 0.4],
                texture: (1 << 40) + 9,
            }],
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
        assert_eq!(
            slots[0],
            DisunityDrawnMaterial {
                r: 0.1,
                g: 0.2,
                b: 0.3,
                a: 0.4,
                texture: (1 << 40) + 9
            }
        );
        assert_eq!(slots[1].texture, 0);
        assert_eq!((slots[1].r, slots[1].a), (1.0, 1.0));
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

    #[test]
    fn a_frame_without_mesh_objects_lists_none() {
        let world = fixture();
        assert_eq!(prepare(world).mesh_objects, 0);
    }
}
