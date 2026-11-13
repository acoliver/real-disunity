//! A command buffer's mesh draw: recorded in call order with the frame's other work, refused when
//! the identity is zero, the matrix is not finite, the material is gone or the pass is below -1,
//! carried by the wire under tag sixteen with its property block, if it has one, and copied into
//! the frame's snapshot with its material.
use worldrunner_world_simulation::graphics::snapshot_wire::{decode_work, encode_work};
use worldrunner_world_simulation::graphics::{
    GraphicsError, GraphicsSnapshot, MaterialId, OffscreenWork, PropertyValue, ShaderProperties,
    TextureRef, TextureSlot, WorldShader,
};
use worldrunner_world_simulation::{World, WorldError, WorldSettings};

fn world() -> World {
    World::new(WorldSettings::default()).unwrap()
}

fn material(world: &mut World) -> MaterialId {
    let shader = world
        .create_shader(WorldShader {
            name: "Hidden/Portrait".into(),
            source: None,
        })
        .unwrap();
    world.create_material(shader).unwrap()
}

fn matrix(seed: f32) -> [f32; 16] {
    std::array::from_fn(|index| seed + 0.5 * f32::from(u8::try_from(index).unwrap()))
}

fn draw(material: MaterialId) -> OffscreenWork {
    OffscreenWork::DrawMesh {
        mesh: 1 << 41,
        matrix: matrix(1.0),
        material,
        submesh: 2,
        pass: -1,
        block: None,
    }
}

fn graphics_error(result: Result<(), WorldError>) -> GraphicsError {
    match result {
        Err(WorldError::Graphics(error)) => error,
        other => panic!("expected a graphics error, got {other:?}"),
    }
}

#[test]
fn mesh_draws_are_recorded_in_call_order_and_taken_with_the_frame() {
    let mut world = world();
    let material = material(&mut world);
    let records = vec![
        draw(material),
        OffscreenWork::SetGlobalKeyword {
            name: "_FOG".into(),
            enabled: true,
        },
        OffscreenWork::DrawMesh {
            mesh: (1 << 41) + 7,
            matrix: matrix(-3.0),
            material,
            submesh: 0,
            pass: 3,
            block: None,
        },
    ];
    for record in records.clone() {
        world.record_offscreen(record).unwrap();
    }
    assert_eq!(world.take_offscreen(), records);
    assert!(world.take_offscreen().is_empty());
}

#[test]
fn a_matrix_value_that_is_not_finite_is_refused() {
    let mut world = world();
    let material = material(&mut world);
    for (index, bad) in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY]
        .into_iter()
        .enumerate()
    {
        let mut values = matrix(1.0);
        values[index * 7] = bad;
        let record = OffscreenWork::DrawMesh {
            mesh: 1 << 41,
            matrix: values,
            material,
            submesh: 0,
            pass: 0,
            block: None,
        };
        assert_eq!(
            graphics_error(world.record_offscreen(record)),
            GraphicsError::NotFinite
        );
    }
    assert!(world.offscreen_work().is_empty());
}

#[test]
fn a_destroyed_material_is_refused() {
    let mut world = world();
    let material = material(&mut world);
    world.destroy_material(material).unwrap();
    assert_eq!(
        graphics_error(world.record_offscreen(draw(material))),
        GraphicsError::StaleMaterial
    );
    assert!(world.offscreen_work().is_empty());
}

#[test]
fn a_zero_mesh_identity_and_a_pass_below_minus_one_are_refused() {
    let mut world = world();
    let material = material(&mut world);
    let zero = OffscreenWork::DrawMesh {
        mesh: 0,
        matrix: matrix(0.0),
        material,
        submesh: 0,
        pass: 0,
        block: None,
    };
    assert_eq!(
        graphics_error(world.record_offscreen(zero)),
        GraphicsError::InvalidOffscreen
    );
    let low = OffscreenWork::DrawMesh {
        mesh: 1 << 41,
        matrix: matrix(0.0),
        material,
        submesh: 0,
        pass: -2,
        block: None,
    };
    assert_eq!(
        graphics_error(world.record_offscreen(low)),
        GraphicsError::InvalidOffscreen
    );
    let accepted = OffscreenWork::DrawMesh {
        mesh: 1 << 41,
        matrix: matrix(0.0),
        material,
        submesh: 0,
        pass: -1,
        block: None,
    };
    world.record_offscreen(accepted).unwrap();
    assert_eq!(world.offscreen_work().len(), 1);
}

#[test]
fn the_wire_carries_every_field_under_tag_sixteen() {
    let record = OffscreenWork::DrawMesh {
        mesh: (1 << 41) + 5,
        matrix: matrix(-1.5),
        material: MaterialId::from_bits(0x0000_0003_0000_0002),
        submesh: 9,
        pass: -1,
        block: None,
    };
    let bytes = encode_work(&record).unwrap();
    assert_eq!(bytes[0], 16);
    assert_eq!(bytes.len(), 1 + 8 + 16 * 4 + 8 + 4 + 4 + 1);
    assert_eq!(decode_work(&bytes).unwrap(), record);
    assert!(decode_work(&bytes[..bytes.len() - 1]).is_err());
}

