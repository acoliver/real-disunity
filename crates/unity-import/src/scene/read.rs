//! Typed reads of serialized fields. Each returns `None` when a present field has the wrong
//! shape, and its default when the field is absent.

use super::{ObjectId, Reference, Value};

pub(crate) const XYZW: [&str; 4] = ["x", "y", "z", "w"];
pub(crate) const RGBA: [&str; 4] = ["r", "g", "b", "a"];
/// `Matrix4x4` fields in serialization order.
pub(crate) const MATRIX: [&str; 16] = [
    "e00", "e01", "e02", "e03", "e10", "e11", "e12", "e13", "e20", "e21", "e22", "e23", "e30",
    "e31", "e32", "e33",
];
pub(crate) const IDENTITY: [f64; 16] = [
    1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
];

pub(crate) fn object_id(value: &Value) -> Option<ObjectId> {
    match value.as_reference()? {
        Reference::Object(id) => Some(id),
        _ => None,
    }
}

pub(crate) fn text(fields: &Value, key: &str) -> Option<String> {
    Some(fields.get(key)?.as_scalar()?.text().to_owned())
}

pub(crate) fn as_number(value: &Value) -> Option<f64> {
    value
        .as_scalar()?
        .as_f64()
        .filter(|value| value.is_finite())
}

pub(crate) fn as_integer(value: &Value) -> Option<i64> {
    value.as_scalar()?.as_i64()
}

pub(crate) fn number(fields: &Value, key: &str, default: f64) -> Option<f64> {
    fields.get(key).map_or(Some(default), as_number)
}

pub(crate) fn integer(fields: &Value, key: &str, default: i64) -> Option<i64> {
    fields.get(key).map_or(Some(default), as_integer)
}

/// A struct of numbers such as `{x: 1, y: 2}`: the members `names` of `value`, each defaulting
/// to the matching entry of `default`.
pub(crate) fn members<const N: usize>(
    value: &Value,
    names: impl IntoIterator<Item = &'static str>,
    default: [f64; N],
) -> Option<[f64; N]> {
    let mut out = default;
    for (slot, name) in out.iter_mut().zip(names) {
        *slot = number(value, name, *slot)?;
    }
    Some(out)
}

/// [`members`] of the field `key`, or `default` when it is absent.
pub(crate) fn vector<const N: usize>(
    fields: &Value,
    key: &str,
    names: impl IntoIterator<Item = &'static str>,
    default: [f64; N],
) -> Option<[f64; N]> {
    fields
        .get(key)
        .map_or(Some(default), |value| members(value, names, default))
}
