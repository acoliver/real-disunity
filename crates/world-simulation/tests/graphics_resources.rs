//! Shaders, materials, the global shader state and render textures of a world.
use worldrunner_world_simulation::graphics::{
    GraphicsError, MaterialEdit, MaterialId, PropertyValue, RenderTextureId, ShaderId,
    ShaderSource, TextureRef, TextureSlot, WorldRenderTexture, WorldSampler, WorldShader,
};
use worldrunner_world_simulation::{World, WorldError, WorldSettings};

fn world() -> World {
    World::new(WorldSettings::default()).unwrap()
}

fn shader(name: &str) -> WorldShader {
    WorldShader {
        name: name.into(),
        source: None,
    }
}

fn target() -> WorldRenderTexture {
    WorldRenderTexture {
        width: 64,
        height: 32,
        format: 8,
        depth_bits: 24,
        mip_count: 3,
        srgb: true,
        sampler: WorldSampler::default(),
        generation: 99,
    }
}

fn material_with_shader(world: &mut World) -> (ShaderId, MaterialId) {
    let shader = world.create_shader(shader("Hidden/Splat")).unwrap();
    let material = world.create_material(shader).unwrap();
    (shader, material)
}

fn graphics_error(result: Result<(), WorldError>) -> GraphicsError {
    match result {
        Err(WorldError::Graphics(error)) => error,
        other => panic!("expected a graphics error, got {other:?}"),
    }
}

#[test]
fn a_shader_keeps_its_name_and_source() {
    let mut world = world();
    let source = ShaderSource {
        file: "sharedassets0.assets".into(),
        path_id: -42,
    };
    let id = world
        .create_shader(WorldShader {
            name: "Terrain/Splat".into(),
            source: Some(source.clone()),
        })
        .unwrap();
    let held = world.shader(id).unwrap();
    assert_eq!(held.name, "Terrain/Splat");
    assert_eq!(held.source, Some(source));
    world.update_shader(id, shader("Terrain/Other")).unwrap();
    let held = world.shader(id).unwrap();
    assert_eq!(
        (held.name.as_str(), held.source.clone()),
        ("Terrain/Other", None)
    );
}

#[test]
fn a_shader_needs_a_name_and_a_usable_source() {
    let mut world = world();
    assert_eq!(
        graphics_error(world.create_shader(shader("")).map(drop)),
        GraphicsError::InvalidName
    );
    let empty_file = WorldShader {
        name: "A".into(),
        source: Some(ShaderSource {
            file: String::new(),
            path_id: 1,
        }),
    };
    assert_eq!(
        graphics_error(world.create_shader(empty_file).map(drop)),
        GraphicsError::InvalidShaderSource
    );
}

#[test]
fn a_destroyed_shader_is_stale_and_its_slot_is_reused_with_a_new_id() {
    let mut world = world();
    let first = world.create_shader(shader("A")).unwrap();
    world.destroy_shader(first).unwrap();
    assert_eq!(
        graphics_error(world.shader(first).map(drop)),
        GraphicsError::StaleShader
    );
    let second = world.create_shader(shader("B")).unwrap();
    assert_ne!(first, second);
    assert_eq!(world.shader(second).unwrap().name, "B");
    assert_eq!(
        graphics_error(world.shader(first).map(drop)),
        GraphicsError::StaleShader
    );
    assert_eq!(
        graphics_error(world.destroy_shader(first)),
        GraphicsError::StaleShader
    );
}

#[test]
fn ids_round_trip_through_their_bits_and_zero_names_nothing() {
    let mut world = world();
    let id = world.create_shader(shader("A")).unwrap();
    assert_ne!(id.to_bits(), 0);
    assert_eq!(ShaderId::from_bits(id.to_bits()), id);
    assert!(world.shader(ShaderId::from_bits(0)).is_err());
}

#[test]
fn a_new_material_has_the_shader_the_default_queue_and_generation_one() {
    let mut world = world();
    let (shader, id) = material_with_shader(&mut world);
    let material = world.material(id).unwrap();
    assert_eq!(material.shader, shader);
    assert_eq!(material.render_queue, -1);
    assert!(material.keywords.is_empty() && material.disabled_passes.is_empty());
    assert!(material.properties.is_empty());
    assert_eq!(material.generation, 1);
}

