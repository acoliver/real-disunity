use worldrunner_disunity_ffi::{abi::*, physics::*, presentation::*, sprites::*, tilemap::*};

struct Wall {
    world: u64,
    tilemap: u64,
    collider: u64,
}

/// The brick sprite: 32x16 px at 16 px per unit, pivot at its centre.
fn brick_sprite() -> DisunitySprite {
    DisunitySprite {
        texture: 7,
        width: 64,
        height: 64,
        w: 32.,
        h: 16.,
        pivot_x: 0.5,
        pivot_y: 0.5,
        pixels_per_unit: 16.,
        ..DisunitySprite::default()
    }
}

fn brick_tile() -> DisunityTile {
    DisunityTile {
        collider: 1,
        r: 1.,
        g: 1.,
        b: 1.,
        a: 1.,
        m00: 1.,
        m11: 1.,
        ..DisunityTile::default()
    }
}

fn breakout_cells() -> Vec<DisunityTileCell> {
    let row = |y| (-5..=4).map(move |x| DisunityTileCell { x, y, tile: 0 });
    let column = |x| (-5..=8).map(move |y| DisunityTileCell { x, y, tile: 0 });
    row(-6)
        .chain(row(9))
        .chain(column(-5))
        .chain(column(4))
        .collect()
}

/// Breakout's wall through the ABI: layout, one Sprite-collider tile, its shape, 48 cells.
fn wall() -> Wall {
    let (mut world, mut object, mut tilemap, mut collider) = (0, 0, 0, 0);
    unsafe {
        assert_eq!(disunity_world_create(&mut world).code, 0);
        assert_eq!(disunity_gameobject_create(world, &mut object).code, 0);
        assert_eq!(disunity_tilemap_create(world, object, &mut tilemap).code, 0);
        assert_eq!(
            disunity_physics_create(world, object, 3, &mut collider).code,
            0
        );
    }
    let layout = DisunityTilemap {
        enabled: 1,
        r: 1.,
        g: 1.,
        b: 1.,
        a: 1.,
        cell_x: 2.,
        cell_y: 1.,
        anchor_x: 0.5,
        anchor_y: 0.5,
        ..DisunityTilemap::default()
    };
    let shape = [-1., -0.5, 1., -0.5, 1., 0.5, -1., 0.5_f32];
    let cells = breakout_cells();
    unsafe {
        assert_eq!(disunity_tilemap_write(world, tilemap, &layout).code, 0);
        assert_eq!(
            disunity_tilemap_set_tile(world, tilemap, 0, &brick_tile(), &brick_sprite()).code,
            0
        );
        let result = disunity_tilemap_set_tile_shape(world, tilemap, 0, shape.as_ptr(), &4, 1);
        assert_eq!(result.code, 0);
        let count = u32::try_from(cells.len()).unwrap();
        assert_eq!(
            disunity_tilemap_set_cells(world, tilemap, cells.as_ptr(), count).code,
            0
        );
    }
    Wall {
        world,
        tilemap,
        collider,
    }
}

impl Wall {
    /// The collider a leftward ray from (x, y) hits first, or zero.
    fn ray_left(&self, x: f32, y: f32) -> u64 {
        let mut hit = DisunityRayHit::default();
        let result =
            unsafe { disunity_physics_raycast(self.world, x, y, -1., 0., 50., u32::MAX, &mut hit) };
        assert_eq!(result.code, 0);
        hit.collider
    }
    fn quads(&self) -> u32 {
        let mut header = DisunityPresentation::default();
        let result = unsafe { disunity_presentation_prepare(self.world, 64, 64, 1, &mut header) };
        assert_eq!(result.code, 0);
        header.quads
    }
}

impl Drop for Wall {
    fn drop(&mut self) {
        unsafe { assert_eq!(disunity_world_destroy(self.world).code, 0) };
    }
}

