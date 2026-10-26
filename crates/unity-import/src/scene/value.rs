use super::ObjectId;
use crate::package::PackageAsset;
use crate::{
    guid::Guid,
    yaml::{Node, Scalar},
};

/// A serialized field value. Scalars stay untyped text, because only the declaring C# or
/// engine type says whether `1` is a number, a boolean or an enum.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Scalar(Scalar),
    Sequence(Vec<Value>),
    Record(Vec<(String, Value)>),
    Reference(Reference),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reference {
    Null,
    Object(ObjectId),
    /// A known asset from Unity itself or a package, which is not a file in the project.
    Package(PackageAsset),
    /// An asset the scene does not expand, such as a texture, script or built-in resource.
    External {
        guid: Guid,
        file_id: i64,
    },
    /// A reference whose target is not in the project.
    Missing {
        guid: Option<Guid>,
        file_id: i64,
    },
}

impl Value {
    pub(crate) fn empty() -> Self {
        Self::Scalar(Scalar::plain(""))
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        self.as_record()?
            .iter()
            .find_map(|(name, value)| (name == key).then_some(value))
    }

    pub fn as_scalar(&self) -> Option<&Scalar> {
        match self {
            Self::Scalar(scalar) => Some(scalar),
            _ => None,
        }
    }

    pub fn as_sequence(&self) -> Option<&[Value]> {
        match self {
            Self::Sequence(items) => Some(items),
            _ => None,
        }
    }

    pub fn as_record(&self) -> Option<&[(String, Value)]> {
        match self {
            Self::Record(fields) => Some(fields),
            _ => None,
        }
    }

    pub fn as_reference(&self) -> Option<Reference> {
        match self {
            Self::Reference(reference) => Some(*reference),
            _ => None,
        }
    }

    pub fn visit_references(&self, visit: &mut impl FnMut(&Reference)) {
        match self {
            Self::Scalar(_) => {}
            Self::Reference(reference) => visit(reference),
            Self::Sequence(items) => items.iter().for_each(|item| item.visit_references(visit)),
            Self::Record(fields) => fields
                .iter()
                .for_each(|(_, value)| value.visit_references(visit)),
        }
    }
}

/// Converts parsed YAML, resolving each `{fileID, guid, type}` mapping through `resolve`.
pub(crate) fn convert<E>(
    node: &Node,
    resolve: &mut dyn FnMut(&Node) -> Result<Reference, E>,
) -> Result<Value, E> {
    Ok(match node {
        Node::Scalar(scalar) => Value::Scalar(scalar.clone()),
        Node::Sequence(items) => Value::Sequence(
            items
                .iter()
                .map(|item| convert(item, resolve))
                .collect::<Result<_, _>>()?,
        ),
        Node::Mapping(_) if is_reference(node) => Value::Reference(resolve(node)?),
        Node::Mapping(mapping) => Value::Record(
            mapping
                .iter()
                .map(|(key, value)| Ok((key.to_owned(), convert(value, resolve)?)))
                .collect::<Result<_, E>>()?,
        ),
    })
}

/// Unity writes object references as flow mappings of `fileID` and optionally `guid` and `type`.
pub(crate) fn is_reference(node: &Node) -> bool {
    node.as_mapping().is_some_and(|mapping| {
        mapping.get("fileID").is_some()
            && mapping
                .iter()
                .all(|(key, _)| matches!(key, "fileID" | "guid" | "type"))
    })
}
