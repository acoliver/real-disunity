use worldrunner_unity_serialized::{
    SerializedError,
    object::PPtr,
    script_builtin::Builtin,
    script_data::{
        FieldType, MAX_CLASS_DEPTH, Primitive, REFERENCES, ScriptField, ScriptLayouts, ScriptType,
        Value, read_script,
    },
};

/// Little-endian object bytes, built field by field.
#[derive(Default)]
struct Bytes(Vec<u8>);

impl Bytes {
    fn u8(mut self, value: u8) -> Self {
        self.0.push(value);
        self
    }
    fn i32(mut self, value: i32) -> Self {
        self.0.extend(value.to_le_bytes());
        self
    }
    fn u32(mut self, value: u32) -> Self {
        self.0.extend(value.to_le_bytes());
        self
    }
    fn i64(mut self, value: i64) -> Self {
        self.0.extend(value.to_le_bytes());
        self
    }
    fn f32(mut self, value: f32) -> Self {
        self.0.extend(value.to_le_bytes());
        self
    }
    fn align(mut self) -> Self {
        while !self.0.len().is_multiple_of(4) {
            self.0.push(0);
        }
        self
    }
    fn string(self, text: &str) -> Self {
        let mut this = self.i32(i32::try_from(text.len()).unwrap());
        this.0.extend(text.as_bytes());
        this.align()
    }
}

fn field(name: &str, ty: FieldType) -> ScriptField {
    ScriptField {
        name: name.to_owned(),
        ty,
    }
}

fn class(key: &str, fields: Vec<ScriptField>) -> ScriptType {
    ScriptType {
        key: key.to_owned(),
        value_type: false,
        fields,
    }
}

const PRIM: fn(Primitive) -> FieldType = FieldType::Primitive;

/// Eight bytes of header stand in for the `MonoBehaviour` fields before the script's.
const HEADER: usize = 8;

fn read(bytes: Bytes, layouts: &ScriptLayouts) -> Result<Vec<(String, Value)>, SerializedError> {
    let mut object = vec![0xEE; HEADER];
    object.extend(bytes.0);
    read_script(&object, HEADER, false, layouts, 0)
}

fn names(fields: &[(String, Value)]) -> Vec<&str> {
    fields.iter().map(|(name, _)| name.as_str()).collect()
}

#[test]
fn aligns_after_narrow_fields_strings_and_arrays_but_not_array_elements() {
    let layouts = ScriptLayouts::new(vec![class(
        "Game:Script",
        vec![
            field("flag", PRIM(Primitive::Bool)),
            field("count", PRIM(Primitive::I32)),
            field("bytes", FieldType::Array(Box::new(PRIM(Primitive::U8)))),
            field("name", PRIM(Primitive::String)),
            field("small", PRIM(Primitive::I16)),
            field(
                "target",
                FieldType::Object("UnityEngine.GameObject".to_owned()),
            ),
        ],
    )]);
    let bytes = Bytes::default()
        .u8(1)
        .align()
        .i32(-5)
        .i32(3)
        .u8(7)
        .u8(8)
        .u8(9)
        .align()
        .string("ab")
        .u8(0xFE)
        .u8(0xFF)
        .align()
        .i32(2)
        .i64(42);
    let fields = read(bytes, &layouts).unwrap();
    assert_eq!(
        fields,
        [
            ("flag".to_owned(), Value::Bool(true)),
            ("count".to_owned(), Value::Int(-5)),
            (
                "bytes".to_owned(),
                Value::Array(vec![Value::UInt(7), Value::UInt(8), Value::UInt(9)])
            ),
            ("name".to_owned(), Value::String("ab".to_owned())),
            ("small".to_owned(), Value::Int(-2)),
            (
                "target".to_owned(),
                Value::Object(PPtr {
                    file_id: 2,
                    path_id: 42
                })
            ),
        ]
    );
}

#[test]
fn reads_builtins_with_their_native_layouts() {
    let layouts = ScriptLayouts::new(vec![class(
        "Game:Script",
        vec![
            field("position", FieldType::Builtin(Builtin::Vector3)),
            field("tint", FieldType::Builtin(Builtin::Color32)),
            field("curve", FieldType::Builtin(Builtin::AnimationCurve)),
            field("gradient", FieldType::Builtin(Builtin::Gradient)),
            field("after", PRIM(Primitive::I32)),
        ],
    )]);
    let mut bytes = Bytes::default()
        .f32(1.0)
        .f32(2.0)
        .f32(3.0)
        .u32(0xFF00_FF00)
        .i32(1);
    for value in [0.0, 1.0, 0.5, -0.5] {
        bytes = bytes.f32(value);
    }
    bytes = bytes.i32(0).f32(0.25).f32(0.75).i32(2).i32(2).i32(4);
    // A gradient: eight colours, sixteen key times, mode, colour space and two key counts.
    for _ in 0..32 {
        bytes = bytes.f32(0.5);
    }
    for _ in 0..8 {
        bytes = bytes.i32(0);
    }
    bytes = bytes.u8(1).u8(0xFF).u8(2).u8(3).i32(77);
    let fields = read(bytes, &layouts).unwrap();
    let Value::Struct(position) = &fields[0].1 else {
        panic!("a Vector3 is a struct")
    };
    assert_eq!(names(position), ["x", "y", "z"]);
    assert_eq!(position[2].1, Value::Float(3.0));
    let Value::Struct(curve) = &fields[2].1 else {
        panic!("a curve is a struct")
    };
    assert_eq!(
        names(curve),
        [
            "m_Curve",
            "m_PreInfinity",
            "m_PostInfinity",
            "m_RotationOrder"
        ]
    );
    let Value::Struct(gradient) = &fields[3].1 else {
        panic!("a gradient is a struct")
    };
    assert_eq!(gradient.last().unwrap().1, Value::UInt(3));
    assert_eq!(fields[4].1, Value::Int(77));
}

