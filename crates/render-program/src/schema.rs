//! The properties a program takes and the kind of value each one accepts.
//!
//! A schema is built when a program registers, from the members of its constant layouts and
//! the resource bindings that name a property. A material checks every value against it, so a
//! property the program does not have or a value of the wrong kind is refused when it is set
//! rather than when a frame is drawn.

use std::collections::BTreeMap;

use crate::binding::{BindingSource, ResourceKind};
use crate::constants::{ConstantError, ConstantLayout, Member, pack_constants};
use crate::error::ProgramError;
use crate::properties::{PropertySet, PropertyValue};
use crate::registry::Variant;
use crate::resources::{Resource, Store};

/// What the value of one property must be.
#[derive(Clone, Debug)]
enum Expect {
    /// A member of a constant layout, which takes the values the packer writes for its shape.
    Constant(Member),
    Buffer,
    Texture,
}

impl Expect {
    /// Whether two declarations of one property accept the same values.
    fn agrees(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Constant(a), Self::Constant(b)) => {
                a.kind == b.kind
                    && (a.rows, a.cols) == (b.rows, b.cols)
                    && a.array.map(|array| array.count) == b.array.map(|array| array.count)
                    && a.matrix.is_some() == b.matrix.is_some()
            }
            (Self::Buffer, Self::Buffer) | (Self::Texture, Self::Texture) => true,
            _ => false,
        }
    }

    fn describe(&self) -> String {
        match self {
            Self::Constant(member) => format!(
                "a {:?} value of {} by {}{}",
                member.kind,
                member.rows,
                member.cols,
                member
                    .array
                    .map_or(String::new(), |array| format!(" times {}", array.count)),
            ),
            Self::Buffer => "a buffer".to_owned(),
            Self::Texture => "a texture".to_owned(),
        }
    }
}

/// The kind of a value, for an error message.
fn value_kind(value: &PropertyValue) -> &'static str {
    match value {
        PropertyValue::Float(_) => "a float",
        PropertyValue::Int(_) => "an int",
        PropertyValue::Uint(_) => "a uint",
        PropertyValue::Bool(_) => "a bool",
        PropertyValue::Vector(_) => "a vector",
        PropertyValue::Matrix(_) => "a matrix",
        PropertyValue::VectorArray(_) => "a vector array",
        PropertyValue::MatrixArray(_) => "a matrix array",
        PropertyValue::IntVector(_) => "an integer vector",
        PropertyValue::IntVectorArray(_) => "an integer vector array",
        PropertyValue::Resource(Resource::Buffer(_)) => "a buffer",
        PropertyValue::Resource(Resource::Texture(_)) => "a texture",
    }
}

#[derive(Default)]
pub(crate) struct Schema {
    properties: BTreeMap<String, Expect>,
}

impl Schema {
    /// The properties the variants of the program `label` take.
    ///
    /// # Errors
    /// Fails when two declarations use one property name for values of different kinds.
    pub(crate) fn of(label: &str, variants: &[&Variant]) -> Result<Self, ProgramError> {
        let mut schema = Self::default();
        for variant in variants {
            for constants in &variant.constants {
                for member in &constants.layout.members {
                    if let BindingSource::Property(id) = &member.source {
                        schema.add(label, id.as_str(), Expect::Constant(member.clone()))?;
                    }
                }
            }
            for binding in &variant.resources {
                let covered = variant.constants.iter().any(|c| c.at == binding.at);
                let BindingSource::Property(id) = &binding.source else {
                    continue;
                };
                match binding.kind {
                    _ if covered => {}
                    ResourceKind::Uniform { .. } | ResourceKind::Storage { .. } => {
                        schema.add(label, id.as_str(), Expect::Buffer)?;
                    }
                    ResourceKind::Texture { .. } | ResourceKind::StorageImage { .. } => {
                        schema.add(label, id.as_str(), Expect::Texture)?;
                    }
                    ResourceKind::Sampler(_) => {}
                }
            }
        }
        Ok(schema)
    }

    fn add(&mut self, label: &str, name: &str, expect: Expect) -> Result<(), ProgramError> {
        match self.properties.get(name) {
            None => {
                self.properties.insert(name.to_owned(), expect);
            }
            Some(known) if known.agrees(&expect) => {}
            Some(_) => {
                return Err(ProgramError::PropertyConflict {
                    program: label.to_owned(),
                    property: name.to_owned(),
                });
            }
        }
        Ok(())
    }

    /// Checks that the program `label` has a property `name` and that `value` is one it takes.
    /// A resource handle must also be live in `store`.
    ///
    /// # Errors
    /// Fails with `UnknownProperty`, `WrongPropertyKind`, an array longer than its member, or
    /// `UnknownHandle` for a resource that was released.
    pub(crate) fn check(
        &self,
        store: &Store,
        label: &str,
        name: &str,
        value: &PropertyValue,
    ) -> Result<(), ProgramError> {
        let expect = self
            .properties
            .get(name)
            .ok_or_else(|| ProgramError::UnknownProperty {
                program: label.to_owned(),
                property: name.to_owned(),
            })?;
        let wrong = || ProgramError::WrongPropertyKind {
            property: name.to_owned(),
            expected: expect.describe(),
            found: value_kind(value).to_owned(),
        };
        match (expect, value) {
            (Expect::Constant(member), _) => accepts(member, name, value).map_err(|error| {
                if matches!(error, ConstantError::WrongKind { .. }) {
                    wrong()
                } else {
                    error.into()
                }
            }),
            (Expect::Buffer, PropertyValue::Resource(Resource::Buffer(handle))) => {
                store.buffer(*handle).map(|_| ())
            }
            (Expect::Texture, PropertyValue::Resource(Resource::Texture(handle))) => {
                store.texture(*handle).map(|_| ())
            }
            _ => Err(wrong()),
        }
    }

    /// Checks every value of `set`.
    ///
    /// # Errors
    /// Fails like [`check`](Self::check) for the first value that fails.
    pub(crate) fn check_all(
        &self,
        store: &Store,
        label: &str,
        set: &PropertySet,
    ) -> Result<(), ProgramError> {
        set.iter()
            .try_for_each(|(name, value)| self.check(store, label, name, value))
    }
}

/// Whether the constant packer writes `value` into `member`: the packer is the one authority
/// on which values a member takes, so the value is packed into a buffer of its own.
fn accepts(member: &Member, name: &str, value: &PropertyValue) -> Result<(), ConstantError> {
    let mut probe = member.clone();
    probe.offset = 0;
    let size = usize::try_from(probe.extent()).unwrap_or(usize::MAX);
    let layout = ConstantLayout {
        name: name.to_owned(),
        byte_size: 0,
        members: vec![probe],
    };
    let mut only = PropertySet::new();
    only.set(name, value.clone());
    pack_constants(&layout, &[&only], size).map(|_| ())
}
