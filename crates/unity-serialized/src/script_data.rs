//! A script's serialized fields: the bytes after a `MonoBehaviour`'s header, read with a
//! layout of the script class. Players strip type trees, so the layout comes from the
//! script's assembly (Disunity.Engine's `ScriptLayouts`); this module applies Unity's
//! byte rules to it: alignment, the native layouts of builtin values, and the nesting limit.

use std::collections::{HashMap, HashSet};

use crate::{
    SerializedError,
    object::{ObjectReader, PPtr},
    script_builtin::{Builtin, read_builtin},
};

/// A number, `bool`, `char` or `string` field. Enums of up to 32 bits are their underlying
/// type; Unity does not serialize wider ones.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Primitive {
    Bool,
    I8,
    U8,
    I16,
    U16,
    /// A UTF-16 code unit.
    Char,
    I32,
    U32,
    I64,
    U64,
    F32,
    F64,
    String,
}

impl Primitive {
    /// The primitive with Unity's type tree name, such as `int` or `unsigned int`.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "bool" => Self::Bool,
            "SInt8" => Self::I8,
            "UInt8" => Self::U8,
            "SInt16" => Self::I16,
            "UInt16" => Self::U16,
            "char" => Self::Char,
            "int" => Self::I32,
            "unsigned int" => Self::U32,
            "SInt64" => Self::I64,
            "UInt64" => Self::U64,
            "float" => Self::F32,
            "double" => Self::F64,
            "string" => Self::String,
            _ => return None,
        })
    }

    /// Unity aligns to four bytes after a field narrower than four bytes, though not after
    /// each element of an array of them.
    fn narrow(self) -> bool {
        matches!(
            self,
            Self::Bool | Self::I8 | Self::U8 | Self::I16 | Self::U16 | Self::Char
        )
    }
}

/// How a field is serialized.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FieldType {
    Primitive(Primitive),
    /// A reference to a `UnityEngine.Object` of the named class.
    Object(String),
    Builtin(Builtin),
    /// A `[Serializable]` class or struct: an index into [`ScriptLayouts::types`].
    Composite(usize),
    /// An array or `List<T>`.
    Array(Box<FieldType>),
    /// A `[SerializeReference]` field of the named declared type.
    Reference(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScriptField {
    pub name: String,
    pub ty: FieldType,
}

/// A class or struct's serialized fields, base classes first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScriptType {
    /// The defining assembly and full name, such as `Assembly-CSharp:Ns.Name`.
    pub key: String,
    pub value_type: bool,
    pub fields: Vec<ScriptField>,
}

/// The layouts of a player's script classes and the types their fields hold.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScriptLayouts {
    pub types: Vec<ScriptType>,
    index: HashMap<String, usize>,
}

impl ScriptLayouts {
    #[must_use]
    pub fn new(types: Vec<ScriptType>) -> Self {
        let index = types
            .iter()
            .enumerate()
            .map(|(i, ty)| (ty.key.clone(), i))
            .collect();
        Self { types, index }
    }

    /// The index of the layout with this key.
    #[must_use]
    pub fn find(&self, key: &str) -> Option<usize> {
        self.index.get(key).copied()
    }

    /// Whether a type holds a `[SerializeReference]` field, directly or through the
    /// composites its fields hold. Objects of such a type end with a reference registry.
    #[must_use]
    pub fn has_references(&self, ty: usize) -> bool {
        let mut seen = HashSet::new();
        let mut pending = vec![ty];
        while let Some(index) = pending.pop() {
            if !seen.insert(index) {
                continue;
            }
            for field in &self.types[index].fields {
                let mut ty = &field.ty;
                while let FieldType::Array(element) = ty {
                    ty = element;
                }
                match ty {
                    FieldType::Reference(_) => return true,
                    FieldType::Composite(nested) => pending.push(*nested),
                    _ => {}
                }
            }
        }
        false
    }
}

/// A decoded field value.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Bool(bool),
    Int(i64),
    UInt(u64),
    Float(f64),
    String(String),
    Object(PPtr),
    Array(Vec<Value>),
    /// A composite or builtin value's fields, in order.
    Struct(Vec<(String, Value)>),
    /// A `[SerializeReference]` value's ID in the object's reference registry.
    Reference(i64),
}

