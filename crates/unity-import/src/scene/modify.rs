use super::value::{Reference, Value};
use crate::yaml::Scalar;

/// Largest array a property override may create.
const MAX_ARRAY: usize = 1 << 20;

/// A prefab override's new value: its `value` text, and its `objectReference` when not null.
pub(crate) struct Override {
    pub scalar: Scalar,
    pub reference: Reference,
}

/// Applies `new` at a Unity property path such as `m_LocalPosition.x`,
/// `m_Materials.Array.data[0]` or `m_Calls.Array.size`.
pub(crate) fn apply(fields: &mut Value, path: &str, new: &Override) -> Result<(), String> {
    let mut target = fields;
    let mut parts = path.split('.');
    while let Some(part) = parts.next() {
        if part != "Array" {
            target = field(target, part)?;
            continue;
        }
        let element = parts.next().ok_or("property path ends after `Array`")?;
        let items = sequence(target)?;
        if element == "size" {
            if parts.next().is_some() {
                return Err("`Array.size` must end the property path".into());
            }
            let size = new
                .scalar
                .as_i64()
                .and_then(|size| usize::try_from(size).ok())
                .filter(|size| *size <= MAX_ARRAY)
                .ok_or_else(|| format!("invalid array size `{}`", new.scalar.text()))?;
            items.resize(size, Value::empty());
            return Ok(());
        }
        let index = element
            .strip_prefix("data[")
            .and_then(|rest| rest.strip_suffix(']'))
            .and_then(|index| index.parse::<usize>().ok())
            .filter(|index| *index < MAX_ARRAY)
            .ok_or_else(|| format!("invalid array element `{element}`"))?;
        if index >= items.len() {
            items.resize(index + 1, Value::empty());
        }
        target = &mut items[index];
    }
    *target = match (new.reference, &*target) {
        (Reference::Null, Value::Reference(_)) => Value::Reference(Reference::Null),
        (Reference::Null, _) => Value::Scalar(new.scalar.clone()),
        (reference, _) => Value::Reference(reference),
    };
    Ok(())
}

fn field<'a>(value: &'a mut Value, name: &str) -> Result<&'a mut Value, String> {
    if matches!(value, Value::Scalar(scalar) if scalar.is_empty()) {
        *value = Value::Record(Vec::new());
    }
    let Value::Record(fields) = value else {
        return Err(format!("`{name}` is inside a value that is not a record"));
    };
    let index = fields.iter().position(|(key, _)| key == name);
    let index = index.unwrap_or_else(|| {
        fields.push((name.to_owned(), Value::empty()));
        fields.len() - 1
    });
    Ok(&mut fields[index].1)
}

fn sequence(value: &mut Value) -> Result<&mut Vec<Value>, String> {
    if matches!(value, Value::Scalar(scalar) if scalar.is_empty()) {
        *value = Value::Sequence(Vec::new());
    }
    match value {
        Value::Sequence(items) => Ok(items),
        _ => Err("`Array` indexes a value that is not an array".into()),
    }
}
