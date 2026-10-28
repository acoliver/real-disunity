//! The native layouts of Unity values that scripts serialize, such as `Vector3`,
//! `AnimationCurve` and `GUIStyle`. Their managed types wrap native data, so their layouts
//! come from the engine, not from the script's assembly.

use crate::{SerializedError, object::ObjectReader, script_data::Value};

/// A Unity value with a native layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Builtin {
    Vector2,
    Vector3,
    Vector4,
    Vector2Int,
    Vector3Int,
    Quaternion,
    Matrix4x4,
    Color,
    Color32,
    Rect,
    RectInt,
    Bounds,
    BoundsInt,
    LayerMask,
    RenderingLayerMask,
    AnimationCurve,
    Gradient,
    RectOffset,
    GuiStyle,
    Hash128,
    /// Part of a `GUIStyle`; scripts do not hold one directly.
    GuiStyleState,
}

impl Builtin {
    /// The builtin with this full managed name, such as `UnityEngine.Vector3`.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name.strip_prefix("UnityEngine.")? {
            "Vector2" => Self::Vector2,
            "Vector3" => Self::Vector3,
            "Vector4" => Self::Vector4,
            "Vector2Int" => Self::Vector2Int,
            "Vector3Int" => Self::Vector3Int,
            "Quaternion" => Self::Quaternion,
            "Matrix4x4" => Self::Matrix4x4,
            "Color" => Self::Color,
            "Color32" => Self::Color32,
            "Rect" => Self::Rect,
            "RectInt" => Self::RectInt,
            "Bounds" => Self::Bounds,
            "BoundsInt" => Self::BoundsInt,
            "LayerMask" => Self::LayerMask,
            "RenderingLayerMask" => Self::RenderingLayerMask,
            "AnimationCurve" => Self::AnimationCurve,
            "Gradient" => Self::Gradient,
            "RectOffset" => Self::RectOffset,
            "GUIStyle" => Self::GuiStyle,
            "Hash128" => Self::Hash128,
            _ => return None,
        })
    }

    fn layout(self) -> &'static [Native] {
        match self {
            Self::Vector2 => VECTOR2,
            Self::Vector3 => VECTOR3,
            Self::Vector4 | Self::Quaternion => VECTOR4,
            Self::Vector2Int => VECTOR2_INT,
            Self::Vector3Int => VECTOR3_INT,
            Self::Matrix4x4 => MATRIX4X4,
            Self::Color => COLOR,
            Self::Color32 => &[Native::U32("rgba")],
            Self::Rect => RECT,
            Self::RectInt => RECT_INT,
            Self::Bounds => BOUNDS,
            Self::BoundsInt => BOUNDS_INT,
            Self::LayerMask | Self::RenderingLayerMask => &[Native::U32("m_Bits")],
            Self::AnimationCurve => ANIMATION_CURVE,
            Self::Gradient => GRADIENT,
            Self::RectOffset => RECT_OFFSET,
            Self::GuiStyle => GUI_STYLE,
            Self::Hash128 => HASH128,
            Self::GuiStyleState => GUI_STYLE_STATE,
        }
    }
}