/// The name of the pseudo-field that holds an object's reference registry: one struct per
/// referenced value with its `rid`, `class`, `namespace`, `assembly` and `data`.
pub const REFERENCES: &str = "references";

/// The registry format Unity 2021.2 and later write.
const REGISTRY_VERSION: i32 = 2;

/// A `[Serializable]` class nested more than this many classes below the script keeps its
/// other fields but leaves out those that hold classes, directly or in an array. A class
/// holding a list of itself, or itself through another class, ends there. (A field of a
/// class's own type is not serialized at all; the layout leaves it out.)
pub const MAX_CLASS_DEPTH: usize = 10;

/// Values nested deeper than this, counting structs, which Unity does not limit, are taken
/// as malformed data, so a struct holding an array of itself cannot exhaust the stack.
pub const MAX_NESTING: usize = 64;

/// Reads the script fields of a `MonoBehaviour`, starting `start` bytes into its object
/// bytes, with the layout at `root`. The fields must use every remaining byte.
///
/// A type with `[SerializeReference]` fields is followed by the object's reference
/// registry, returned as the [`REFERENCES`] field. Unity 6000.3 writes registry version 2
/// there; other versions are rejected.
///
/// # Errors
/// Fails when the data ends early or bytes remain, when a value is malformed, or when the
/// registry is in another format or names a class without a layout.
pub fn read_script(
    object: &[u8],
    start: usize,
    big_endian: bool,
    layouts: &ScriptLayouts,
    root: usize,
) -> Result<Vec<(String, Value)>, SerializedError> {
    let mut reader = ScriptReader {
        r: ObjectReader::at(object, start, big_endian),
        layouts,
        nesting: 0,
    };
    let mut fields = reader.fields(root, 0)?;
    if layouts.has_references(root) {
        fields.push((REFERENCES.to_owned(), reader.registry()?));
    }
    if reader.r.remaining() != 0 {
        return Err(SerializedError::ScriptTrailingBytes {
            script: layouts.types[root].key.clone(),
            read: reader.r.position() - start,
            size: object.len() - start,
        });
    }
    Ok(fields)
}

struct ScriptReader<'a, 'l> {
    r: ObjectReader<'a>,
    layouts: &'l ScriptLayouts,
    /// Composite values entered and not yet finished.
    nesting: usize,
}

impl ScriptReader<'_, '_> {
    fn fields(&mut self, ty: usize, depth: usize) -> Result<Vec<(String, Value)>, SerializedError> {
        let layout = &self.layouts.types[ty];
        self.nesting += 1;
        if self.nesting > MAX_NESTING {
            return Err(SerializedError::UnsupportedScriptField {
                field_type: format!("{} nested more than {MAX_NESTING} values deep", layout.key),
            });
        }
        let mut values = Vec::with_capacity(layout.fields.len());
        for field in &layout.fields {
            if let Some(value) = self.field(&field.ty, depth)? {
                values.push((field.name.clone(), value));
            }
        }
        self.nesting -= 1;
        Ok(values)
    }

    /// A direct field of a class or struct; `None` when Unity's nesting limit leaves it out.
    fn field(&mut self, ty: &FieldType, depth: usize) -> Result<Option<Value>, SerializedError> {
        let mut held = ty;
        while let FieldType::Array(element) = held {
            held = element;
        }
        if let FieldType::Composite(index) = held
            && !self.layouts.types[*index].value_type
            && depth > MAX_CLASS_DEPTH
        {
            return Ok(None);
        }
        let value = self.value(ty, depth)?;
        if let FieldType::Primitive(primitive) = ty
            && primitive.narrow()
        {
            self.r.align()?;
        }
        Ok(Some(value))
    }

    /// A value without the alignment Unity adds after a direct field.
    fn value(&mut self, ty: &FieldType, depth: usize) -> Result<Value, SerializedError> {
        Ok(match ty {
            FieldType::Primitive(primitive) => self.primitive(*primitive)?,
            FieldType::Object(_) => Value::Object(self.r.pptr()?),
            FieldType::Builtin(builtin) => read_builtin(&mut self.r, *builtin)?,
            FieldType::Composite(index) => {
                let nested = depth + usize::from(!self.layouts.types[*index].value_type);
                Value::Struct(self.fields(*index, nested)?)
            }
            FieldType::Array(element) => self.array(element, depth)?,
            FieldType::Reference(_) => Value::Reference(self.r.i64()?),
        })
    }

