//! The values the adapter derives for the properties a material does not set: a texture's
//! `<name>_ST` from the scale and offset the material holds, and the defaults its shader
//! declares for numbers and vectors. Every expected value is one the test wrote.

use std::collections::{BTreeMap, BTreeSet};

use worldrunner_render_program::properties::PropertyValue;
use worldrunner_render_program::unity::synthetic::shaders::camera_passes;
use worldrunner_unity_serialized::shader::{Shader, ShaderProperty};
use worldrunner_world_simulation::graphics::{
    PropertyValue as WorldValue, RenderTextureId, ShaderProperties, TextureRef, TextureSlot,
    WorldRenderTexture,
};

use super::super::defaults::{ShaderFill, declared_fill};
use super::super::frame::{Settings, ShaderInfo, adapt};
use super::super::properties::{
    RULE_DEFAULT_FLOAT, RULE_DEFAULT_INT, RULE_DEFAULT_SCALE_OFFSET, RULE_DEFAULT_VECTOR,
    RULE_SLOT_SCALE_OFFSET, Textures, fallback_layer,
};
use super::gpu::target;
use super::support::{available, material, set, shader, snapshot, world};
use worldrunner_render_program::ids::MaterialId;
use worldrunner_render_program::unity::offscreen::frame::Record;
use worldrunner_world_simulation::graphics::OffscreenWork;

fn property(name: &str, kind: i32, default: [f32; 4], dimension: i32) -> ShaderProperty {
    ShaderProperty {
        name: name.to_owned(),
        description: String::new(),
        attributes: Vec::new(),
        kind,
        flags: 0,
        default_value: default,
        default_texture: String::new(),
        texture_dimension: dimension,
    }
}

fn shader_with(properties: Vec<ShaderProperty>) -> Shader {
    let mut shader = camera_passes().shader;
    shader.parsed.properties = properties;
    shader
}

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

fn slot(scale: [f32; 2], offset: [f32; 2]) -> TextureSlot {
    TextureSlot {
        texture: TextureRef::Asset(5),
        scale,
        offset,
    }
}

#[test]
fn a_shader_declares_a_default_for_each_number_vector_colour_and_integer_property() {
    let shader = shader_with(vec![
        property("_Intensity", 2, [0.75, 0.0, 0.0, 0.0], 0),
        property("_Strength", 3, [0.5, 0.0, 1.0, 0.0], 0),
        property("_Tint", 0, [0.25, 0.5, 0.75, 1.0], 0),
        property("_Place", 1, [1.0, 2.0, 3.0, 4.0], 0),
        property("_Mode", 5, [3.0, 0.0, 0.0, 0.0], 0),
        property("_Main", 4, [0.0; 4], 2),
        property("_Cube", 4, [0.0; 4], 4),
    ]);
    let fill = declared_fill(&shader);
    assert_eq!(
        fill.values,
        vec![
            ("_Intensity".to_owned(), PropertyValue::Float(0.75)),
            // A range's first float is its default; the rest are its limits.
            ("_Strength".to_owned(), PropertyValue::Float(0.5)),
            (
                "_Tint".to_owned(),
                PropertyValue::Vector([0.25, 0.5, 0.75, 1.0])
            ),
            (
                "_Place".to_owned(),
                PropertyValue::Vector([1.0, 2.0, 3.0, 4.0])
            ),
            ("_Mode".to_owned(), PropertyValue::Int(3)),
        ]
    );
    assert_eq!(fill.textures, vec!["_Main".to_owned()], "2D textures only");
}

#[test]
fn a_texture_the_material_lists_gives_its_scale_and_offset_as_scale_then_offset() {
    let found = properties(vec![WorldValue::Texture(
        "_Heightmap".into(),
        slot([2.0, 3.0], [0.25, 0.5]),
    )]);
    let (render, stored) = no_textures();
    let mut textures = Textures::new(&render, &stored);
    let layer = fallback_layer(&found, &ShaderFill::default(), &mut textures);
    assert_eq!(
        layer.values.get("_Heightmap_ST"),
        Some(&PropertyValue::Vector([2.0, 3.0, 0.25, 0.5]))
    );
    assert_eq!(
        textures.filled().get(RULE_SLOT_SCALE_OFFSET),
        Some(&1),
        "{:?}",
        textures.filled()
    );
}

#[test]
fn a_slot_with_no_texture_still_gives_its_scale_and_offset() {
    let mut empty = slot([4.0, 5.0], [6.0, 7.0]);
    empty.texture = TextureRef::None;
    let found = properties(vec![WorldValue::Texture("_Alphamap".into(), empty)]);
    let (render, stored) = no_textures();
    let layer = fallback_layer(
        &found,
        &ShaderFill::default(),
        &mut Textures::new(&render, &stored),
    );
    assert_eq!(
        layer.values.get("_Alphamap_ST"),
        Some(&PropertyValue::Vector([4.0, 5.0, 6.0, 7.0]))
    );
}

#[test]
fn a_texture_the_shader_declares_and_the_material_lists_no_slot_for_gets_scale_one_and_offset_zero()
{
    let fill = ShaderFill {
        textures: vec!["_Main".to_owned(), "_Listed".to_owned()],
        values: Vec::new(),
    };
    let found = properties(vec![WorldValue::Texture(
        "_Listed".into(),
        slot([2.0, 2.0], [0.5, 0.5]),
    )]);
    let (render, stored) = no_textures();
    let mut textures = Textures::new(&render, &stored);
    let layer = fallback_layer(&found, &fill, &mut textures);
    assert_eq!(
        layer.values.get("_Main_ST"),
        Some(&PropertyValue::Vector([1.0, 1.0, 0.0, 0.0]))
    );
    assert_eq!(
        layer.values.get("_Listed_ST"),
        Some(&PropertyValue::Vector([2.0, 2.0, 0.5, 0.5])),
        "a listed slot keeps its own, not the default"
    );
    assert_eq!(textures.filled().get(RULE_DEFAULT_SCALE_OFFSET), Some(&1));
    assert_eq!(textures.filled().get(RULE_SLOT_SCALE_OFFSET), Some(&1));
}

