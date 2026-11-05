use crate::{SaveError, StoreLimits, byte_io::check_size};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq)]
pub enum SettingValue {
    Bool(bool),
    Integer(i64),
    Float(f64),
    String(String),
}

pub type Settings = BTreeMap<String, SettingValue>;

pub(crate) fn encode(values: &Settings, limits: StoreLimits) -> Result<Vec<u8>, SaveError> {
    check_size(values.len(), limits.settings_entries)?;
    let mut bytes = Vec::new();
    for (key, value) in values {
        validate_key(key, limits)?;
        let length = match value {
            SettingValue::Bool(_) => 1,
            SettingValue::Integer(_) | SettingValue::Float(_) => 8,
            SettingValue::String(text) => text.len(),
        };
        check_size(length, limits.value_bytes)?;
        let total = bytes
            .len()
            .checked_add(9)
            .and_then(|v| v.checked_add(key.len()))
            .and_then(|v| v.checked_add(length))
            .ok_or(SaveError::InvalidLength)?;
        check_size(total, limits.settings_bytes)?;
        bytes.extend_from_slice(
            &u32::try_from(key.len())
                .expect("validated key length")
                .to_le_bytes(),
        );
        bytes.extend_from_slice(key.as_bytes());
        append_value(&mut bytes, value)?;
    }
    Ok(bytes)
}

fn validate_key(key: &str, limits: StoreLimits) -> Result<(), SaveError> {
    if key.is_empty() {
        return Err(SaveError::InvalidSettings("empty key"));
    }
    check_size(key.len(), limits.value_bytes)
}

fn append_value(bytes: &mut Vec<u8>, value: &SettingValue) -> Result<(), SaveError> {
    match value {
        SettingValue::Bool(value) => {
            bytes.push(0);
            bytes.extend_from_slice(&1_u32.to_le_bytes());
            bytes.push(u8::from(*value));
        }
        SettingValue::Integer(value) => {
            bytes.push(1);
            bytes.extend_from_slice(&8_u32.to_le_bytes());
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        SettingValue::Float(value) => {
            if !value.is_finite() {
                return Err(SaveError::InvalidSettings("non-finite float"));
            }
            bytes.push(2);
            bytes.extend_from_slice(&8_u32.to_le_bytes());
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        SettingValue::String(value) => {
            bytes.push(3);
            bytes.extend_from_slice(
                &u32::try_from(value.len())
                    .expect("validated string length")
                    .to_le_bytes(),
            );
            bytes.extend_from_slice(value.as_bytes());
        }
    }
    Ok(())
}

pub(crate) fn decode(bytes: &[u8], limits: StoreLimits) -> Result<Settings, SaveError> {
    check_size(bytes.len(), limits.settings_bytes)?;
    let mut input = Input(bytes);
    let mut values = Settings::new();
    while !input.0.is_empty() {
        check_size(values.len() + 1, limits.settings_entries)?;
        let key = input.text(limits.value_bytes)?;
        validate_key(&key, limits)?;
        let tag = input.take(1)?[0];
        let length = input.length(limits.value_bytes)?;
        let value = decode_value(tag, input.take(length)?)?;
        if values.insert(key, value).is_some() {
            return Err(SaveError::InvalidSettings("duplicate key"));
        }
    }
    Ok(values)
}

fn decode_value(tag: u8, bytes: &[u8]) -> Result<SettingValue, SaveError> {
    match (tag, bytes.len()) {
        (0, 1) if bytes[0] <= 1 => Ok(SettingValue::Bool(bytes[0] == 1)),
        (1, 8) => Ok(SettingValue::Integer(i64::from_le_bytes(
            bytes.try_into().expect("eight bytes"),
        ))),
        (2, 8) => {
            let value = f64::from_le_bytes(bytes.try_into().expect("eight bytes"));
            if !value.is_finite() {
                return Err(SaveError::InvalidSettings("non-finite float"));
            }
            Ok(SettingValue::Float(value))
        }
        (3, _) => Ok(SettingValue::String(text(bytes)?.to_owned())),
        _ => Err(SaveError::InvalidSettings("invalid value tag or length")),
    }
}

fn text(bytes: &[u8]) -> Result<&str, SaveError> {
    std::str::from_utf8(bytes).map_err(|_| SaveError::InvalidSettings("invalid UTF-8"))
}

struct Input<'a>(&'a [u8]);

impl<'a> Input<'a> {
    fn take(&mut self, length: usize) -> Result<&'a [u8], SaveError> {
        if length > self.0.len() {
            return Err(SaveError::Truncated);
        }
        let (head, tail) = self.0.split_at(length);
        self.0 = tail;
        Ok(head)
    }

    fn length(&mut self, limit: usize) -> Result<usize, SaveError> {
        let length = u32::from_le_bytes(self.take(4)?.try_into().expect("four bytes")) as usize;
        check_size(length, limit)?;
        Ok(length)
    }

    fn text(&mut self, limit: usize) -> Result<String, SaveError> {
        let length = self.length(limit)?;
        Ok(text(self.take(length)?)?.to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn malformed_settings_fail_explicitly() {
        let limits = StoreLimits {
            payload_bytes: 1024,
            label_bytes: 32,
            settings_bytes: 256,
            value_bytes: 32,
            settings_entries: 8,
        };
        let values = Settings::from([("a".into(), SettingValue::Integer(1))]);
        let bytes = encode(&values, limits).unwrap();
        assert!(matches!(
            decode(&bytes[..bytes.len() - 1], limits),
            Err(SaveError::Truncated)
        ));
        let mut duplicate = bytes.clone();
        duplicate.extend_from_slice(&bytes);
        assert!(matches!(
            decode(&duplicate, limits),
            Err(SaveError::InvalidSettings("duplicate key"))
        ));
        let mut oversized = bytes.clone();
        oversized[..4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(matches!(
            decode(&oversized, limits),
            Err(SaveError::PayloadTooLarge { .. })
        ));
        let mut unknown_tag = bytes;
        unknown_tag[5] = 99;
        assert!(matches!(
            decode(&unknown_tag, limits),
            Err(SaveError::InvalidSettings(_))
        ));
        let values = Settings::from([("a".into(), SettingValue::Float(f64::NAN))]);
        assert!(matches!(
            encode(&values, limits),
            Err(SaveError::InvalidSettings(_))
        ));
    }
}
