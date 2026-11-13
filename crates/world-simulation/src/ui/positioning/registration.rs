use super::{PairSubtable, parse_pair};
use crate::UiError;
use ttf_parser::{Face, Tag, opentype_layout::LayoutTable};

const KERN: Tag = Tag::from_bytes(b"kern");
// Bound scalar reference visits independently of physical bytes: offsets may alias.
// 8192 permits thousands of ordinary feature references without quadratic expansion.
const MAX_REGISTRATION_WORK: usize = 8192;
// Each candidate costs at most one retained descriptor and one probe per pair query.
// Cap potential PairPos slots before parsing any, keeping worst-case queries to 1024 probes.
const MAX_PAIR_SUBTABLES: usize = 1024;

#[derive(Default)]
struct Work(usize);
impl Work {
    fn charge(&mut self, count: usize) -> Result<(), UiError> {
        self.0 = self.0.checked_add(count).ok_or(UiError::PositioningLimit)?;
        if self.0 > MAX_REGISTRATION_WORK {
            return Err(UiError::PositioningLimit);
        }
        Ok(())
    }
}

pub(super) fn resolve(face: &Face<'_>, data: &[u8]) -> Result<Option<Vec<PairSubtable>>, UiError> {
    let gpos = face.tables().gpos.ok_or(UiError::InvalidGpos)?;
    let mut has_kern = false;
    for index in 0..gpos.features.len() {
        let feature = gpos.features.get(index).ok_or(UiError::InvalidGpos)?;
        has_kern |= feature.tag == KERN;
    }
    if !has_kern {
        return Ok(None);
    }
    let mut work = Work::default();
    let indices = script_lookups(gpos, Tag::from_bytes(b"DFLT"), &mut work)?;
    let indices = match indices {
        Some(indices) => indices,
        None => script_lookups(gpos, Tag::from_bytes(b"latn"), &mut work)?.unwrap_or_default(),
    };
    let lookups = preflight(data, &indices, &mut work)?;
    let mut subtables = Vec::new();
    for lookup in &lookups {
        expand(data, lookup, &mut subtables)?;
    }
    Ok(Some(subtables))
}

fn script_lookups(
    gpos: LayoutTable<'_>,
    tag: Tag,
    work: &mut Work,
) -> Result<Option<Vec<u16>>, UiError> {
    let Some(language) = gpos.scripts.find(tag).and_then(|s| s.default_language) else {
        return Ok(None);
    };
    work.charge(
        usize::from(language.feature_indices.len())
            + usize::from(language.required_feature.is_some()),
    )?;
    let mut features = vec![false; usize::from(gpos.features.len())];
    for index in language
        .required_feature
        .into_iter()
        .chain(language.feature_indices)
    {
        *features
            .get_mut(usize::from(index))
            .ok_or(UiError::InvalidGpos)? = true;
    }
    // Count unique feature arrays before walking any lookup references.
    let mut reachable = false;
    for index in 0..gpos.features.len() {
        if features[usize::from(index)] {
            let feature = gpos.features.get(index).ok_or(UiError::InvalidGpos)?;
            if feature.tag == KERN {
                reachable = true;
                work.charge(usize::from(feature.lookup_indices.len()))?;
            }
        }
    }
    let mut selected = vec![false; usize::from(gpos.lookups.len())];
    for index in 0..gpos.features.len() {
        if !features[usize::from(index)] {
            continue;
        }
        let feature = gpos.features.get(index).ok_or(UiError::InvalidGpos)?;
        if feature.tag != KERN {
            continue;
        }
        for lookup in feature.lookup_indices {
            *selected
                .get_mut(usize::from(lookup))
                .ok_or(UiError::InvalidGpos)? = true;
        }
    }
    Ok(reachable.then(|| {
        (0..gpos.lookups.len())
            .filter(|&i| selected[usize::from(i)])
            .collect()
    }))
}

struct SelectedLookup {
    index: u16,
    offset: usize,
    kind: u16,
    count: u16,
}

fn preflight(
    data: &[u8],
    indices: &[u16],
    work: &mut Work,
) -> Result<Vec<SelectedLookup>, UiError> {
    work.charge(indices.len())?;
    let list = relative(data, 0, word(data, 8)?)?;
    let mut potential_pairs = 0_usize;
    let mut lookups = Vec::with_capacity(indices.len());
    for &index in indices {
        let offset = relative(data, list, word(data, list + 2 + usize::from(index) * 2)?)?;
        let kind = word(data, offset)?;
        let count = word(data, offset + 4)?;
        work.charge(usize::from(count))?;
        if kind == 2 || kind == 9 {
            potential_pairs = potential_pairs
                .checked_add(usize::from(count))
                .ok_or(UiError::PositioningLimit)?;
            if potential_pairs > MAX_PAIR_SUBTABLES {
                return Err(UiError::PositioningLimit);
            }
        }
        data.get(offset + 6..offset + 6 + usize::from(count) * 2)
            .ok_or(UiError::InvalidGpos)?;
        lookups.push(SelectedLookup {
            index,
            offset,
            kind,
            count,
        });
    }
    Ok(lookups)
}

fn expand(
    data: &[u8],
    lookup: &SelectedLookup,
    out: &mut Vec<PairSubtable>,
) -> Result<(), UiError> {
    if lookup.kind != 2 && lookup.kind != 9 {
        return Ok(());
    }
    for index in 0..lookup.count {
        let offset = relative(
            data,
            lookup.offset,
            word(data, lookup.offset + 6 + usize::from(index) * 2)?,
        )
        .map_err(|_| {
            if lookup.kind == 9 {
                UiError::InvalidGposExtension
            } else {
                UiError::InvalidGpos
            }
        })?;
        let pair = if lookup.kind == 9 {
            extension_pair(data, offset)?
        } else {
            Some(offset)
        };
        if let Some(offset) = pair {
            let bytes = &data[offset..];
            // Other PairPos formats are unsupported, not candidates for legacy fallback.
            if !matches!(word(bytes, 0)?, 1 | 2) {
                continue;
            }
            parse_pair(bytes).ok_or(UiError::InvalidGpos)?;
            out.push(PairSubtable {
                offset: u32::try_from(offset).map_err(|_| UiError::PositioningLimit)?,
                lookup: lookup.index,
            });
        }
    }
    Ok(())
}

fn extension_pair(data: &[u8], offset: usize) -> Result<Option<usize>, UiError> {
    let validate = || {
        let format = word(data, offset).ok()?;
        let kind = word(data, offset + 2).ok()?;
        let bytes = data.get(offset + 4..offset + 8)?;
        let delta = usize::try_from(u32::from_be_bytes(bytes.try_into().ok()?)).ok()?;
        let inner = offset.checked_add(delta)?;
        if format != 1
            || kind == 9
            || delta == 0
            || data.get(inner..inner.checked_add(2)?).is_none()
        {
            return None;
        }
        Some((kind, inner))
    };
    let (kind, inner) = validate().ok_or(UiError::InvalidGposExtension)?;
    Ok((kind == 2).then_some(inner))
}

fn word(data: &[u8], offset: usize) -> Result<u16, UiError> {
    let bytes = data.get(offset..offset + 2).ok_or(UiError::InvalidGpos)?;
    Ok(u16::from_be_bytes(bytes.try_into().expect("two bytes")))
}

fn relative(data: &[u8], base: usize, delta: u16) -> Result<usize, UiError> {
    let offset = base
        .checked_add(usize::from(delta))
        .ok_or(UiError::InvalidGpos)?;
    if delta == 0 || offset >= data.len() {
        return Err(UiError::InvalidGpos);
    }
    Ok(offset)
}
