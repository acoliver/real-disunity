//! Which feature passes draw in each entry of a pass list, found once per plan instead of by
//! scanning every feature for every pass.
use super::{FeatureDeclaration, FeaturePass, PassDeclaration, PassEntry};
use crate::plan_hash::map_with_capacity;

/// The feature passes of a plan, grouped by the pass list entry they draw in.
///
/// Entries are matched by value, so two passes that name one entry share their feature passes,
/// which the checks of such a list then refuse.
pub(crate) struct EntryFeatures<'a> {
    /// The group of each pass of the list, in list order.
    group_of_pass: Vec<u32>,
    /// The feature passes of an authored entry, ordered by group and, within one, by declaration.
    grouped: Vec<(u32, &'a FeaturePass)>,
    /// The first pass, in list order, that names an entry an earlier pass names.
    repeated: Option<PassEntry>,
    /// The first feature pass, in declaration order, that draws in an entry the list lacks.
    unauthored: Option<PassEntry>,
}

impl<'a> EntryFeatures<'a> {
    pub(crate) fn new(passes: &[PassDeclaration], features: &'a [FeatureDeclaration]) -> Self {
        let mut groups = map_with_capacity(passes.len());
        let mut repeated = None;
        let group_of_pass = passes
            .iter()
            .map(|pass| {
                let next = u32::try_from(groups.len()).expect("bounded pass count");
                let group = *groups.entry(pass.entry()).or_insert(next);
                if group != next && repeated.is_none() {
                    repeated = Some(pass.entry());
                }
                group
            })
            .collect();
        let mut unauthored = None;
        let mut grouped = Vec::new();
        for feature in features.iter().flat_map(|feature| &feature.passes) {
            match groups.get(&feature.entry()) {
                Some(&group) => grouped.push((group, feature)),
                None if unauthored.is_none() => unauthored = Some(feature.entry()),
                None => {}
            }
        }
        grouped.sort_by_key(|&(group, _)| group);
        Self {
            group_of_pass,
            grouped,
            repeated,
            unauthored,
        }
    }

    /// The first pass of the list that repeats an entry, as its entry.
    pub(crate) const fn repeated(&self) -> Option<PassEntry> {
        self.repeated
    }

    /// The entry of the first feature pass that draws in an entry the list does not author.
    pub(crate) const fn unauthored(&self) -> Option<PassEntry> {
        self.unauthored
    }

    /// The feature passes that draw in pass `index` of the list, in declaration order.
    pub(crate) fn of_pass(&self, index: usize) -> impl Iterator<Item = &'a FeaturePass> + '_ {
        let group = self.group_of_pass[index];
        let start = self.grouped.partition_point(|&(other, _)| other < group);
        self.grouped[start..]
            .iter()
            .take_while(move |&&(other, _)| other == group)
            .map(|&(_, feature)| feature)
    }
}

/// The distinct textures of a frame's views, numbered in order of first appearance.
///
/// A frame's views of one texture are usually listed together, so a texture is looked up
/// starting at the one numbered last.
#[derive(Default)]
pub(crate) struct Textures<'a> {
    distinct: Vec<&'a wgpu::Texture>,
    last: usize,
}

impl<'a> Textures<'a> {
    /// The number of `texture`, which is new if the frame has not listed it before.
    pub(crate) fn number(&mut self, texture: &'a wgpu::Texture) -> usize {
        let listed_last = self
            .distinct
            .get(self.last)
            .is_some_and(|other| *other == texture);
        if !listed_last {
            self.last = self
                .distinct
                .iter()
                .position(|other| *other == texture)
                .unwrap_or_else(|| {
                    self.distinct.push(texture);
                    self.distinct.len() - 1
                });
        }
        self.last
    }

    pub(crate) const fn len(&self) -> usize {
        self.distinct.len()
    }
}
