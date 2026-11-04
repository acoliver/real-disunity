//! A world material's values as a property layer.

use std::collections::{BTreeMap, BTreeSet};

use worldrunner_render_program::ids::TextureId;
use worldrunner_render_program::properties::PropertyValue;
use worldrunner_world_simulation::graphics::{
    PropertyValue as WorldValue, RenderTextureId, ShaderProperties, TextureRef, TextureSlot,
    WorldRenderTexture,
};

use super::super::properties::{Textures, layer};
use super::support::{render_texture, texture_slot, world};

fn properties(values: Vec<WorldValue>) -> ShaderProperties {
    let mut properties = ShaderProperties::default();
    for value in values {
        properties.set(value).unwrap();
    }
    properties
}

fn no_textures() -> (BTreeMap<RenderTextureId, WorldRenderTexture>, BTreeSet<u64>) {
    (BTreeMap::new(), BTreeSet::new())
}

#[test]
fn each_kind_of_value_keeps_its_name_and_its_numbers() {
    let matrix: [f32; 16] = std::array::from_fn(|index| index as f32);
    let found = properties(vec![
        WorldValue::Float("_Cutoff".into(), 0.5),
        WorldValue::Int("_Mode".into(), 3),
        WorldValue::Vector("_Tint".into(), [1.0, 0.5, 0.25, 1.0]),
        WorldValue::Matrix("_Map".into(), matrix),
    ]);
    let (render, stored) = no_textures();
    let layer = layer(&found, &mut Textures::new(&render, &stored));
    assert_eq!(
        layer.values.get("_Cutoff"),
        Some(&PropertyValue::Float(0.5))
    );
    assert_eq!(layer.values.get("_Mode"), Some(&PropertyValue::Int(3)));
    assert_eq!(
        layer.values.get("_Tint"),
        Some(&PropertyValue::Vector([1.0, 0.5, 0.25, 1.0]))
    );
    assert_eq!(
        layer.values.get("_Map"),
        Some(&PropertyValue::Matrix(matrix)),
        "sixteen floats in the order the world stores them"
    );
}

#[test]
fn a_name_that_two_kinds_hold_takes_the_later_kind() {
    let found = properties(vec![
        WorldValue::Float("_Both".into(), 1.0),
        WorldValue::Vector("_Both".into(), [2.0; 4]),
    ]);
    let (render, stored) = no_textures();
    let layer = layer(&found, &mut Textures::new(&render, &stored));
    assert_eq!(
        layer.values.get("_Both"),
        Some(&PropertyValue::Vector([2.0; 4]))
    );
}

#[test]
fn a_texture_slot_names_a_render_texture_a_stored_texture_or_nothing() {
    let mut world = world();
    let live = render_texture(&mut world, [4, 4]);
    let gone = render_texture(&mut world, [4, 4]);
    let render: BTreeMap<_, _> = world.render_textures().map(|(id, t)| (id, *t)).collect();
    world.destroy_render_texture(gone).unwrap();
    let render: BTreeMap<_, _> = render.into_iter().filter(|(id, _)| *id != gone).collect();
    let stored: BTreeSet<u64> = BTreeSet::from([1 << 40]);
    let found = properties(vec![
        WorldValue::Texture("_Render".into(), texture_slot(TextureRef::Render(live))),
        WorldValue::Texture("_Stored".into(), texture_slot(TextureRef::Asset(1 << 40))),
        WorldValue::Texture("_Unread".into(), texture_slot(TextureRef::Asset(1 << 41))),
        WorldValue::Texture("_Gone".into(), texture_slot(TextureRef::Render(gone))),
        WorldValue::Texture("_None".into(), TextureSlot::default()),
    ]);
    let mut textures = Textures::new(&render, &stored);
    let layer = layer(&found, &mut textures);
    let names: Vec<_> = layer.textures.keys().map(String::as_str).collect();
    assert_eq!(names, ["_Render", "_Stored"]);
    assert_eq!(layer.textures["_Stored"].texture, TextureId(1 << 40));
    assert_ne!(
        layer.textures["_Render"].texture.0 >> 63,
        0,
        "a render texture has the top bit"
    );
    assert_eq!(layer.textures["_Render"].sampler, None);
    assert_eq!(textures.used().iter().copied().collect::<Vec<_>>(), [live]);
}

#[test]
fn a_texture_scale_and_offset_add_no_value() {
    let slot = TextureSlot {
        texture: TextureRef::Asset(5),
        scale: [2.0, 3.0],
        offset: [0.25, 0.5],
    };
    let found = properties(vec![WorldValue::Texture("_Main".into(), slot)]);
    let (render, _) = no_textures();
    let stored = BTreeSet::from([5]);
    let layer = layer(&found, &mut Textures::new(&render, &stored));
    assert!(layer.values.get("_Main_ST").is_none());
    assert!(layer.values.get("_Main").is_none());
}
