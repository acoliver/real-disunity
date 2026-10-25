#![cfg(feature = "simulation")]
use worldrunner_render_plan::simulation::{SpriteTextures, extract_world};
use worldrunner_world_simulation::{
    ComponentDesc, ObjectDesc, World, WorldSettings,
    presentation::{Sprite, SpriteRenderer},
};
#[test]
fn sorting_is_layer_then_order_with_submission_ties_and_inactive_exclusion() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    for (texture, layer, order, active, enabled) in [
        (1, 2, 0, true, true),
        (2, 1, 3, true, true),
        (3, 1, 3, true, true),
        (4, 1, -1, true, true),
        (5, -9, 0, false, true),
        (6, -9, 0, true, false),
    ] {
        let object = world
            .create_object(ObjectDesc {
                active,
                ..Default::default()
            })
            .unwrap();
        world
            .add_component(
                object,
                ComponentDesc::Sprite(SpriteRenderer {
                    enabled,
                    sprite: Some(Sprite {
                        identity: 0,
                        texture,
                        texture_size: [1, 1],
                        rect: [0., 0., 1., 1.],
                        pixels_per_unit: 1.,
                        pivot: [0.5, 0.5],
                    }),
                    sorting_layer: layer,
                    sorting_order: order,
                    ..Default::default()
                }),
            )
            .unwrap();
    }
    let plan = extract_world(&world, &SpriteTextures::default(), [10, 10]).unwrap();
    assert_eq!(
        plan.quads.iter().map(|q| q.texture).collect::<Vec<_>>(),
        [Some(4), Some(2), Some(3), Some(1)]
    );
}
#[test]
fn invalid_sprite_subrect_does_not_publish_a_component() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let object = world.create_object(ObjectDesc::default()).unwrap();
    let before = world.snapshot();
    assert!(
        world
            .add_component(
                object,
                ComponentDesc::Sprite(SpriteRenderer {
                    sprite: Some(Sprite {
                        identity: 0,
                        texture: 1,
                        texture_size: [2, 2],
                        rect: [1., 0., 2., 1.],
                        pixels_per_unit: 1.,
                        pivot: [0.5, 0.5]
                    }),
                    ..Default::default()
                })
            )
            .is_err()
    );
    assert_eq!(world.snapshot(), before);
}
