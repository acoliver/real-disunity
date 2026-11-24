//! How the frame names the textures a material, a global or a blit reads.

use std::collections::BTreeMap;
use worldrunner_render_program::ids::TextureId;
use worldrunner_render_program::sampler::{Filter, SamplerState, Wrap};
use worldrunner_render_program::unity::offscreen::frame::{Record, UndeclaredTextures};
use worldrunner_world_simulation::graphics::{
    PropertyValue as WorldValue, TextureRef, WorldRenderTexture, WorldSampler,
};

use super::super::defaults::DefaultTexture;
use super::super::frame::adapt;
use super::super::ids;
use super::frame::{blit, settings};
use super::support::{
    available, material, render_texture, set, shader, snapshot, texture_slot, world,
};

#[test]
fn a_stored_texture_the_host_can_read_is_named_by_its_identity() {
    let mut world = world();
    let (a, b) = (
        render_texture(&mut world, [8, 8]),
        render_texture(&mut world, [8, 8]),
    );
    let shader = shader(&mut world, "Blit");
    let material = material(&mut world, shader);
    set(
        &mut world,
        material,
        WorldValue::Texture("_Lookup".into(), texture_slot(TextureRef::Asset(1 << 40))),
    );
    blit(&mut world, TextureRef::Asset(1 << 40), b, Some(material), 0);
    blit(&mut world, TextureRef::Render(a), b, Some(material), 0);
    let snapshot = snapshot(&mut world);
    let adapted = adapt(&snapshot, &available(&snapshot, 1, &[1 << 40]), &settings());
    let input = adapted.frame.materials[&ids::material(material)]
        .properties
        .textures["_Lookup"];
    assert_eq!(input.texture, TextureId(1 << 40));
    let Record::Blit { source, .. } = &adapted.frame.records[0] else {
        panic!("a blit");
    };
    assert_eq!(*source, TextureId(1 << 40));
}

#[test]
fn a_texture_property_that_names_no_texture_the_frame_can_read_is_counted_by_name_and_reason() {
    let mut world = world();
    let target = render_texture(&mut world, [8, 8]);
    let shader = shader(&mut world, "Blit");
    let material = material(&mut world, shader);
    for (name, texture) in [
        ("_NotUploaded", TextureRef::Asset(1 << 40)),
        ("_Uploaded", TextureRef::Asset(1 << 41)),
        ("_Nothing", TextureRef::None),
    ] {
        set(
            &mut world,
            material,
            WorldValue::Texture(name.into(), texture_slot(texture)),
        );
    }
    blit(
        &mut world,
        TextureRef::Render(target),
        target,
        Some(material),
        0,
    );
    let snapshot = snapshot(&mut world);
    let adapted = adapt(&snapshot, &available(&snapshot, 1, &[1 << 41]), &settings());
    assert_eq!(
        adapted.report.unresolved_textures,
        BTreeMap::from([("_NotUploaded: asset not readable by the host".to_owned(), 1)])
    );
    let stand_ins = &adapted.frame.materials[&ids::material(material)]
        .properties
        .stand_ins;
    assert_eq!(
        stand_ins.get("_Nothing").map(String::as_str),
        Some("undeclared"),
        "a property that reads a stand-in is counted by the stand-in census and not as unresolved"
    );
}

/// A texture property that nothing sets and the shader does not declare reads the grey default
/// texture; one the shader declares reads its declared default, and one whose default
/// the host does not supply stays unset, as does any when the host registered no defaults.
#[test]
fn an_unset_texture_the_shader_does_not_declare_reads_grey() {
    let mut world = world();
    let target = render_texture(&mut world, [8, 8]);
    let shader = shader(&mut world, "Blit");
    let material = material(&mut world, shader);
    for name in ["_UserLut", "_Declared"] {
        set(
            &mut world,
            material,
            WorldValue::Texture(name.into(), texture_slot(TextureRef::None)),
        );
    }
    blit(
        &mut world,
        TextureRef::Render(target),
        target,
        Some(material),
        0,
    );
    let snapshot = snapshot(&mut world);
    let mut available = available(&snapshot, 1, &[]);
    for info in available.shaders.values_mut() {
        info.fill.declared_textures = vec!["_Declared".to_owned()];
    }
    let adapted = adapt(&snapshot, &available, &settings());
    let textures = &adapted.frame.materials[&ids::material(material)]
        .properties
        .textures;
    assert_eq!(textures["_UserLut"].texture, DefaultTexture::Grey.id());
    assert!(!textures.contains_key("_Declared"));
    assert_eq!(
        adapted.frame.undeclared_textures.d2,
        Some(DefaultTexture::Grey.id()),
        "records read the grey default for what no layer names"
    );
    assert_eq!(
        adapted.report.unresolved_textures,
        BTreeMap::from([("_Declared: no texture set".to_owned(), 1)])
    );
    assert_eq!(
        adapted.frame.materials[&ids::material(material)]
            .properties
            .stand_ins
            .get("_UserLut")
            .map(String::as_str),
        Some("undeclared")
    );
    available.default_textures.clear();
    let adapted = adapt(&snapshot, &available, &settings());
    assert_eq!(adapted.frame.undeclared_textures.d2, None);
    assert!(
        !adapted.frame.materials[&ids::material(material)]
            .properties
            .textures
            .contains_key("_UserLut")
    );
}

