//! Direct writes to the global shader state, ordered with the frame's offscreen work.
//!
//! A script that sets a global, renders, and sets the global again expects the render to see the
//! first value. The world stores a direct write in its one global state at once, so reads and the
//! snapshot's starting state see the last value. A host replays the frame's work in order from that
//! starting state, so the render would also see the last value. The world therefore notes each
//! name written directly and, when the next record is appended, puts one ordered
//! [`OffscreenWork::SetGlobal`] or [`OffscreenWork::SetGlobalKeyword`] in front of it for each
//! noted name, holding the value the state has at that point. Several writes to a name between two
//! records give one record, with the last value.
use std::collections::BTreeSet;

use super::{OffscreenWork, PropertyValue, TextureSlot};
use crate::World;

/// The kind of a global value; a name set as two kinds is two values.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Kind {
    Float,
    Int,
    Vector,
    Matrix,
    Texture,
    VectorArray,
    MatrixArray,
}

impl Kind {
    fn of(value: &PropertyValue) -> Self {
        match value {
            PropertyValue::Float(..) => Self::Float,
            PropertyValue::Int(..) => Self::Int,
            PropertyValue::Vector(..) => Self::Vector,
            PropertyValue::Matrix(..) => Self::Matrix,
            PropertyValue::Texture(..) => Self::Texture,
            PropertyValue::VectorArray(..) => Self::VectorArray,
            PropertyValue::MatrixArray(..) => Self::MatrixArray,
        }
    }
}

/// A written global value, by kind and name.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct ValueKey(Kind, String);

impl ValueKey {
    pub(super) fn of(value: &PropertyValue) -> Self {
        Self(Kind::of(value), value.name().to_owned())
    }
}

/// The global values and keywords written directly since the last record was appended or the
/// frame's work was taken. Only the names are kept: the value to order is the state's own when
/// the record is appended, which is the last one written.
#[derive(Clone, Debug)]
pub(super) struct DirectWrites {
    values: BTreeSet<ValueKey>,
    keywords: BTreeSet<String>,
}

impl DirectWrites {
    pub(super) const fn new() -> Self {
        Self {
            values: BTreeSet::new(),
            keywords: BTreeSet::new(),
        }
    }

    pub(super) fn note_value(&mut self, key: ValueKey) {
        self.values.insert(key);
    }

    pub(super) fn note_keyword(&mut self, name: &str) {
        self.keywords.insert(name.to_owned());
    }

    /// Forgets every write: they are ordered, or the frame's starting state holds them.
    pub(super) fn clear(&mut self) {
        self.values.clear();
        self.keywords.clear();
    }

    fn is_empty(&self) -> bool {
        self.values.is_empty() && self.keywords.is_empty()
    }
}

impl World {
    /// The records that put the direct writes since the previous record in front of the next one:
    /// each name's current value, then each keyword's current state. A property
    /// the state no longer holds (it was cleared) has no ordered form and is left out, as the
    /// host reads its absence from the starting state.
    pub(super) fn ordered_global_records(&self) -> Vec<OffscreenWork> {
        let writes = &self.graphics.direct_writes;
        if writes.is_empty() {
            return Vec::new();
        }
        let keywords = &self.graphics.global.keywords;
        let values = writes
            .values
            .iter()
            .filter_map(|ValueKey(kind, name)| self.current_global(*kind, name))
            .map(OffscreenWork::SetGlobal);
        let switches = writes
            .keywords
            .iter()
            .map(|name| OffscreenWork::SetGlobalKeyword {
                name: name.clone(),
                enabled: keywords.binary_search(name).is_ok(),
            });
        values.chain(switches).collect()
    }

    /// The value the global state holds under `name` as `kind`. A render texture destroyed since
    /// it was set reads as no texture, as it does everywhere else.
    fn current_global(&self, kind: Kind, name: &str) -> Option<PropertyValue> {
        let properties = &self.graphics.global.properties;
        let owned = || name.to_owned();
        Some(match kind {
            Kind::Float => PropertyValue::Float(owned(), properties.float(name)?),
            Kind::Int => PropertyValue::Int(owned(), properties.int(name)?),
            Kind::Vector => PropertyValue::Vector(owned(), properties.vector(name)?),
            Kind::Matrix => PropertyValue::Matrix(owned(), properties.matrix(name)?),
            Kind::VectorArray => {
                PropertyValue::VectorArray(owned(), properties.vector_array(name)?)
            }
            Kind::MatrixArray => {
                PropertyValue::MatrixArray(owned(), properties.matrix_array(name)?)
            }
            Kind::Texture => {
                let slot = properties.texture(name)?;
                PropertyValue::Texture(
                    owned(),
                    TextureSlot {
                        texture: self.live_texture(slot.texture),
                        ..slot
                    },
                )
            }
        })
    }
}
