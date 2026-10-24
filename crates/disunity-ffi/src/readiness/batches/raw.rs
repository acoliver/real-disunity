//! Allocation-bounded token validation before JSON maps erase duplicate keys.
use serde::de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use std::fmt;
const KEYS: &[&str] = &[
    "op",
    "context",
    "batch",
    "index",
    "envelope",
    "version",
    "requestId",
    "world",
    "session",
    "frame",
    "fixed_tick",
    "phase",
    "sequence",
    "token",
    "lease",
    "root_phase",
    "flags",
    "time",
    "numerator",
    "denominator",
    "dynamic_delta",
    "fixed_delta",
    "unscaled_delta",
    "callback_batch",
    "callbacks",
    "binding",
    "component",
    "kind",
    "payload",
];
#[derive(Default)]
struct Scan {
    nodes: usize,
    contextual: bool,
    duplicate: bool,
    bounded: bool,
}
struct Token<'a> {
    scan: &'a mut Scan,
    depth: usize,
    callbacks: bool,
    op: bool,
}
pub(crate) fn preflight(bytes: &[u8]) -> Result<(), i32> {
    let scan = scan(bytes, false)?;
    if scan.contextual {
        if scan.duplicate {
            return Err(crate::result_code::INVALID_ARGUMENT);
        }
        scan_bytes(bytes)?;
    }
    Ok(())
}
fn scan_bytes(bytes: &[u8]) -> Result<(), i32> {
    scan(bytes, true).map(|_| ())
}
fn scan(bytes: &[u8], bounded: bool) -> Result<Scan, i32> {
    let mut scan = Scan {
        bounded,
        ..Scan::default()
    };
    let mut d = serde_json::Deserializer::from_slice(bytes);
    Token {
        scan: &mut scan,
        depth: 0,
        callbacks: false,
        op: false,
    }
    .deserialize(&mut d)
    .map_err(|_| crate::result_code::INVALID_ARGUMENT)?;
    d.end().map_err(|_| crate::result_code::INVALID_ARGUMENT)?;
    Ok(scan)
}

impl<'de> DeserializeSeed<'de> for Token<'_> {
    type Value = ();
    fn deserialize<D: de::Deserializer<'de>>(self, d: D) -> Result<(), D::Error> {
        self.scan.nodes += 1;
        if self.scan.bounded && (self.depth > 64 || self.scan.nodes > 100_000) {
            return Err(de::Error::custom("raw token budget exceeded"));
        }
        d.deserialize_any(self)
    }
}
impl<'de> Visitor<'de> for Token<'_> {
    type Value = ();
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("bounded JSON")
    }
    fn visit_bool<E: de::Error>(self, _: bool) -> Result<(), E> {
        Ok(())
    }
    fn visit_u64<E: de::Error>(self, _: u64) -> Result<(), E> {
        Ok(())
    }
    fn visit_i64<E: de::Error>(self, _: i64) -> Result<(), E> {
        Ok(())
    }
    fn visit_f64<E: de::Error>(self, _: f64) -> Result<(), E> {
        Ok(())
    }
    fn visit_unit<E: de::Error>(self) -> Result<(), E> {
        Ok(())
    }
    fn visit_str<E: de::Error>(self, v: &str) -> Result<(), E> {
        if self.op && v.starts_with("callbacks.") {
            self.scan.contextual = true;
        }
        Ok(())
    }
    fn visit_map<A: MapAccess<'de>>(self, mut a: A) -> Result<(), A::Error> {
        let mut mask = 0_u64;
        while let Some(key) = a.next_key::<String>()? {
            if let Some(i) = KEYS.iter().position(|k| *k == key) {
                let bit = 1_u64 << i;
                self.scan.duplicate |= mask & bit != 0;
                mask |= bit;
            }
            a.next_value_seed(Token {
                scan: self.scan,
                depth: self.depth + 1,
                callbacks: key == "callbacks",
                op: key == "op",
            })?;
        }
        Ok(())
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut a: A) -> Result<(), A::Error> {
        let mut count = 0;
        while a
            .next_element_seed(Token {
                scan: self.scan,
                depth: self.depth + 1,
                callbacks: false,
                op: false,
            })?
            .is_some()
        {
            count += 1;
            if self.scan.bounded && self.callbacks && count > super::super::LIMIT {
                return Err(de::Error::custom("callback record limit exceeded"));
            }
        }
        Ok(())
    }
}
