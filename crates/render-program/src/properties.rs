//! Material and global property values, looked up by name.

use std::collections::BTreeMap;

use crate::resources::Resource;

/// A value a shader property holds.
#[derive(Clone, Debug, PartialEq)]
pub enum PropertyValue {
    Float(f32),
    /// A signed integer. A member that stores an unsigned integer or a boolean accepts the
    /// values that fit it: zero or more for the first, 0 or 1 for the second.
    Int(i32),
    Uint(u32),
    Bool(bool),
    /// A vector or color; a vector of fewer components uses the leading ones.
    Vector([f32; 4]),
    /// Sixteen floats of a 4 by 4 matrix, element `(row, column)` at index `column * 4 + row`:
    /// the logical values, which a constant layout's storage order places in the buffer.
    Matrix([f32; 16]),
    VectorArray(Vec<[f32; 4]>),
    MatrixArray(Vec<[f32; 16]>),
    /// A buffer or texture the runtime owns, for a binding to take.
    Resource(Resource),
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