#[test]
fn the_snapshot_copies_the_material_a_mesh_draw_names_with_its_shader() {
    let mut world = world();
    let material = material(&mut world);
    world.record_offscreen(draw(material)).unwrap();
    let snapshot = GraphicsSnapshot::take(&mut world).unwrap();
    assert_eq!(snapshot.work, vec![draw(material)]);
    assert!(snapshot.materials.contains_key(&material));
    let shader = snapshot.materials[&material].shader;
    assert_eq!(snapshot.shaders[&shader].name, "Hidden/Portrait");
}

/// A block with one value of every kind, a texture by asset identity and two-element arrays.
fn every_kind() -> ShaderProperties {
    let mut block = ShaderProperties::default();
    for value in [
        PropertyValue::Float("_Fade".into(), 0.25),
        PropertyValue::Int("_Mode".into(), -3),
        PropertyValue::Vector("_Tint".into(), [0.5, 1.0, 1.5, 2.0]),
        PropertyValue::Matrix("_Frame".into(), matrix(4.0)),
        PropertyValue::Texture(
            "_Lut".into(),
            TextureSlot {
                texture: TextureRef::Asset(1 << 40),
                scale: [2.0, 3.0],
                offset: [0.25, 0.75],
            },
        ),
        PropertyValue::VectorArray("_Points".into(), vec![[1.0; 4], [2.0; 4]]),
        PropertyValue::MatrixArray("_Bones".into(), vec![matrix(0.0), matrix(9.0)]),
    ] {
        block.set(value).unwrap();
    }
    block
}

#[test]
fn a_draw_with_a_property_block_is_recorded_and_carried_with_every_value() {
    let mut world = world();
    let material = material(&mut world);
    let record = OffscreenWork::DrawMesh {
        mesh: 1 << 41,
        matrix: matrix(2.0),
        material,
        submesh: 0,
        pass: 1,
        block: Some(Box::new(every_kind())),
    };
    world.record_offscreen(record.clone()).unwrap();
    assert_eq!(world.offscreen_work(), std::slice::from_ref(&record));
    let bytes = encode_work(&record).unwrap();
    assert_eq!(bytes[0], 16);
    assert_eq!(decode_work(&bytes).unwrap(), record);
    let plain = encode_work(&draw(material)).unwrap();
    assert_eq!(
        *plain.last().unwrap(),
        0,
        "a draw without a block ends with none"
    );
    assert!(decode_work(&bytes[..bytes.len() - 1]).is_err());
    let snapshot = GraphicsSnapshot::take(&mut world).unwrap();
    assert_eq!(snapshot.work, vec![record]);
}

#[test]
fn a_block_that_names_a_gone_render_texture_or_no_texture_is_refused() {
    let mut world = world();
    let material = material(&mut world);
    let with = |texture| {
        let mut block = ShaderProperties::default();
        block.textures.push((
            "_Map".into(),
            TextureSlot {
                texture,
                scale: [1.0, 1.0],
                offset: [0.0, 0.0],
            },
        ));
        OffscreenWork::DrawMesh {
            mesh: 1 << 41,
            matrix: matrix(0.0),
            material,
            submesh: 0,
            pass: 0,
            block: Some(Box::new(block)),
        }
    };
    let gone =
        worldrunner_world_simulation::graphics::RenderTextureId::from_bits(0x0000_0001_0000_0007);
    assert_eq!(
        graphics_error(world.record_offscreen(with(TextureRef::Render(gone)))),
        GraphicsError::StaleRenderTexture
    );
    assert_eq!(
        graphics_error(world.record_offscreen(with(TextureRef::Asset(0)))),
        GraphicsError::InvalidTexture
    );
    assert!(world.offscreen_work().is_empty());
    world
        .record_offscreen(with(TextureRef::Asset(1 << 40)))
        .unwrap();
    assert_eq!(world.offscreen_work().len(), 1);
}

#[test]
fn a_draw_names_the_render_textures_its_block_reads() {
    use worldrunner_world_simulation::graphics::RenderTextureId;
    let named = RenderTextureId::from_bits(0x0000_0001_0000_0003);
    let other = RenderTextureId::from_bits(0x0000_0001_0000_0004);
    let mut block = ShaderProperties::default();
    block
        .set(PropertyValue::Texture(
            "_Lut".into(),
            TextureSlot {
                texture: TextureRef::Render(named),
                scale: [1.0, 1.0],
                offset: [0.0, 0.0],
            },
        ))
        .unwrap();
    let material = MaterialId::from_bits(0x0000_0001_0000_0001);
    let with = OffscreenWork::DrawMesh {
        mesh: 1 << 41,
        matrix: matrix(0.0),
        material,
        submesh: 0,
        pass: 0,
        block: Some(Box::new(block)),
    };
    assert!(with.names_render_texture(named));
    assert!(!with.names_render_texture(other));
    assert!(!draw(material).names_render_texture(named));
}