/// A cube or 3D slot nothing sets and no shader declares, as the URP blit program's
/// `_BlitCubeTexture` is, reads the grey cube or volume when the host registered it and stays
/// missing when it did not.
#[test]
fn the_frame_carries_the_grey_cube_and_volume_only_when_the_host_registered_them() {
    let mut world = world();
    let target = render_texture(&mut world, [8, 8]);
    blit(&mut world, TextureRef::Render(target), target, None, 0);
    let snapshot = snapshot(&mut world);
    let mut available = available(&snapshot, 1, &[]);
    let adapted = adapt(&snapshot, &available, &settings());
    assert_eq!(
        adapted.frame.undeclared_textures,
        UndeclaredTextures {
            d2: Some(DefaultTexture::Grey.id()),
            cube: Some(DefaultTexture::GreyCube.id()),
            d3: Some(DefaultTexture::Grey3d.id()),
        }
    );
    assert_ne!(DefaultTexture::Grey.id(), DefaultTexture::GreyCube.id());
    assert_ne!(DefaultTexture::Grey.id(), DefaultTexture::Grey3d.id());
    available.default_textures.remove(&DefaultTexture::Grey3d);
    let adapted = adapt(&snapshot, &available, &settings());
    assert_eq!(
        adapted.frame.undeclared_textures,
        UndeclaredTextures {
            d2: Some(DefaultTexture::Grey.id()),
            cube: Some(DefaultTexture::GreyCube.id()),
            d3: None,
        }
    );
    available.default_textures.remove(&DefaultTexture::GreyCube);
    let adapted = adapt(&snapshot, &available, &settings());
    assert_eq!(
        adapted.frame.undeclared_textures,
        UndeclaredTextures {
            d2: Some(DefaultTexture::Grey.id()),
            cube: None,
            d3: None,
        }
    );
    available.default_textures.clear();
    let adapted = adapt(&snapshot, &available, &settings());
    assert_eq!(
        adapted.frame.undeclared_textures,
        UndeclaredTextures::default()
    );
}

#[test]
fn a_render_texture_named_by_a_property_is_read_with_its_own_sampler() {
    let mut world = world();
    let description = WorldRenderTexture {
        width: 8,
        height: 8,
        format: 8,
        depth_bits: 0,
        mip_count: 1,
        srgb: false,
        dimension: 2,
        volume_depth: 1,
        auto_generate_mips: false,
        sampler: WorldSampler {
            wrap_u: 1,
            wrap_v: 2,
            wrap_w: 0,
            filter: 0,
            aniso_level: 1,
            mip_bias: 0.0,
        },
        generation: 0,
    };
    let (a, b) = (
        world.create_render_texture(description).unwrap(),
        render_texture(&mut world, [8, 8]),
    );
    world
        .set_global_property(WorldValue::Texture(
            "_Lookup".into(),
            texture_slot(TextureRef::Render(a)),
        ))
        .unwrap();
    // An earlier take ends with the value in the state the frame starts from.
    snapshot(&mut world);
    blit(&mut world, TextureRef::Render(a), b, None, 0);
    let snapshot = snapshot(&mut world);
    let adapted = adapt(&snapshot, &available(&snapshot, 1, &[]), &settings());
    assert_eq!(
        adapted.frame.globals.textures["_Lookup"].sampler,
        Some(SamplerState {
            wrap_w: Wrap::Repeat,
            ..SamplerState::new(Wrap::Clamp, Wrap::Mirror, Filter::Point)
        })
    );
}