#[test]
fn breakouts_wall_draws_and_collides_from_one_tilemap() {
    let wall = wall();
    assert_eq!(wall.quads(), 48);
    assert_eq!(
        wall.ray_left(0., 0.),
        wall.collider,
        "the left wall stops the ray"
    );
    let mut record = DisunityCollider2d::default();
    assert_eq!(
        unsafe { disunity_collider_read(wall.world, wall.collider, &mut record) }.code,
        0
    );
    assert_eq!(record.kind, 2, "reads back as a tilemap collider");
    let mut layout = DisunityTilemap::default();
    assert_eq!(
        unsafe { disunity_tilemap_read(wall.world, wall.tilemap, &mut layout) }.code,
        0
    );
    assert_eq!(
        (layout.cell_x, layout.cell_y, layout.anchor_x),
        (2., 1., 0.5)
    );
    let open = [DisunityTileCell {
        x: 4,
        y: 0,
        tile: 0,
    }];
    unsafe {
        assert_eq!(
            disunity_tilemap_set_cells(wall.world, wall.tilemap, open.as_ptr(), 1).code,
            0
        );
    }
    assert_eq!(wall.quads(), 1);
    assert_eq!(
        wall.ray_left(0., 0.),
        0,
        "removing cells opens the left wall"
    );
}

#[test]
fn rejected_edits_leave_the_wall_drawn_and_colliding() {
    let wall = wall();
    let (world, map) = (wall.world, wall.tilemap);
    let outside = [DisunityTileCell {
        x: 0,
        y: 0,
        tile: 1,
    }];
    let repeated = [DisunityTileCell::default(), DisunityTileCell::default()];
    let concave = [0., 0., 2., 0., 1., 0.2, 1., 1.0_f32];
    let grid = DisunityTile {
        collider: 2,
        ..brick_tile()
    };
    unsafe {
        assert_ne!(
            disunity_tilemap_set_cells(world, map, outside.as_ptr(), 1).code,
            0
        );
        assert_ne!(
            disunity_tilemap_set_cells(world, map, repeated.as_ptr(), 2).code,
            0
        );
        assert_ne!(
            disunity_tilemap_set_cells(world, map, std::ptr::null(), 1).code,
            0
        );
        let past_end = disunity_tilemap_set_tile(world, map, 2, &brick_tile(), std::ptr::null());
        assert_ne!(past_end.code, 0);
        let unknown = DisunityTile {
            collider: 3,
            ..brick_tile()
        };
        assert_ne!(
            disunity_tilemap_set_tile(world, map, 0, &unknown, std::ptr::null()).code,
            0
        );
        assert_ne!(
            disunity_tilemap_set_tile_shape(world, map, 0, concave.as_ptr(), &4, 1).code,
            0
        );
        assert_ne!(
            disunity_tilemap_set_tile_shape(world, map, 0, concave.as_ptr(), &200, 1).code,
            0
        );
        assert_eq!(
            disunity_tilemap_set_tile(world, map, 1, &grid, std::ptr::null()).code,
            0
        );
        let not_sprite = disunity_tilemap_set_tile_shape(world, map, 1, concave.as_ptr(), &3, 1);
        assert_ne!(
            not_sprite.code, 0,
            "only Sprite-collider tiles take a shape"
        );
    }
    assert_eq!(wall.quads(), 48);
    assert_eq!(wall.ray_left(0., 0.), wall.collider);
}

#[test]
fn rejected_layouts_leave_the_wall_unchanged() {
    let wall = wall();
    let flat = DisunityTilemap {
        cell_y: 0.,
        ..DisunityTilemap::default()
    };
    let flags = DisunityTilemap {
        enabled: 2,
        cell_x: 2.,
        cell_y: 1.,
        ..DisunityTilemap::default()
    };
    for layout in [flat, flags] {
        let result = unsafe { disunity_tilemap_write(wall.world, wall.tilemap, &layout) };
        assert_ne!(result.code, 0, "{layout:?}");
    }
    assert_eq!(wall.quads(), 48);
    assert_eq!(wall.ray_left(0., 0.), wall.collider);
}
