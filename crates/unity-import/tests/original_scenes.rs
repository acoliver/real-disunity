//! Every engine component in the three original games' scenes and referenced prefabs reads
//! through its typed view.
use std::path::Path;
use worldrunner_original_games::{Game, checkout};
use worldrunner_unity_import::{
    BodyType, ClearFlags, ColliderShape, Interpolation, Object, ObjectId, Project, Scene,
    import_scene,
};

fn scene(game: Game) -> Scene {
    let root = checkout(game).unwrap();
    let path = match game {
        Game::Snake => "Assets/Scenes/GameScene.unity",
        Game::Breakout => "Assets/Scenes/SampleScene.unity",
        Game::Pong => "Assets/Scenes/Main.unity",
    };
    import_scene(&Project::open(&root).unwrap(), Path::new(path)).unwrap()
}

/// Reads `object` through its view, returning a short description of what it holds.
fn read(scene: &Scene, id: ObjectId, object: &Object) -> Option<String> {
    let text = match object.class_id {
        20 => format!("{:?}", scene.camera(id).unwrap()),
        212 => format!("{:?}", scene.sprite_renderer(id).unwrap()),
        483_693_784 => format!("{:?}", scene.tilemap_renderer(id).unwrap()),
        50 => format!("{:?}", scene.rigidbody_2d(id).unwrap()),
        58 | 61 | 19_719_996 => format!("{:?}", scene.collider(id).unwrap()),
        81 | 92 | 124 => format!("{:?}", scene.camera_participant(id).unwrap()),
        _ => return None,
    };
    Some(text)
}

fn views(scene: &Scene) -> Vec<(i32, String)> {
    scene
        .objects()
        .filter_map(|(id, object)| Some((object.class_id, read(scene, id, object)?)))
        .collect()
}

fn count(views: &[(i32, String)], class: i32) -> usize {
    views.iter().filter(|(c, _)| *c == class).count()
}

#[test]
fn every_original_component_reads_through_its_view() {
    let counts = |game| {
        let views = views(&scene(game));
        [20, 212, 50, 61, 58, 19_719_996, 483_693_784].map(|class| count(&views, class))
    };
    // Camera, SpriteRenderer, Rigidbody2D, Box, Circle, TilemapCollider2D, TilemapRenderer.
    assert_eq!(counts(Game::Snake), [1, 2, 0, 0, 0, 0, 0]);
    assert_eq!(counts(Game::Breakout), [1, 5, 2, 3, 1, 1, 1]);
    // Pong's Enemy.prefab is in the project but not in the scene.
    assert_eq!(counts(Game::Pong), [1, 3, 3, 9, 0, 0, 0]);
}

#[test]
fn camera_bodies_and_colliders_carry_the_authored_values() {
    let pong = scene(Game::Pong);
    let camera = pong
        .objects()
        .find(|(_, o)| o.class_id == 20)
        .map(|(id, _)| pong.camera(id).unwrap())
        .unwrap();
    assert_eq!(
        (camera.clear, camera.orthographic_size),
        (ClearFlags::Skybox, 8.)
    );
    let bodies: Vec<_> = pong
        .objects()
        .filter(|(_, o)| o.class_id == 50)
        .map(|(id, _)| pong.rigidbody_2d(id).unwrap())
        .collect();
    assert!(
        bodies
            .iter()
            .all(|b| b.body_type == BodyType::Dynamic && b.gravity_scale == 0.)
    );
    assert!(
        bodies
            .iter()
            .all(|b| b.interpolation == Interpolation::Interpolate)
    );
    let mut masses: Vec<_> = bodies.iter().map(|b| b.mass.to_string()).collect();
    masses.sort();
    assert_eq!(masses, ["0.0001", "1000000", "1000000"], "the ball, then both paddles");
    assert!(bodies.iter().all(|b| b.material.is_some()));
    let breakout = scene(Game::Breakout);
    let ball = breakout
        .objects()
        .find(|(_, o)| o.class_id == 58)
        .map(|(id, _)| breakout.collider(id).unwrap())
        .unwrap();
    assert_eq!(ball.shape, ColliderShape::Circle { radius: 0.5 });
    assert!(ball.settings.material.is_some());
    let snake = scene(Game::Snake);
    let camera = snake
        .objects()
        .find(|(_, o)| o.class_id == 20)
        .map(|(id, _)| snake.camera(id).unwrap())
        .unwrap();
    assert_eq!(camera.clear, ClearFlags::SolidColor);
    assert_eq!(
        (camera.orthographic_size, camera.background),
        (11., [0.; 4])
    );
}