#[test]
fn a_material_needs_a_live_shader() {
    let mut world = world();
    let shader = world.create_shader(shader("A")).unwrap();
    world.destroy_shader(shader).unwrap();
    assert_eq!(
        graphics_error(world.create_material(shader).map(drop)),
        GraphicsError::StaleShader
    );
}

#[test]
fn every_material_edit_raises_the_generation() {
    let mut world = world();
    let (_, id) = material_with_shader(&mut world);
    let other = world.create_shader(shader("Other")).unwrap();
    let edits = [
        MaterialEdit::Shader(other),
        MaterialEdit::RenderQueue(2000),
        MaterialEdit::Keyword {
            name: "FOG".into(),
            enabled: true,
        },
        MaterialEdit::Pass {
            name: "ShadowCaster".into(),
            enabled: false,
        },
        MaterialEdit::Property(PropertyValue::Float("_Cutoff".into(), 0.5)),
    ];
    for (index, edit) in edits.into_iter().enumerate() {
        world.edit_material(id, edit).unwrap();
        assert_eq!(
            world.material(id).unwrap().generation,
            2 + u64::try_from(index).unwrap()
        );
    }
    let material = world.material(id).unwrap();
    assert_eq!(material.shader, other);
    assert_eq!(material.render_queue, 2000);
    assert_eq!(material.keywords, ["FOG"]);
    assert_eq!(material.disabled_passes, ["ShadowCaster"]);
}

#[test]
fn a_rejected_edit_changes_nothing() {
    let mut world = world();
    let (_, id) = material_with_shader(&mut world);
    let before = world.material(id).unwrap().clone();
    let rejected = world.edit_material(
        id,
        MaterialEdit::Property(PropertyValue::Float("_Gloss".into(), f32::NAN)),
    );
    assert_eq!(graphics_error(rejected), GraphicsError::NotFinite);
    assert_eq!(*world.material(id).unwrap(), before);
}

#[test]
fn keywords_stay_sorted_and_unique_and_can_be_removed() {
    let mut world = world();
    let (_, id) = material_with_shader(&mut world);
    for name in ["B", "A", "B", "C"] {
        let name = name.into();
        let enable = MaterialEdit::Keyword {
            name,
            enabled: true,
        };
        world.edit_material(id, enable).unwrap();
    }
    assert_eq!(world.material(id).unwrap().keywords, ["A", "B", "C"]);
    let disable = MaterialEdit::Keyword {
        name: "B".into(),
        enabled: false,
    };
    world.edit_material(id, disable).unwrap();
    assert_eq!(world.material(id).unwrap().keywords, ["A", "C"]);
    for name in ["B", "D"] {
        let enable = MaterialEdit::Keyword {
            name: name.into(),
            enabled: true,
        };
        world.edit_material(id, enable).unwrap();
    }
    assert_eq!(world.material(id).unwrap().keywords, ["A", "B", "C", "D"]);
}

#[test]
fn switching_a_pass_off_and_on_again_leaves_it_out_of_the_disabled_list() {
    let mut world = world();
    let (_, id) = material_with_shader(&mut world);
    for (name, enabled) in [("Z", false), ("A", false), ("Z", true)] {
        let name = name.into();
        world
            .edit_material(id, MaterialEdit::Pass { name, enabled })
            .unwrap();
    }
    assert_eq!(world.material(id).unwrap().disabled_passes, ["A"]);
}

#[test]
fn setting_a_property_replaces_its_value_and_keeps_the_names_sorted() {
    let mut world = world();
    let (_, id) = material_with_shader(&mut world);
    let sets = [
        PropertyValue::Float("_b".into(), 1.0),
        PropertyValue::Float("_a".into(), 2.0),
        PropertyValue::Float("_b".into(), 3.0),
        PropertyValue::Int("_n".into(), -4),
        PropertyValue::Vector("_v".into(), [1.0, 2.0, 3.0, 4.0]),
        PropertyValue::Matrix("_m".into(), [2.0; 16]),
    ];
    for value in sets {
        world
            .edit_material(id, MaterialEdit::Property(value))
            .unwrap();
    }
    let properties = &world.material(id).unwrap().properties;
    assert_eq!(
        properties.floats,
        [("_a".to_owned(), 2.0), ("_b".to_owned(), 3.0)]
    );
    assert_eq!(properties.int("_n"), Some(-4));
    assert_eq!(properties.vector("_v"), Some([1.0, 2.0, 3.0, 4.0]));
    assert_eq!(properties.matrix("_m"), Some([2.0; 16]));
    assert_eq!(properties.float("_missing"), None);
}

