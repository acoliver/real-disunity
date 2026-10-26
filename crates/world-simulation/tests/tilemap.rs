//! Tilemaps are one description that both drawing and `TilemapCollider2D` read.
use glam::{Mat4, Vec2, Vec3};
use worldrunner_world_simulation::physics2d::{
    Body2d, Collider2d, ColliderGeometry, ContactPhase, PhysicsError, QueryFilter, Shape2d,
};
use worldrunner_world_simulation::tilemap::{Tile, TileCell, TileCollider, TileLayout, Tilemap};
use worldrunner_world_simulation::{
    ComponentDesc, ComponentHandle, LocalTransform, ObjectDesc, ObjectHandle, World, WorldError,
    WorldSettings,
};

/// The opaque 2x1 brick sprite's generated physics shape, around its centre pivot.
fn brick_shape() -> Vec<Vec2> {
    vec![
        Vec2::new(-1., -0.5),
        Vec2::new(1., -0.5),
        Vec2::new(1., 0.5),
        Vec2::new(-1., 0.5),
    ]
}

/// Breakout's wall: 2x1 cells, anchor (0.5, 0.5), one Sprite-collider tile, a U of 48 cells.
fn breakout_wall() -> Tilemap {
    let mut cells = Vec::new();
    for x in -5..=4 {
        cells.push([x, -6]);
        cells.push([x, 9]);
    }
    for y in -5..=8 {
        cells.push([-5, y]);
        cells.push([4, y]);
    }
    Tilemap {
        layout: TileLayout {
            cell_size: Vec2::new(2., 1.),
            ..Default::default()
        },
        tiles: vec![Tile {
            collider: TileCollider::Sprite(vec![brick_shape()]),
            ..Default::default()
        }],
        cells: cells
            .into_iter()
            .map(|position| TileCell { position, tile: 0 })
            .collect(),
        ..Default::default()
    }
}

fn world_with(tilemap: Tilemap) -> (World, ObjectHandle, ComponentHandle, ComponentHandle) {
    let mut w = World::new(WorldSettings::default()).unwrap();
    let wall = w.create_object(ObjectDesc::default()).unwrap();
    let map = w
        .add_component(wall, ComponentDesc::Tilemap(tilemap))
        .unwrap();
    let collider = w
        .add_component(
            wall,
            ComponentDesc::Collider2d(Collider2d {
                geometry: ColliderGeometry::Tilemap,
                ..Default::default()
            }),
        )
        .unwrap();
    (w, wall, map, collider)
}

fn hits(w: &World, point: Vec2) -> Vec<ComponentHandle> {
    w.overlap_point(point, QueryFilter::default()).unwrap()
}

#[test]
fn breakouts_wall_cells_become_shapes_where_the_tiles_are_drawn() {
    let wall = breakout_wall();
    assert_eq!(wall.cells.len(), 48);
    assert_eq!(wall.collision_shapes().unwrap().len(), 48);
    let corner = TileCell {
        position: [-5, -6],
        tile: 0,
    };
    let drawn = wall.tile_matrix(corner).transform_point3(Vec3::ZERO);
    assert_eq!(
        drawn,
        Vec3::new(-9., -5.5, 0.),
        "cell origin plus half a cell"
    );
    let (w, _, _, collider) = world_with(wall);
    assert_eq!(
        hits(&w, Vec2::new(-9.9, -5.9)),
        [collider],
        "bottom-left brick"
    );
    assert_eq!(hits(&w, Vec2::new(9.9, 9.9)), [collider], "top-right brick");
    assert!(hits(&w, Vec2::ZERO).is_empty(), "the play area is open");
    assert!(
        hits(&w, Vec2::new(-10.1, 0.)).is_empty(),
        "outside the left wall"
    );
}

#[test]
fn cell_edits_reach_physics_without_rewriting_the_collider() {
    let (mut w, _, map, collider) = world_with(breakout_wall());
    let point = Vec2::new(-9., 0.5);
    assert_eq!(hits(&w, point), [collider]);
    let mut edited = breakout_wall();
    edited.cells.retain(|cell| cell.position != [-5, 0]);
    w.set_component(map, ComponentDesc::Tilemap(edited))
        .unwrap();
    assert!(
        hits(&w, point).is_empty(),
        "the removed cell no longer collides"
    );
    w.destroy_component(map).unwrap();
    w.step_physics(0.02).unwrap();
    assert!(
        hits(&w, Vec2::new(-9., -5.5)).is_empty(),
        "no tilemap, no shapes"
    );
}