/// A class that holds itself: each level has a name and the next level.
fn chain() -> ScriptLayouts {
    ScriptLayouts::new(vec![class(
        "Game:Link",
        vec![
            field("name", PRIM(Primitive::String)),
            field("next", FieldType::Composite(0)),
        ],
    )])
}

#[test]
fn stops_nested_classes_at_unitys_depth_limit() {
    let mut bytes = Bytes::default();
    for level in 0..=MAX_CLASS_DEPTH {
        bytes = bytes.string(&level.to_string());
    }
    let mut fields = read(bytes, &chain()).unwrap();
    let mut levels = 1;
    while let Some((_, Value::Struct(next))) = fields.iter().find(|(name, _)| name == "next") {
        fields = next.clone();
        levels += 1;
    }
    assert_eq!(levels, MAX_CLASS_DEPTH + 1);
    assert_eq!(names(&fields), ["name"]);
}

#[test]
fn reads_the_reference_registry_after_the_fields() {
    let layouts = ScriptLayouts::new(vec![
        class(
            "Game:Script",
            vec![field(
                "shapes",
                FieldType::Array(Box::new(FieldType::Reference("IShape".to_owned()))),
            )],
        ),
        class(
            "Game:Shapes.Circle",
            vec![field("radius", PRIM(Primitive::F32))],
        ),
    ]);
    let bytes = Bytes::default()
        .i32(2)
        .i64(7)
        .i64(-2)
        .i32(2)
        .i32(2)
        .i64(7)
        .string("Circle")
        .string("Shapes")
        .string("Game")
        .f32(1.5)
        .i64(-2)
        .string("")
        .string("")
        .string("");
    let fields = read(bytes, &layouts).unwrap();
    assert_eq!(names(&fields), ["shapes", REFERENCES]);
    assert_eq!(
        fields[0].1,
        Value::Array(vec![Value::Reference(7), Value::Reference(-2)])
    );
    let Value::Array(entries) = &fields[1].1 else {
        panic!("the registry is an array")
    };
    let Value::Struct(circle) = &entries[0] else {
        panic!("an entry is a struct")
    };
    assert_eq!(
        circle.last().unwrap().1,
        Value::Struct(vec![("radius".to_owned(), Value::Float(1.5))])
    );
}

#[test]
fn rejects_data_the_layout_does_not_match() {
    let layouts = ScriptLayouts::new(vec![class(
        "Game:Script",
        vec![field("flag", PRIM(Primitive::Bool))],
    )]);
    assert!(matches!(
        read(Bytes::default().u8(1).align().i32(9), &layouts),
        Err(SerializedError::ScriptTrailingBytes {
            read: 4,
            size: 8,
            ..
        })
    ));
    assert!(matches!(
        read(Bytes::default().u8(2).align(), &layouts),
        Err(SerializedError::InvalidBool { value: 2, .. })
    ));
    assert!(matches!(
        read(Bytes::default().u8(1), &layouts),
        Err(SerializedError::Truncated { .. })
    ));
    let nested = ScriptLayouts::new(vec![class(
        "Game:Script",
        vec![field(
            "grid",
            FieldType::Array(Box::new(FieldType::Array(Box::new(PRIM(Primitive::I32))))),
        )],
    )]);
    assert!(matches!(
        read(Bytes::default().i32(0), &nested),
        Err(SerializedError::UnsupportedScriptField { .. })
    ));
}

#[test]
fn rejects_registries_it_cannot_read() {
    let layouts = ScriptLayouts::new(vec![class(
        "Game:Script",
        vec![field("shape", FieldType::Reference("IShape".to_owned()))],
    )]);
    let old = Bytes::default().i64(1).i32(1).i32(0);
    assert!(matches!(
        read(old, &layouts),
        Err(SerializedError::UnsupportedScriptField { field_type }) if field_type.contains("version 1")
    ));
    let unknown = Bytes::default()
        .i64(1)
        .i32(2)
        .i32(1)
        .i64(1)
        .string("Square")
        .string("")
        .string("Game");
    assert!(matches!(
        read(unknown, &layouts),
        Err(SerializedError::UnsupportedScriptField { field_type }) if field_type.contains("Game:Square")
    ));
}