#[test]
fn a_texture_property_keeps_its_scale_and_offset() {
    let mut world = world();
    let (_, id) = material_with_shader(&mut world);
    let render = world.create_render_texture(target()).unwrap();
    let slots = [
        ("_Asset", TextureRef::Asset(1 << 40)),
        ("_Render", TextureRef::Render(render)),
        ("_None", TextureRef::None),
    ];
    for (name, texture) in slots {
        let slot = TextureSlot {
            texture,
            scale: [2.0, 3.0],
            offset: [0.25, 0.5],
        };
        let value = PropertyValue::Texture(name.into(), slot);
        world
            .edit_material(id, MaterialEdit::Property(value))
            .unwrap();
    }
    let properties = &world.material(id).unwrap().properties;
    let held = properties.texture("_Render").unwrap();
    assert_eq!(held.texture, TextureRef::Render(render));
    assert_eq!((held.scale, held.offset), ([2.0, 3.0], [0.25, 0.5]));
    assert_eq!(
        properties.texture("_Asset").unwrap().texture,
        TextureRef::Asset(1 << 40)
    );
}

#[test]
fn a_texture_property_rejects_asset_zero_and_a_stale_render_texture() {
    let mut world = world();
    let (_, id) = material_with_shader(&mut world);
    let render = world.create_render_texture(target()).unwrap();
    world.destroy_render_texture(render).unwrap();
    for texture in [TextureRef::Asset(0), TextureRef::Render(render)] {
        let slot = TextureSlot {
            texture,
            ..TextureSlot::default()
        };
        let value = PropertyValue::Texture("_T".into(), slot);
        assert!(
            world
                .edit_material(id, MaterialEdit::Property(value))
                .is_err()
        );
    }
}

#[test]
fn names_hold_one_to_256_bytes_and_a_set_holds_1024_values_of_a_kind() {
    let mut world = world();
    let (_, id) = material_with_shader(&mut world);
    for name in [String::new(), "x".repeat(257)] {
        let value = PropertyValue::Float(name, 1.0);
        let rejected = world.edit_material(id, MaterialEdit::Property(value));
        assert_eq!(graphics_error(rejected), GraphicsError::InvalidName);
    }
    let longest = PropertyValue::Float("x".repeat(256), 1.0);
    world
        .edit_material(id, MaterialEdit::Property(longest))
        .unwrap();
    for index in 0..1023 {
        let value = PropertyValue::Float(format!("f{index}"), 1.0);
        world
            .edit_material(id, MaterialEdit::Property(value))
            .unwrap();
    }
    let one_more = PropertyValue::Float("overflow".into(), 1.0);
    let rejected = world.edit_material(id, MaterialEdit::Property(one_more));
    assert_eq!(graphics_error(rejected), GraphicsError::TooManyProperties);
    let replacing = PropertyValue::Float("f0".into(), 7.0);
    world
        .edit_material(id, MaterialEdit::Property(replacing))
        .unwrap();
}

#[test]
fn a_destroyed_material_is_stale_and_edits_to_it_fail() {
    let mut world = world();
    let (_, id) = material_with_shader(&mut world);
    world.destroy_material(id).unwrap();
    assert_eq!(
        graphics_error(world.material(id).map(drop)),
        GraphicsError::StaleMaterial
    );
    assert_eq!(
        graphics_error(world.edit_material(id, MaterialEdit::RenderQueue(1))),
        GraphicsError::StaleMaterial
    );
    assert_eq!(world.materials().count(), 0);
}

#[test]
fn destroying_a_shader_a_material_uses_is_allowed_and_leaves_the_material_stale_on_it() {
    let mut world = world();
    let (shader, id) = material_with_shader(&mut world);
    world.destroy_shader(shader).unwrap();
    assert_eq!(world.material(id).unwrap().shader, shader);
    assert!(world.shader(shader).is_err());
    assert_eq!(
        graphics_error(world.edit_material(id, MaterialEdit::Shader(shader))),
        GraphicsError::StaleShader
    );
}

