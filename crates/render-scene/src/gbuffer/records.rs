//! The lighting-record table: the constant material values that deferred lighting reads at
//! the index each G-buffer pixel stores in G1.a.
//!
//! A record holds what forward shading reads from the material uniform besides the surface's
//! own channels (`SceneLightingRecord` in `shader/environment.wgsl`): the back-scattering
//! shape, colour and mode, and the transmission amount. The renderer builds one table per
//! uploaded material set. Equal records share an index, index 0 is the neutral standard model
//! (no scattering, no transmission), and every material uniform carries its index in
//! `scattering_mode.z`. G1.a stores the index in eight bits, so a frame that lights the
//! G-buffer admits at most [`LIGHTING_RECORD_LIMIT`] records besides the neutral one; more is
//! an admission error, never a truncated index.
use std::collections::BTreeMap;

use crate::compositor::opaque::OpaqueAdmissionError;
use crate::pipeline::MATERIAL_UNIFORM_BYTES;

/// Distinct records besides the neutral record 0 that the eight-bit G1.a index can address.
pub const LIGHTING_RECORD_LIMIT: u32 = 255;

/// Bytes of one record in the table the lighting pass binds: four `vec4<f32>`.
pub const LIGHTING_RECORD_BYTES: usize = 64;

/// Uniform offsets of the values a record copies, as `geometry.wgsl`'s `Material` lays them
/// out: `scattering_shape`, `scattering_colour` and `scattering_mode` (x and y), and the
/// transmission amount in `geometry_flags.y`.
const SHAPE: usize = 112;
const COLOUR: usize = 128;
const MODE: usize = 144;
const TRANSMISSION: usize = 68;
/// `scattering_mode.z`, which carries the material's index as a float.
const INDEX: usize = 152;

/// One record, as the bit patterns of its sixteen floats in table order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct LightingRecord {
    words: [u32; 16],
}

impl LightingRecord {
    /// The neutral standard model, record 0.
    pub const NEUTRAL: Self = Self { words: [0; 16] };

    /// The record a material uniform describes.
    #[must_use]
    pub fn from_uniform(bytes: &[u8; MATERIAL_UNIFORM_BYTES]) -> Self {
        let word = |offset: usize| {
            u32::from_le_bytes(bytes[offset..offset + 4].try_into().expect("four bytes"))
        };
        let mut words = [0; 16];
        for lane in 0..4 {
            words[lane] = word(SHAPE + lane * 4);
            words[4 + lane] = word(COLOUR + lane * 4);
        }
        words[8] = word(MODE);
        words[9] = word(MODE + 4);
        words[12] = word(TRANSMISSION);
        Self { words }
    }

    /// The record from its sixteen floats, for tests and hosts that author records directly.
    #[must_use]
    pub fn from_floats(values: [f32; 16]) -> Self {
        Self {
            words: values.map(f32::to_bits),
        }
    }

    /// The record's bytes in the table's layout.
    #[must_use]
    pub fn to_bytes(self) -> [u8; LIGHTING_RECORD_BYTES] {
        let mut bytes = [0; LIGHTING_RECORD_BYTES];
        for (chunk, word) in bytes.chunks_exact_mut(4).zip(self.words) {
            chunk.copy_from_slice(&word.to_le_bytes());
        }
        bytes
    }
}

/// The records of one material set, record 0 first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LightingRecordTable {
    records: Vec<LightingRecord>,
    indices: BTreeMap<LightingRecord, u32>,
}

impl Default for LightingRecordTable {
    fn default() -> Self {
        Self {
            records: vec![LightingRecord::NEUTRAL],
            indices: BTreeMap::from([(LightingRecord::NEUTRAL, 0)]),
        }
    }
}

impl LightingRecordTable {
    /// The index of `record`, adding it when no equal record is present.
    pub fn index(&mut self, record: LightingRecord) -> u32 {
        if let Some(&index) = self.indices.get(&record) {
            return index;
        }
        let index = u32::try_from(self.records.len()).expect("a record count fits u32");
        self.records.push(record);
        self.indices.insert(record, index);
        index
    }

    /// Indexes the record of a material uniform and writes the index into it.
    #[expect(
        clippy::cast_precision_loss,
        reason = "one record per material at most, far below the 2^24 a float holds exactly"
    )]
    pub fn assign(&mut self, bytes: &mut [u8; MATERIAL_UNIFORM_BYTES]) -> u32 {
        let index = self.index(LightingRecord::from_uniform(bytes));
        bytes[INDEX..INDEX + 4].copy_from_slice(&(index as f32).to_le_bytes());
        index
    }

    /// Distinct records besides the neutral record 0.
    #[must_use]
    pub fn distinct(&self) -> u32 {
        u32::try_from(self.records.len() - 1).expect("a record count fits u32")
    }

    /// The records in index order.
    #[must_use]
    pub fn records(&self) -> &[LightingRecord] {
        &self.records
    }

    /// Refuses a table that the eight-bit G1.a index cannot address.
    pub fn admit(&self) -> Result<(), OpaqueAdmissionError> {
        let records = self.distinct();
        if records <= LIGHTING_RECORD_LIMIT {
            Ok(())
        } else {
            Err(OpaqueAdmissionError::RecordOverflow {
                records,
                limit: LIGHTING_RECORD_LIMIT,
            })
        }
    }

    /// The table's bytes in index order, as the lighting pass binds them.
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        self.records
            .iter()
            .flat_map(|record| record.to_bytes())
            .collect()
    }
}

#[cfg(test)]
#[path = "records_tests.rs"]
mod tests;
