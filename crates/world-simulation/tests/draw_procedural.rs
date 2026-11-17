//! A command buffer's procedural draw: recorded in call order with the frame's other work,
//! refused when the matrix is not finite, the material is gone or the pass is below -1, carried
//! by the wire under its own tag, and copied into the frame's snapshot with its material. Zero
//! vertex or instance counts are valid ordered records.
use worldrunner_world_simulation::graphics::snapshot_wire::{decode_work, encode_work};
use worldrunner_world_simulation::graphics::{
    DrawTopology, GraphicsError, GraphicsSnapshot, MaterialId, OffscreenWork, WorldShader,
};
use worldrunner_world_simulation::{World, WorldError, WorldSettings};

fn world() -> World {
    World::new(WorldSettings::default()).unwrap()
}

fn material(world: &mut World) -> MaterialId {
    let shader = world
        .create_shader(WorldShader {
            name: "Hidden/Generated".into(),
            source: None,
        })
        .unwrap();
    world.create_material(shader).unwrap()
}

fn matrix(seed: f32) -> [f32; 16] {
    std::array::from_fn(|index| seed + 0.25 * f32::from(u8::try_from(index).unwrap()))
}

fn draw(material: MaterialId, topology: DrawTopology, counts: (u32, u32)) -> OffscreenWork {
    OffscreenWork::DrawProcedural {
        matrix: matrix(2.0),
        material,
        pass: -1,
        topology,
        vertex_count: counts.0,
        instance_count: counts.1,
    }
}

fn graphics_error(result: Result<(), WorldError>) -> GraphicsError {
    match result {
        Err(WorldError::Graphics(error)) => error,
        other => panic!("expected a graphics error, got {other:?}"),
    }
}

#[test]
fn procedural_draws_keep_call_order_with_other_work_including_empty_draws() {
    let mut world = world();
    let material = material(&mut world);
    let records = vec![
        draw(material, DrawTopology::TriangleList, (6, 2)),
        OffscreenWork::SetGlobalKeyword {
            name: "_FOG".into(),
            enabled: true,
        },
        draw(material, DrawTopology::LineList, (0, 1)),
        draw(material, DrawTopology::TriangleList, (3, 0)),
    ];
    for record in records.clone() {
        world.record_offscreen(record).unwrap();
    }
    assert_eq!(world.take_offscreen(), records);
    assert!(world.take_offscreen().is_empty());
}

#[test]
fn a_bad_matrix_material_or_pass_is_refused_and_earlier_work_stays() {
    let mut world = world();
    let material = material(&mut world);
    let kept = draw(material, DrawTopology::TriangleList, (3, 1));
    world.record_offscreen(kept.clone()).unwrap();
    let mut values = matrix(1.0);
    values[13] = f32::NAN;
    let not_finite = OffscreenWork::DrawProcedural {
        matrix: values,
        material,
        pass: 0,
        topology: DrawTopology::TriangleList,
        vertex_count: 3,
        instance_count: 1,
    };
    assert_eq!(
        graphics_error(world.record_offscreen(not_finite)),
        GraphicsError::NotFinite
    );
    let low = OffscreenWork::DrawProcedural {
        matrix: matrix(0.0),
        material,
        pass: -2,
        topology: DrawTopology::LineList,
        vertex_count: 2,
        instance_count: 1,
    };
    assert_eq!(
        graphics_error(world.record_offscreen(low)),
        GraphicsError::InvalidOffscreen
    );
    let gone = self::material(&mut world);
    world.destroy_material(gone).unwrap();
    assert_eq!(
        graphics_error(world.record_offscreen(draw(gone, DrawTopology::LineList, (2, 1)))),
        GraphicsError::StaleMaterial
    );
    assert_eq!(world.offscreen_work(), &[kept][..]);
}

#[test]
fn the_wire_carries_every_field_and_refuses_an_unknown_topology() {
    let record = OffscreenWork::DrawProcedural {
        matrix: matrix(-1.5),
        material: MaterialId::from_bits(0x0000_0003_0000_0002),
        pass: 4,
        topology: DrawTopology::LineList,
        vertex_count: 0x0001_0203,
        instance_count: 0x0405_0607,
    };
    let bytes = encode_work(&record).unwrap();
    assert_eq!(decode_work(&bytes).unwrap(), record);
    assert!(decode_work(&bytes[..bytes.len() - 1]).is_err());
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert!(decode_work(&trailing).is_err());
    // The topology byte follows the tag, the matrix, the material and the pass.
    let mut unknown = bytes;
    unknown[1 + 16 * 4 + 8 + 4] = 0xEE;
    assert!(decode_work(&unknown).is_err());
}

#[test]
fn the_snapshot_copies_the_material_a_procedural_draw_names_with_its_shader() {
    let mut world = world();
    let material = material(&mut world);
    let record = draw(material, DrawTopology::TriangleList, (6, 1));
    world.record_offscreen(record.clone()).unwrap();
    let snapshot = GraphicsSnapshot::take(&mut world).unwrap();
    assert_eq!(snapshot.work, vec![record]);
    let shader = snapshot.materials[&material].shader;
    assert_eq!(snapshot.shaders[&shader].name, "Hidden/Generated");
}
