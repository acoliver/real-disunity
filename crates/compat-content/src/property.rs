use serde_json::{Value, json};

pub(crate) fn qualify(mut value: Value, guid: &str, scope: &str) -> Value {
    if value.get("guid").is_none() && value["fileID"].as_i64().is_some_and(|v| v != 0) {
        value["guid"] = guid.into();
        value["_instance"] = scope.into();
    }
    value
}
pub(crate) fn scalar_override(raw: &Value, current: Option<&Value>) -> Value {
    let Some(text) = raw.as_str() else {
        return raw.clone();
    };
    if current.is_some_and(Value::is_string) {
        return raw.clone();
    }
    if current.is_some_and(Value::is_boolean) {
        return Value::Bool(text == "1" || text == "true");
    }
    serde_json::from_str(text).unwrap_or_else(|_| raw.clone())
}
#[derive(Debug)]
enum Part {
    Field(String),
    Index(usize),
    Size,
}
fn parts(path: &str) -> Result<Vec<Part>, String> {
    if path.is_empty() {
        return Err("empty property path".into());
    }
    let mut result = vec![];
    let mut tokens = path.split('.').peekable();
    while let Some(token) = tokens.next() {
        if token == "Array" {
            match tokens.next() {
                Some("size") => result.push(Part::Size),
                Some(t) if t.starts_with("data[") && t.ends_with(']') => result.push(Part::Index(
                    t[5..t.len() - 1]
                        .parse()
                        .map_err(|_| "invalid array index")?,
                )),
                _ => return Err("invalid Array property path".into()),
            }
        } else {
            result.push(Part::Field(token.into()));
        }
    }
    Ok(result)
}
pub(crate) fn get_property<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    let mut value = value;
    for part in parts(path).ok()? {
        value = match part {
            Part::Field(f) => value.get(f)?,
            Part::Index(i) => value.get(i)?,
            Part::Size => return None,
        };
    }
    Some(value)
}
pub(crate) fn set_property(
    value: &mut Value,
    path: &str,
    replacement: Value,
    limit: usize,
) -> Result<(), String> {
    let tokens = parts(path)?;
    let mut value = value;
    for part in &tokens {
        match part {
            Part::Field(f) => {
                if value.is_null() {
                    *value = json!({});
                }
                value = value
                    .as_object_mut()
                    .ok_or("property path crosses scalar")?
                    .entry(f)
                    .or_insert(Value::Null);
            }
            Part::Index(i) => {
                if *i >= limit {
                    return Err("array index limit exceeded".into());
                }
                if value.is_null() {
                    *value = json!([]);
                }
                let array = value
                    .as_array_mut()
                    .ok_or("property path crosses non-array")?;
                if array.len() <= *i {
                    array.resize(*i + 1, Value::Null);
                }
                value = &mut array[*i];
            }
            Part::Size => {
                let len = replacement
                    .as_u64()
                    .and_then(|v| usize::try_from(v).ok())
                    .ok_or("invalid array size")?;
                if len > limit {
                    return Err("array size limit exceeded".into());
                }
                if value.is_null() {
                    *value = json!([]);
                }
                value
                    .as_array_mut()
                    .ok_or("size applied to non-array")?
                    .resize(len, Value::Null);
                return Ok(());
            }
        }
    }
    *value = replacement;
    Ok(())
}
