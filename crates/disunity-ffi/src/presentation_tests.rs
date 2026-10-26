use super::*;
use worldrunner_world_simulation::{
    ObjectDesc,
    presentation::{Camera, Sprite, Tile, Tilemap},
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
                    tiles: vec![Tile {
                        sprite: Sprite {
                            identity: 0,
                            texture: 17,
                            rect: [0., 0., 2., 2.],
                            pivot: [0.25, 0.75],
                            pixels_per_unit: 1.,
                            texture_size: [2, 2],
                        },
                        transform: glam::Mat4::from_translation(glam::Vec3::new(3., 4., 0.)),
                        color: [0.2, 0.4, 0.6, 0.5],
                    }],
                    ..Default::default()
                }),
            )
            .unwrap();
        world
    }
}