#[test]
fn global_properties_and_keywords_are_stored_and_raise_the_generation() {
    let mut world = world();
    assert_eq!(world.global_shader_state().generation, 0);
    world
        .set_global_property(PropertyValue::Float("_Time".into(), 2.0))
        .unwrap();
    world
        .set_global_property(PropertyValue::Vector("_Fog".into(), [0.1; 4]))
        .unwrap();
    for name in ["WATER", "FOG"] {
        world.set_global_keyword(name, true).unwrap();
    }
    world.set_global_keyword("WATER", false).unwrap();
    let state = world.global_shader_state();
    assert_eq!(state.properties.float("_Time"), Some(2.0));
    assert_eq!(state.properties.vector("_Fog"), Some([0.1; 4]));
    assert_eq!(state.keywords, ["FOG"]);
    assert_eq!(state.generation, 5);
}

#[test]
fn a_rejected_global_change_leaves_the_state_and_generation() {
    let mut world = world();
    let rejected = world.set_global_property(PropertyValue::Int(String::new(), 1));
    assert_eq!(graphics_error(rejected), GraphicsError::InvalidName);
    assert_eq!(
        graphics_error(world.set_global_keyword("", true)),
        GraphicsError::InvalidName
    );
    assert_eq!(world.global_shader_state().generation, 0);
}

#[test]
fn a_render_texture_keeps_its_description_and_starts_at_generation_one() {
    let mut world = world();
    let id = world.create_render_texture(target()).unwrap();
    let held = world.render_texture(id).unwrap();
    assert_eq!((held.width, held.height, held.format), (64, 32, 8));
    assert_eq!((held.depth_bits, held.mip_count, held.srgb), (24, 3, true));
    assert_eq!(held.generation, 1);
}

#[test]
fn updating_a_render_texture_raises_its_generation_by_one() {
    let mut world = world();
    let id = world.create_render_texture(target()).unwrap();
    let resized = WorldRenderTexture {
        width: 128,
        ..target()
    };
    world.update_render_texture(id, resized).unwrap();
    world.update_render_texture(id, target()).unwrap();
    let held = world.render_texture(id).unwrap();
    assert_eq!((held.width, held.generation), (64, 3));
}

fn mirrored() -> WorldSampler {
    WorldSampler {
        wrap_u: 2,
        wrap_v: 1,
        wrap_w: 3,
        filter: 2,
        aniso_level: 4,
        mip_bias: 0.25,
    }
}

#[test]
fn a_render_texture_starts_with_the_engine_default_sampler() {
    let mut world = world();
    let id = world.create_render_texture(target()).unwrap();
    let held = world.render_texture(id).unwrap().sampler;
    assert_eq!((held.wrap_u, held.wrap_v, held.wrap_w), (0, 0, 0));
    assert_eq!((held.filter, held.aniso_level), (1, 1));
    assert_eq!(held.mip_bias.to_bits(), 0.0f32.to_bits());
}

#[test]
fn changing_the_sampler_of_a_render_texture_raises_its_generation() {
    let mut world = world();
    let id = world.create_render_texture(target()).unwrap();
    let sampled = WorldRenderTexture {
        sampler: mirrored(),
        ..target()
    };
    world.update_render_texture(id, sampled).unwrap();
    let held = world.render_texture(id).unwrap();
    assert_eq!(held.sampler, mirrored());
    assert_eq!(held.generation, 2);
}

#[test]
fn a_render_texture_sampler_outside_the_engine_values_is_rejected() {
    let mut world = world();
    let id = world.create_render_texture(target()).unwrap();
    let bad = [
        WorldSampler {
            wrap_u: 4,
            ..mirrored()
        },
        WorldSampler {
            wrap_v: 4,
            ..mirrored()
        },
        WorldSampler {
            wrap_w: 4,
            ..mirrored()
        },
        WorldSampler {
            filter: 3,
            ..mirrored()
        },
        WorldSampler {
            aniso_level: 17,
            ..mirrored()
        },
        WorldSampler {
            mip_bias: f32::NAN,
            ..mirrored()
        },
    ];
    for sampler in bad {
        let texture = WorldRenderTexture {
            sampler,
            ..target()
        };
        assert_eq!(
            graphics_error(world.create_render_texture(texture).map(drop)),
            GraphicsError::InvalidRenderTexture,
            "{sampler:?}"
        );
        assert_eq!(
            graphics_error(world.update_render_texture(id, texture)),
            GraphicsError::InvalidRenderTexture
        );
    }
    assert_eq!(world.render_texture(id).unwrap().generation, 1);
}

