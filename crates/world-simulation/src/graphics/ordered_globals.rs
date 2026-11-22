//! Direct writes to the global shader state, ordered with the frame's offscreen work.
//!
//! A script that sets a global, renders, and sets the global again expects the render to see the
//! first value. The world stores a direct write in its one global state at once, so reads see the
//! last value. A host replays the frame's work in order, so a render that no record of the write
//! precedes would also see the last value. The world therefore notes each
//! name written directly and, when the next record is appended, puts one ordered
//! [`OffscreenWork::SetGlobal`] or [`OffscreenWork::SetGlobalKeyword`] in front of it for each
//! noted name, holding the value the state has at that point. Several writes to a name between two
//! records give one record, with the last value.
//!
//! Writes after the last record are ordered when the work is taken (see
//! [`World::take_offscreen`]), so every write of a take is a record. The state the replay starts
//! from is then the state as it was before the take's first write, which the world copies when
//! that write happens: a draw before a global's first write of the take reads the value the
//! previous take left, as the script saw it.
use std::collections::BTreeSet;

use super::{GlobalShaderState, GraphicsState, OffscreenWork, PropertyValue, TextureSlot};
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

impl GraphicsState {
    /// Keeps the global state as the take's replay starts from it, unless an earlier write of the
    /// take already did. Every change to the state calls this first.
    pub(super) fn keep_global_start(&mut self) {
        if self.global_start.is_none() {
            self.global_start = Some(self.global.clone());
        }
    }

    /// Removes every global property value from the state and from the take's starting copy:
    /// the work list has no record that removes a value, so a cleared name is absent from the
    /// first record of the take on.
    pub(super) fn clear_global_values(&mut self) {
        self.keep_global_start();
        if let Some(start) = &mut self.global_start {
            start.properties = super::ShaderProperties::default();
        }
        self.global.properties = super::ShaderProperties::default();
        self.global.generation += 1;
    }

    /// The state the take's replay starts from: the copy kept before its first write, or
    /// `None` when the take wrote nothing and the state itself is the start.
    pub(super) fn take_global_start(&mut self) -> Option<GlobalShaderState> {
        self.global_start.take()
    }
}

impl World {
    /// The records that put the direct writes since the previous record in front of the next one,
    /// or, when the work is taken, after the last one: each name's current value, then each
    /// keyword's current state. A property the state no longer holds (it was cleared) has no
    /// ordered form and is left out, as the host reads its absence from the starting state.
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