#[test]
fn a_scale_and_offset_vector_the_material_sets_itself_is_left_to_it() {
    let found = properties(vec![
        WorldValue::Texture("_Main".into(), slot([2.0, 2.0], [0.0, 0.0])),
        WorldValue::Vector("_Main_ST".into(), [9.0, 9.0, 9.0, 9.0]),
    ]);
    let (render, stored) = no_textures();
    let mut textures = Textures::new(&render, &stored);
    let layer = fallback_layer(&found, &ShaderFill::default(), &mut textures);
    assert!(layer.values.get("_Main_ST").is_none());
    assert!(textures.filled().is_empty());
}

#[test]
fn the_shader_defaults_fill_what_the_material_leaves_unset_and_never_what_it_sets() {
    let fill = ShaderFill {
        textures: Vec::new(),
        values: vec![
            ("_Intensity".to_owned(), PropertyValue::Float(0.75)),
            ("_Cutoff".to_owned(), PropertyValue::Float(0.5)),
            ("_Tint".to_owned(), PropertyValue::Vector([0.25; 4])),
            ("_Mode".to_owned(), PropertyValue::Int(3)),
            // The material sets this one as another kind of value.
            ("_Both".to_owned(), PropertyValue::Float(1.0)),
        ],
    };
    let found = properties(vec![
        // Zero is a value the material set: the default does not replace it.
        WorldValue::Float("_Cutoff".into(), 0.0),
        WorldValue::Vector("_Both".into(), [2.0; 4]),
    ]);
    let (render, stored) = no_textures();
    let mut textures = Textures::new(&render, &stored);
    let layer = fallback_layer(&found, &fill, &mut textures);
    assert_eq!(
        layer.values.get("_Intensity"),
        Some(&PropertyValue::Float(0.75))
    );
    assert_eq!(
        layer.values.get("_Tint"),
        Some(&PropertyValue::Vector([0.25; 4]))
    );
    assert_eq!(layer.values.get("_Mode"), Some(&PropertyValue::Int(3)));
    assert!(layer.values.get("_Cutoff").is_none());
    assert!(layer.values.get("_Both").is_none());
    let filled: Vec<_> = textures
        .filled()
        .iter()
        .map(|(rule, n)| (*rule, *n))
        .collect();
    let mut expected = vec![
        (RULE_DEFAULT_FLOAT, 1),
        (RULE_DEFAULT_INT, 1),
        (RULE_DEFAULT_VECTOR, 1),
    ];
    expected.sort_unstable();
    assert_eq!(filled, expected);
}

#[test]
fn a_frame_gives_each_material_a_fallback_layer_of_its_texture_scales_and_shader_defaults() {
    let mut world = world();
    let blit = shader(&mut world, "blit");
    let material_id = material(&mut world, blit);
    set(
        &mut world,
        material_id,
        WorldValue::Texture("_Heightmap".into(), slot([2.0, 3.0], [0.25, 0.5])),
    );
    set(
        &mut world,
        material_id,
        WorldValue::Float("_Cutoff".into(), 0.0),
    );
    let heights = target(&mut world, 8);
    world
        .record_offscreen(OffscreenWork::Blit {
            source: TextureRef::Render(heights),
            target: heights,
            material: Some(material_id),
            pass: 0,
        })
        .unwrap();
    let snapshot = snapshot(&mut world);
    let mut available = available(&snapshot, 1, &[5]);
    for info in available.shaders.values_mut() {
        *info = ShaderInfo {
            fill: ShaderFill {
                textures: vec!["_Heightmap".to_owned(), "_Alphamap".to_owned()],
                values: vec![
                    ("_Intensity".to_owned(), PropertyValue::Float(1.5)),
                    ("_Cutoff".to_owned(), PropertyValue::Float(0.5)),
                ],
            },
            ..info.clone()
        };
    }
    let settings = Settings::default();
    let adapted = adapt(&snapshot, &available, &settings);
    assert!(matches!(
        adapted.frame.records.as_slice(),
        [Record::Blit { .. }]
    ));
    let made = adapted
        .frame
        .materials
        .values()
        .next()
        .expect("the blit's material");
    let fallback = &made.fallback.values;
    assert_eq!(
        fallback.get("_Heightmap_ST"),
        Some(&PropertyValue::Vector([2.0, 3.0, 0.25, 0.5]))
    );
    assert_eq!(
        fallback.get("_Alphamap_ST"),
        Some(&PropertyValue::Vector([1.0, 1.0, 0.0, 0.0]))
    );
    assert_eq!(fallback.get("_Intensity"), Some(&PropertyValue::Float(1.5)));
    assert!(fallback.get("_Cutoff").is_none(), "the material set it");
    assert_eq!(
        made.properties.values.get("_Cutoff"),
        Some(&PropertyValue::Float(0.0))
    );
    let census = &adapted.report.census;
    for (rule, count) in [
        (RULE_SLOT_SCALE_OFFSET, 1),
        (RULE_DEFAULT_SCALE_OFFSET, 1),
        (RULE_DEFAULT_FLOAT, 1),
    ] {
        assert_eq!(
            census.get(&format!("constants filled by {rule}")),
            Some(&count),
            "{rule}: {census:?}"
        );
    }
    let _ = MaterialId(0);
}