/// One native field.
enum Native {
    F32(&'static str),
    I32(&'static str),
    U32(&'static str),
    U16(&'static str),
    U8(&'static str),
    I8(&'static str),
    Bool(&'static str),
    String(&'static str),
    Object(&'static str),
    Nested(&'static str, Builtin),
    /// An `AnimationCurve`'s keys.
    Keyframes(&'static str),
    Align,
}

use Native::{Align, Bool, F32, I8, I32, Nested, U8, U16};

const VECTOR2: &[Native] = &[F32("x"), F32("y")];
const VECTOR3: &[Native] = &[F32("x"), F32("y"), F32("z")];
const VECTOR4: &[Native] = &[F32("x"), F32("y"), F32("z"), F32("w")];
const VECTOR2_INT: &[Native] = &[I32("x"), I32("y")];
const VECTOR3_INT: &[Native] = &[I32("x"), I32("y"), I32("z")];
const COLOR: &[Native] = &[F32("r"), F32("g"), F32("b"), F32("a")];
const RECT: &[Native] = &[F32("x"), F32("y"), F32("width"), F32("height")];
const RECT_INT: &[Native] = &[I32("x"), I32("y"), I32("width"), I32("height")];
const BOUNDS: &[Native] = &[
    Nested("m_Center", Builtin::Vector3),
    Nested("m_Extent", Builtin::Vector3),
];
const BOUNDS_INT: &[Native] = &[
    Nested("m_Position", Builtin::Vector3Int),
    Nested("m_Size", Builtin::Vector3Int),
];
const RECT_OFFSET: &[Native] = &[I32("m_Left"), I32("m_Right"), I32("m_Top"), I32("m_Bottom")];
/// Column by column, as Unity stores it.
#[rustfmt::skip]
const MATRIX4X4: &[Native] = &[
    F32("m00"), F32("m10"), F32("m20"), F32("m30"),
    F32("m01"), F32("m11"), F32("m21"), F32("m31"),
    F32("m02"), F32("m12"), F32("m22"), F32("m32"),
    F32("m03"), F32("m13"), F32("m23"), F32("m33"),
];
#[rustfmt::skip]
const HASH128: &[Native] = &[
    U8("bytes[0]"), U8("bytes[1]"), U8("bytes[2]"), U8("bytes[3]"),
    U8("bytes[4]"), U8("bytes[5]"), U8("bytes[6]"), U8("bytes[7]"),
    U8("bytes[8]"), U8("bytes[9]"), U8("bytes[10]"), U8("bytes[11]"),
    U8("bytes[12]"), U8("bytes[13]"), U8("bytes[14]"), U8("bytes[15]"),
];
const KEYFRAME: &[Native] = &[
    F32("time"),
    F32("value"),
    F32("inSlope"),
    F32("outSlope"),
    I32("weightedMode"),
    F32("inWeight"),
    F32("outWeight"),
];
const ANIMATION_CURVE: &[Native] = &[
    Native::Keyframes("m_Curve"),
    I32("m_PreInfinity"),
    I32("m_PostInfinity"),
    I32("m_RotationOrder"),
];
#[rustfmt::skip]
const GRADIENT: &[Native] = &[
    Nested("key0", Builtin::Color), Nested("key1", Builtin::Color),
    Nested("key2", Builtin::Color), Nested("key3", Builtin::Color),
    Nested("key4", Builtin::Color), Nested("key5", Builtin::Color),
    Nested("key6", Builtin::Color), Nested("key7", Builtin::Color),
    U16("ctime0"), U16("ctime1"), U16("ctime2"), U16("ctime3"),
    U16("ctime4"), U16("ctime5"), U16("ctime6"), U16("ctime7"),
    U16("atime0"), U16("atime1"), U16("atime2"), U16("atime3"),
    U16("atime4"), U16("atime5"), U16("atime6"), U16("atime7"),
    U8("m_Mode"), I8("m_ColorSpace"), U8("m_NumColorKeys"), U8("m_NumAlphaKeys"), Align,
];
const GUI_STYLE_STATE: &[Native] = &[
    Native::Object("m_Background"),
    Nested("m_TextColor", Builtin::Color),
];
#[rustfmt::skip]
const GUI_STYLE: &[Native] = &[
    Native::String("m_Name"),
    Nested("m_Normal", Builtin::GuiStyleState), Nested("m_Hover", Builtin::GuiStyleState),
    Nested("m_Active", Builtin::GuiStyleState), Nested("m_Focused", Builtin::GuiStyleState),
    Nested("m_OnNormal", Builtin::GuiStyleState), Nested("m_OnHover", Builtin::GuiStyleState),
    Nested("m_OnActive", Builtin::GuiStyleState), Nested("m_OnFocused", Builtin::GuiStyleState),
    Nested("m_Border", Builtin::RectOffset), Nested("m_Margin", Builtin::RectOffset),
    Nested("m_Padding", Builtin::RectOffset), Nested("m_Overflow", Builtin::RectOffset),
    Native::Object("m_Font"), I32("m_FontSize"), I32("m_FontStyle"), I32("m_Alignment"),
    Bool("m_WordWrap"), Bool("m_RichText"), Align,
    I32("m_TextClipping"), I32("m_ImagePosition"), Nested("m_ContentOffset", Builtin::Vector2),
    F32("m_FixedWidth"), F32("m_FixedHeight"), Bool("m_StretchWidth"), Bool("m_StretchHeight"), Align,
];

/// Reads a builtin value as a struct of its native fields.
///
/// # Errors
/// Fails when the data ends early or a value is malformed.
pub fn read_builtin(r: &mut ObjectReader<'_>, builtin: Builtin) -> Result<Value, SerializedError> {
    read_native(r, builtin.layout())
}

fn read_native(r: &mut ObjectReader<'_>, layout: &[Native]) -> Result<Value, SerializedError> {
    let mut fields = Vec::with_capacity(layout.len());
    for native in layout {
        let (name, value) = match native {
            Native::F32(name) => (*name, Value::Float(r.f32()?.into())),
            Native::I32(name) => (*name, Value::Int(r.i32()?.into())),
            Native::U32(name) => (*name, Value::UInt(r.u32()?.into())),
            Native::U16(name) => (*name, Value::UInt(r.u16()?.into())),
            Native::U8(name) => (*name, Value::UInt(r.u8()?.into())),
            Native::I8(name) => (*name, Value::Int(r.i8()?.into())),
            Native::Bool(name) => (*name, Value::Bool(r.bool()?)),
            Native::String(name) => (*name, Value::String(r.string()?)),
            Native::Object(name) => (*name, Value::Object(r.pptr()?)),
            Native::Nested(name, nested) => (*name, read_builtin(r, *nested)?),
            Native::Keyframes(name) => {
                let keys = r.array("keyframe", 28, |r| read_native(r, KEYFRAME))?;
                (*name, Value::Array(keys))
            }
            Native::Align => {
                r.align()?;
                continue;
            }
        };
        fields.push((name.to_owned(), value));
    }
    Ok(Value::Struct(fields))
}
