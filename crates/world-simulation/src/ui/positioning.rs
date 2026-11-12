//! Horizontal pair advances in font units, parsed by ttf-parser. Class matrices stay in the
//! registered font bytes; only the distinct, ordered lookup indices require owned storage.
use std::sync::Arc;
use ttf_parser::{Face, GlyphId, Tag, gpos, kern, opentype_layout::LayoutTable};

const KERN: Tag = Tag::from_bytes(b"kern");

#[derive(Clone, Debug)]
pub(super) struct PairPositioning {
    // Some, including an empty slice, suppresses legacy kern for the entire face.
    lookups: Option<Arc<[u16]>>,
}

impl PairPositioning {
    pub(super) fn new(face: &Face<'_>) -> Self {
        let lookups = face.tables().gpos.and_then(|gpos| {
            gpos.features.into_iter().any(|f| f.tag == KERN).then(|| {
                script_lookups(gpos, Tag::from_bytes(b"DFLT"))
                    .or_else(|| script_lookups(gpos, Tag::from_bytes(b"latn")))
                    .unwrap_or_default()
            })
        });
        Self {
            lookups: lookups.map(Arc::from),
        }
    }

    pub(super) fn storage_bytes(&self) -> usize {
        self.lookups.as_deref().map_or(0, std::mem::size_of_val)
    }

    pub(super) fn scaled<'a>(&'a self, face: &Face<'a>, size: f32) -> ScaledPairs<'a> {
        ScaledPairs {
            positioning: self,
            gpos: face.tables().gpos,
            kern: face.tables().kern,
            pixels_per_unit: size / f32::from(face.units_per_em()),
        }
    }
}

fn script_lookups(gpos: LayoutTable<'_>, tag: Tag) -> Option<Box<[u16]>> {
    let language = gpos.scripts.find(tag)?.default_language?;
    let mut selected = vec![false; usize::from(gpos.lookups.len())];
    let mut reachable = false;
    for index in language
        .required_feature
        .into_iter()
        .chain(language.feature_indices)
    {
        let Some(feature) = gpos.features.get(index).filter(|f| f.tag == KERN) else {
            continue;
        };
        reachable = true;
        for lookup in feature.lookup_indices {
            if let Some(flag) = selected.get_mut(usize::from(lookup)) {
                *flag = true;
            }
        }
    }
    reachable.then(|| {
        (0..gpos.lookups.len())
            .filter(|&index| selected[usize::from(index)])
            .collect()
    })
}

#[derive(Clone, Copy)]
pub(super) struct ScaledPairs<'a> {
    positioning: &'a PairPositioning,
    gpos: Option<LayoutTable<'a>>,
    kern: Option<kern::Table<'a>>,
    pixels_per_unit: f32,
}

impl ScaledPairs<'_> {
    pub(super) fn x_advance(self, first: u16, second: u16) -> f32 {
        self.units(GlyphId(first), GlyphId(second)) * self.pixels_per_unit
    }

    fn units(self, first: GlyphId, second: GlyphId) -> f32 {
        if let Some(indices) = &self.positioning.lookups {
            let gpos = self.gpos.expect("GPOS present when lookups were selected");
            indices
                .iter()
                .filter_map(|&i| gpos.lookups.get(i))
                .filter_map(|lookup| {
                    // A matching zero advance still stops this lookup's subtable search.
                    (0..lookup.subtables.len()).find_map(|i| {
                        match lookup.subtables.get::<gpos::PositioningSubtable<'_>>(i)? {
                            gpos::PositioningSubtable::Pair(pair) => {
                                pair_advance(pair, first, second)
                            }
                            _ => None,
                        }
                    })
                })
                .map(f32::from)
                .sum()
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