    fn primitive(&mut self, primitive: Primitive) -> Result<Value, SerializedError> {
        let r = &mut self.r;
        Ok(match primitive {
            Primitive::Bool => Value::Bool(r.bool()?),
            Primitive::I8 => Value::Int(r.i8()?.into()),
            Primitive::U8 => Value::UInt(r.u8()?.into()),
            Primitive::I16 => Value::Int(r.i16()?.into()),
            Primitive::U16 | Primitive::Char => Value::UInt(r.u16()?.into()),
            Primitive::I32 => Value::Int(r.i32()?.into()),
            Primitive::U32 => Value::UInt(r.u32()?.into()),
            Primitive::I64 => Value::Int(r.i64()?),
            Primitive::U64 => Value::UInt(r.u64()?),
            Primitive::F32 => Value::Float(r.f32()?.into()),
            Primitive::F64 => Value::Float(r.f64()?),
            Primitive::String => Value::String(r.string()?),
        })
    }

    /// The registry of an object's `[SerializeReference]` values. Each entry names its class.
    /// An entry with a negative ID (-1 unknown, -2 null) or an empty class name has no data.
    fn registry(&mut self) -> Result<Value, SerializedError> {
        let version = self.r.i32()?;
        if version != REGISTRY_VERSION {
            return Err(SerializedError::UnsupportedScriptField {
                field_type: format!("a version {version} reference registry"),
            });
        }
        let count = self.count("referenced object")?;
        let mut entries = Vec::new();
        for _ in 0..count {
            entries.push(self.registry_entry()?);
        }
        self.r.align()?;
        Ok(Value::Array(entries))
    }

    fn registry_entry(&mut self) -> Result<Value, SerializedError> {
        let rid = self.r.i64()?;
        let class = self.r.string()?;
        let namespace = self.r.string()?;
        let assembly = self.r.string()?;
        let data = if rid < 0 || class.is_empty() {
            if rid < 0 && !class.is_empty() {
                return Err(SerializedError::UnsupportedScriptField {
                    field_type: format!("a registry entry {rid} that names class {class}"),
                });
            }
            Vec::new()
        } else {
            let name = if namespace.is_empty() {
                class.clone()
            } else {
                format!("{namespace}.{class}")
            };
            let key = format!("{assembly}:{name}");
            let ty = self
                .layouts
                .find(&key)
                .ok_or(SerializedError::UnsupportedScriptField {
                    field_type: format!("referenced class {key}, which has no layout"),
                })?;
            self.fields(ty, 0)?
        };
        Ok(Value::Struct(vec![
            ("rid".to_owned(), Value::Int(rid)),
            ("class".to_owned(), Value::String(class)),
            ("namespace".to_owned(), Value::String(namespace)),
            ("assembly".to_owned(), Value::String(assembly)),
            ("data".to_owned(), Value::Struct(data)),
        ]))
    }

    /// An element count, which cannot exceed the bytes left.
    fn count(&mut self, what: &'static str) -> Result<usize, SerializedError> {
        let offset = self.r.position();
        let count = self.r.i32()?;
        usize::try_from(count)
            .ok()
            .filter(|&n| n <= self.r.remaining())
            .ok_or(SerializedError::InvalidCount {
                what,
                count,
                offset,
                remaining: self.r.remaining(),
            })
    }

    /// A count, the elements without per-element alignment, then alignment. An element that
    /// is an array is a nested collection, which Unity does not serialize.
    fn array(&mut self, element: &FieldType, depth: usize) -> Result<Value, SerializedError> {
        if matches!(element, FieldType::Array(_)) {
            return Err(SerializedError::UnsupportedScriptField {
                field_type: "an array of arrays".to_owned(),
            });
        }
        let count = self.count("script array element")?;
        let mut items = Vec::new();
        for _ in 0..count {
            items.push(self.value(element, depth)?);
        }
        self.r.align()?;
        Ok(Value::Array(items))
    }
}
