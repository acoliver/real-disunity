use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use std::fmt;
const SAFE: u64 = 9_007_199_254_740_991;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Exact(pub u64);
impl Serialize for Exact {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        if self.0 <= SAFE {
            s.serialize_u64(self.0)
        } else {
            s.serialize_str(&self.0.to_string())
        }
    }
}
impl<'de> Deserialize<'de> for Exact {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        d.deserialize_any(Integer)
    }
}
struct Integer;
impl<'de> de::Visitor<'de> for Integer {
    type Value = Exact;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("safe unsigned integer or canonical decimal string")
    }
    fn visit_u64<E: de::Error>(self, v: u64) -> Result<Exact, E> {
        if v > SAFE {
            return Err(E::custom("unsafe numeric integer"));
        }
        Ok(Exact(v))
    }
    fn visit_str<E: de::Error>(self, v: &str) -> Result<Exact, E> {
        if v.is_empty()
            || v.len() > 20
            || (v.len() > 1 && v.starts_with('0'))
            || !v.bytes().all(|b| b.is_ascii_digit())
        {
            return Err(E::custom("noncanonical integer"));
        }
        v.parse().map(Exact).map_err(E::custom)
    }
}
