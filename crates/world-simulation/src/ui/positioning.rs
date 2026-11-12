//! Horizontal pair advances in font units. `PairPos` offsets are resolved at registration;
//! class matrices stay in font bytes, and queries never walk the lookup list.
mod registration;
use super::UiError;
use std::sync::Arc;
use ttf_parser::{Face, GlyphId, Tag, gpos, kern, opentype_layout::LookupSubtable};

const GPOS: Tag = Tag::from_bytes(b"GPOS");

#[derive(Clone, Copy, Debug)]
struct PairSubtable {
    offset: u32,
    lookup: u16,
}

#[derive(Clone, Debug)]
pub(super) struct PairPositioning {
    // Some, including an empty slice, suppresses legacy kern for the entire face.
    subtables: Option<Arc<[PairSubtable]>>,
}

impl PairPositioning {
    pub(super) fn new(face: &Face<'_>) -> Result<Self, UiError> {
        let subtables = match face.raw_face().table(GPOS) {
            Some(data) => registration::resolve(face, data)?.map(Arc::from),
            None => None,
        };
        Ok(Self { subtables })
    }

    pub(super) fn storage_bytes(&self) -> usize {
        self.subtables.as_deref().map_or(0, std::mem::size_of_val)
    }

    pub(super) fn scaled<'a>(&'a self, face: &Face<'a>, size: f32) -> ScaledPairs<'a> {
        ScaledPairs {
            positioning: self,
            gpos: face.raw_face().table(GPOS),
            kern: face.tables().kern,
            pixels_per_unit: size / f32::from(face.units_per_em()),
        }
    }
}

fn parse_pair(data: &[u8]) -> Option<gpos::PairAdjustment<'_>> {
    match gpos::PositioningSubtable::parse(data, 2)? {
        gpos::PositioningSubtable::Pair(pair) => Some(pair),
        _ => unreachable!("type 2 parses only PairPos"),
    }
}

#[derive(Clone, Copy)]
pub(super) struct ScaledPairs<'a> {
    positioning: &'a PairPositioning,
    gpos: Option<&'a [u8]>,
    kern: Option<kern::Table<'a>>,
    pixels_per_unit: f32,
}

impl ScaledPairs<'_> {
    pub(super) fn x_advance(self, first: u16, second: u16) -> f32 {
        self.units(GlyphId(first), GlyphId(second)) * self.pixels_per_unit
    }

    fn units(self, first: GlyphId, second: GlyphId) -> f32 {
        if let Some(subtables) = &self.positioning.subtables {
            let data = self
                .gpos
                .expect("GPOS present when subtables were resolved");
            let mut matched_lookup = None;
            let mut advance = 0.;
            for subtable in subtables.iter() {
                if matched_lookup == Some(subtable.lookup) {
                    continue;
                }
                let pair = parse_pair(&data[subtable.offset as usize..])
                    .expect("PairPos validated at registration");
                if let Some(value) = pair_advance(pair, first, second) {
                    advance += f32::from(value);
                    // A matching zero advance also stops this lookup's subtable search.
                    matched_lookup = Some(subtable.lookup);
                }
            }
            advance
        } else {
            self.kern.map_or(0., |kern| {
                kern.subtables
                    .into_iter()
                    .filter(|t| t.horizontal && !t.variable && !t.has_state_machine)
                    .filter_map(|t| t.glyphs_kerning(first, second))
                    .map(f32::from)
                    .sum()
            })
        }
    }
}

fn pair_advance(pair: gpos::PairAdjustment<'_>, first: GlyphId, second: GlyphId) -> Option<i16> {
    let coverage_index = pair.coverage().get(first)?;
    let (first_value, _) = match pair {
        gpos::PairAdjustment::Format1 { sets, .. } => sets.get(coverage_index)?.get(second)?,
        gpos::PairAdjustment::Format2 {
            classes, matrix, ..
        } => matrix.get((classes.0.get(first), classes.1.get(second)))?,
    };
    Some(first_value.x_advance)
}

#[cfg(test)]
mod tests;
