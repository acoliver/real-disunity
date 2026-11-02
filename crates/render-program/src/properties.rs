//! Material and global property values, looked up by name.

use std::collections::BTreeMap;

/// A value a shader property holds.
#[derive(Clone, Debug, PartialEq)]
pub enum PropertyValue {
    Float(f32),
    /// A vector or color; a vector of fewer components uses the leading ones.
    Vector([f32; 4]),
    /// Sixteen floats, copied to the constant buffer in the order given.
    Matrix([f32; 16]),
    VectorArray(Vec<[f32; 4]>),
    MatrixArray(Vec<[f32; 16]>),
}

/// Named property values, such as a material's or the globals'.
#[derive(Clone, Debug, PartialEq)]
pub struct PropertySet {
    values: BTreeMap<String, PropertyValue>,
}

impl PropertySet {
    #[must_use]
    pub fn new() -> Self {
        Self {
            values: BTreeMap::new(),
        }
    }

    pub fn set(&mut self, name: &str, value: PropertyValue) {
        self.values.insert(name.to_owned(), value);
    }

    #[must_use]
    pub fn get(&self, name: &str) -> Option<&PropertyValue> {
        self.values.get(name)
    }
}

impl Default for PropertySet {
    fn default() -> Self {
        Self::new()
    }
}

/// The value of the first set that has `name`: callers list a material before the globals.
#[must_use]
pub fn lookup<'a>(layers: &[&'a PropertySet], name: &str) -> Option<&'a PropertyValue> {
    layers.iter().find_map(|set| set.get(name))
}