#[test]
fn a_render_texture_description_is_validated() {
    let mut world = world();
    let bad = [
        WorldRenderTexture {
            width: 0,
            ..target()
        },
        WorldRenderTexture {
            height: 16385,
            ..target()
        },
        WorldRenderTexture {
            depth_bits: 8,
            ..target()
        },
        WorldRenderTexture {
            mip_count: 0,
            ..target()
        },
        WorldRenderTexture {
            mip_count: 16,
            ..target()
        },
    ];
    for texture in bad {
        assert_eq!(
            graphics_error(world.create_render_texture(texture).map(drop)),
            GraphicsError::InvalidRenderTexture
        );
    }
    let edge = WorldRenderTexture {
        width: 16384,
        height: 1,
        depth_bits: 0,
        mip_count: 15,
        ..target()
    };
    assert!(world.create_render_texture(edge).is_ok());
}

#[test]
fn a_destroyed_render_texture_is_stale_and_reads_as_no_texture() {
    let mut world = world();
    let id = world.create_render_texture(target()).unwrap();
    assert_eq!(
        world.live_texture(TextureRef::Render(id)),
        TextureRef::Render(id)
    );
    world.destroy_render_texture(id).unwrap();
    assert_eq!(
        graphics_error(world.render_texture(id).map(drop)),
        GraphicsError::StaleRenderTexture
    );
    assert_eq!(world.live_texture(TextureRef::Render(id)), TextureRef::None);
    assert_eq!(
        world.live_texture(TextureRef::Asset(1 << 40)),
        TextureRef::Asset(1 << 40)
    );
    assert_eq!(
        graphics_error(world.update_render_texture(id, target())),
        GraphicsError::StaleRenderTexture
    );
    assert_eq!(RenderTextureId::from_bits(id.to_bits()), id);
}

#[test]
fn listing_resources_reports_each_live_one_with_its_id() {
    let mut world = world();
    let a = world.create_shader(shader("A")).unwrap();
    let b = world.create_shader(shader("B")).unwrap();
    world.destroy_shader(a).unwrap();
    let ids: Vec<_> = world
        .shaders()
        .map(|(id, shader)| (id, shader.name.clone()))
        .collect();
    assert_eq!(ids, [(b, "B".to_owned())]);
    let texture = world.create_render_texture(target()).unwrap();
    assert_eq!(
        world
            .render_textures()
            .map(|(id, _)| id)
            .collect::<Vec<_>>(),
        [texture]
    );
}

#[test]
fn clearing_properties_removes_values_but_not_keywords() {
    let mut world = world();
    let (_, id) = material_with_shader(&mut world);
    let value = PropertyValue::Float("_Cutoff".into(), 0.5);
    world
        .edit_material(id, MaterialEdit::Property(value.clone()))
        .unwrap();
    let keyword = MaterialEdit::Keyword {
        name: "FOG".into(),
        enabled: true,
    };
    world.edit_material(id, keyword).unwrap();
    world
        .edit_material(id, MaterialEdit::ClearProperties)
        .unwrap();
    let material = world.material(id).unwrap();
    assert!(material.properties.is_empty());
    assert_eq!(material.keywords, ["FOG"]);
    assert_eq!(material.generation, 4);

    world.set_global_property(value).unwrap();
    world.clear_global_properties();
    assert!(world.global_shader_state().properties.is_empty());
    assert_eq!(world.global_shader_state().generation, 2);
}

#[test]
fn a_material_holds_at_most_256_keywords() {
    let mut world = world();
    let (_, id) = material_with_shader(&mut world);
    let enable = |name: String| MaterialEdit::Keyword {
        name,
        enabled: true,
    };
    for number in 0..256 {
        world
            .edit_material(id, enable(format!("K{number}")))
            .unwrap();
    }
    let error = world.edit_material(id, enable("ONE_MORE".into()));
    assert_eq!(
        error,
        Err(WorldError::Graphics(GraphicsError::TooManyKeywords))
    );
    world.edit_material(id, enable("K0".into())).unwrap();
    assert_eq!(world.material(id).unwrap().keywords.len(), 256);
}
