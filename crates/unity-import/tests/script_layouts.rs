use worldrunner_unity_import::script_layouts::{LayoutError, parse_script_layouts};
use worldrunner_unity_serialized::{
    script_builtin::Builtin,
    script_data::{FieldType, Primitive},
};

const LAYOUTS: &str = r#"{
  "scripts": ["Game:Spawner"],
  "types": {
    "Game:Spawner": {"value_type": false, "fields": [
      {"name": "count", "type": {"primitive": "unsigned int"}},
      {"name": "prefab", "type": {"object": "UnityEngine.GameObject"}},
      {"name": "points", "type": {"array": {"composite": "Game:Point"}}},
      {"name": "curve", "type": {"builtin": "UnityEngine.AnimationCurve"}},
      {"name": "shape", "type": {"reference": "IShape"}}
    ]},
    "Game:Point": {"value_type": true, "fields": [
      {"name": "x", "type": {"primitive": "float"}}
    ]}
  }
}"#;

#[test]
fn reads_layouts_and_resolves_composites_by_key() {
    let layouts = parse_script_layouts(LAYOUTS).unwrap();
    let spawner = layouts.find("Game:Spawner").unwrap();
    let point = layouts.find("Game:Point").unwrap();
    assert!(layouts.types[point].value_type);
    let fields: Vec<_> = layouts.types[spawner]
        .fields
        .iter()
        .map(|f| f.ty.clone())
        .collect();
    assert_eq!(
        fields,
        [
            FieldType::Primitive(Primitive::U32),
            FieldType::Object("UnityEngine.GameObject".to_owned()),
            FieldType::Array(Box::new(FieldType::Composite(point))),
            FieldType::Builtin(Builtin::AnimationCurve),
            FieldType::Reference("IShape".to_owned()),
        ]
    );
    assert!(layouts.has_references(spawner));
    assert!(!layouts.has_references(point));
}

#[test]
fn names_the_field_a_layout_cannot_describe() {
    for (replace, with, reason) in [
        ("\"unsigned int\"", "\"uint\"", "unknown primitive uint"),
        (
            "\"Game:Point\"}}",
            "\"Game:Missing\"}}",
            "composite Game:Missing is not listed",
        ),
        (
            "UnityEngine.AnimationCurve",
            "UnityEngine.Mesh",
            "unknown builtin UnityEngine.Mesh",
        ),
    ] {
        let json = LAYOUTS.replacen(replace, with, 1);
        match parse_script_layouts(&json) {
            Err(LayoutError::Field {
                key, reason: got, ..
            }) => {
                assert_eq!(key, "Game:Spawner");
                assert_eq!(got, reason);
            }
            other => panic!("{reason}: got {other:?}"),
        }
    }
    assert!(matches!(
        parse_script_layouts("{"),
        Err(LayoutError::Json(_))
    ));
}