#[test]
fn collider_types_follow_unity() {
    let tiles = vec![
        Tile::default(),
        Tile {
            collider: TileCollider::Grid,
            transform: Mat4::from_scale(Vec3::splat(0.25)),
            ..Default::default()
        },
        Tile {
            collider: TileCollider::Sprite(vec![vec![Vec2::ZERO, Vec2::X, Vec2::Y]]),
            transform: Mat4::from_rotation_z(std::f32::consts::FRAC_PI_2),
            ..Default::default()
        },
    ];
    let cells = (0..3)
        .map(|i| TileCell {
            position: [i, 0],
            tile: u32::try_from(i).unwrap(),
        })
        .collect();
    let (w, _, _, collider) = world_with(Tilemap {
        tiles,
        cells,
        ..Default::default()
    });
    assert!(
        hits(&w, Vec2::new(0.5, 0.5)).is_empty(),
        "None has no shape"
    );
    assert_eq!(
        hits(&w, Vec2::new(1.9, 0.9)),
        [collider],
        "Grid fills the cell"
    );
    assert_eq!(
        hits(&w, Vec2::new(2.4, 0.9)),
        [collider],
        "the sprite shape turns with the tile, around the anchored point"
    );
    assert!(hits(&w, Vec2::new(2.9, 0.9)).is_empty());
}

#[test]
fn a_ball_bounces_off_the_wall_and_the_contact_names_the_tilemap_collider() {
    let (mut w, _, _, wall) = world_with(breakout_wall());
    let ball = w
        .create_object(ObjectDesc {
            local_transform: LocalTransform {
                translation: Vec3::new(-7., 0., 0.),
                ..Default::default()
            },
            ..Default::default()
        })
        .unwrap();
    w.add_component(
        ball,
        ComponentDesc::Body2d(Body2d {
            velocity: Vec2::new(-10., 0.),
            gravity_scale: 0.,
            restitution: 1.,
            ..Default::default()
        }),
    )
    .unwrap();
    let ball_collider = w
        .add_component(
            ball,
            ComponentDesc::Collider2d(Collider2d {
                geometry: ColliderGeometry::Shapes(vec![Shape2d::Circle { radius: 0.25 }]),
                ..Default::default()
            }),
        )
        .unwrap();
    let mut entered = false;
    for _ in 0..30 {
        w.step_physics(0.02).unwrap();
        entered |= w.contacts().iter().any(|c| {
            c.phase == ContactPhase::Enter
                && [c.collider_a, c.collider_b].contains(&wall)
                && [c.collider_a, c.collider_b].contains(&ball_collider)
        });
    }
    assert!(entered, "the ball reached the wall");
    let snapshot = w.snapshot();
    let body = snapshot
        .components
        .iter()
        .find_map(|c| match &c.desc {
            ComponentDesc::Body2d(body) if c.object == ball => Some(body),
            _ => None,
        })
        .unwrap();
    assert!(body.velocity.x > 0., "bounced back: {:?}", body.velocity);
    assert!(w.local_transform(ball).unwrap().translation.x > -9.75);
}

#[test]
fn invalid_tilemaps_are_rejected_without_changing_the_world() {
    let (mut w, _, map, _) = world_with(breakout_wall());
    let before = w.snapshot();
    let mut duplicate = breakout_wall();
    duplicate.cells.push(duplicate.cells[0]);
    let mut outside_palette = breakout_wall();
    outside_palette.cells[0].tile = 1;
    let mut far = breakout_wall();
    far.cells[0].position = [40_000, 0];
    let mut flat = breakout_wall();
    flat.layout.cell_size = Vec2::new(2., 0.);
    let mut concave = breakout_wall();
    concave.tiles[0].collider = TileCollider::Sprite(vec![vec![
        Vec2::ZERO,
        Vec2::new(2., 0.),
        Vec2::new(1., 0.2),
        Vec2::new(1., 1.),
    ]]);
    for map_value in [duplicate, outside_palette, far, flat, concave] {
        let result = w.set_component(map, ComponentDesc::Tilemap(map_value));
        assert!(result.is_err());
        assert_eq!(w.snapshot(), before);
    }
    let mut degenerate = breakout_wall();
    degenerate.tiles[0].transform = Mat4::from_scale(Vec3::new(1., 0., 1.));
    w.set_component(map, ComponentDesc::Tilemap(degenerate))
        .unwrap();
    assert_eq!(
        w.step_physics(0.02),
        Err(WorldError::Physics(PhysicsError::InvalidShape)),
        "a transform that flattens the shape fails the step"
    );
}
