//! Deterministic, bounded quadtree selection with separate split and merge thresholds.

use std::collections::BTreeSet;
use thiserror::Error;
use worldrunner_render_plan::TerrainPageSettings;

#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum PageAddressError {
    #[error("heightfield interval counts must be positive")]
    EmptyTopology,
    #[error("page depth or coordinates lie outside the supported quadtree")]
    Address,
    #[error("the requested page is not an ancestor")]
    NotAncestor,
}

/// A single rectangular Z-up heightfield. Neither axis requires a power-of-two interval count.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PageTopology {
    intervals: [u32; 2],
}

impl PageTopology {
    pub fn new(intervals: [u32; 2]) -> Result<Self, PageAddressError> {
        if intervals.contains(&0) {
            return Err(PageAddressError::EmptyTopology);
        }
        Ok(Self { intervals })
    }
    pub const fn intervals(self) -> [u32; 2] {
        self.intervals
    }
}

/// Quadtree coordinates: eastward columns and northward rows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PageKey {
    depth: u8,
    coordinate: [u32; 2],
}

impl PageKey {
    pub const ROOT: Self = Self {
        depth: 0,
        coordinate: [0, 0],
    };

    pub fn new(depth: u8, coordinate: [u32; 2]) -> Result<Self, PageAddressError> {
        if depth > 16
            || coordinate
                .into_iter()
                .any(|value| value >= (1_u32 << depth))
        {
            return Err(PageAddressError::Address);
        }
        Ok(Self { depth, coordinate })
    }
    pub const fn depth(self) -> u8 {
        self.depth
    }
    pub const fn coordinate(self) -> [u32; 2] {
        self.coordinate
    }

    pub fn ancestor(self, depth: u8) -> Option<Self> {
        if depth > self.depth {
            return None;
        }
        let shift = self.depth - depth;
        Some(Self {
            depth,
            coordinate: self.coordinate.map(|value| value >> shift),
        })
    }

    pub fn neighbor(self, offset: [i32; 2]) -> Option<Self> {
        let coordinate = [
            self.coordinate[0].checked_add_signed(offset[0])?,
            self.coordinate[1].checked_add_signed(offset[1])?,
        ];
        Self::new(self.depth, coordinate).ok()
    }

    /// Maps local page UV into a resident ancestor's interior coordinates.
    pub fn uv_in_ancestor(
        self,
        uv: [f64; 2],
        ancestor: Self,
    ) -> Result<[f64; 2], PageAddressError> {
        if self.ancestor(ancestor.depth) != Some(ancestor) {
            return Err(PageAddressError::NotAncestor);
        }
        let scale = f64::from(1_u32 << (self.depth - ancestor.depth));
        Ok(std::array::from_fn(|axis| {
            (f64::from(self.coordinate[axis]) + uv[axis]) / scale
                - f64::from(ancestor.coordinate[axis])
        }))
    }

    fn children(self) -> [Self; 4] {
        let [x, y] = self.coordinate.map(|value| value * 2);
        [[x, y], [x + 1, y], [x, y + 1], [x + 1, y + 1]].map(|coordinate| Self {
            depth: self.depth + 1,
            coordinate,
        })
    }
}

/// Complete non-overlapping coverage. Only selection can construct the leaf set.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PageSelection {
    leaves: Vec<PageKey>,
}

impl PageSelection {
    pub fn leaves(&self) -> &[PageKey] {
        &self.leaves
    }

    fn split_nodes(&self) -> BTreeSet<PageKey> {
        self.leaves
            .iter()
            .flat_map(|key| {
                (0..key.depth).map(|depth| key.ancestor(depth).expect("shallower depth"))
            })
            .collect()
    }
}

/// Selects material pages using projected texel size supplied by the scene view.
///
/// The greatest error is split first, with page address as a stable tie-breaker.
/// A split costs three additional leaves. Budget pressure never creates holes.
/// Previous internal nodes use the lower merge threshold, preventing oscillation.
pub fn select_pages(
    settings: TerrainPageSettings,
    previous: Option<&PageSelection>,
    mut projected_millipixels: impl FnMut(PageKey) -> u32,
) -> PageSelection {
    let policy = settings.parts();
    let old_splits = previous.map(PageSelection::split_nodes).unwrap_or_default();
    let mut leaves = BTreeSet::from([PageKey::ROOT]);
    let mut candidates = BTreeSet::new();
    add_candidate(
        PageKey::ROOT,
        &old_splits,
        settings,
        &mut projected_millipixels,
        &mut candidates,
    );
    while leaves.len() + 3
        <= usize::try_from(policy.maximum_selected_pages).expect("u32 fits usize")
    {
        let Some((_, std::cmp::Reverse(key))) = candidates.pop_last() else {
            break;
        };
        leaves.remove(&key);
        for child in key.children() {
            leaves.insert(child);
            add_candidate(
                child,
                &old_splits,
                settings,
                &mut projected_millipixels,
                &mut candidates,
            );
        }
    }
    PageSelection {
        leaves: leaves.into_iter().collect(),
    }
}

fn add_candidate(
    key: PageKey,
    old_splits: &BTreeSet<PageKey>,
    settings: TerrainPageSettings,
    projected: &mut impl FnMut(PageKey) -> u32,
    candidates: &mut BTreeSet<(u32, std::cmp::Reverse<PageKey>)>,
) {
    let policy = settings.parts();
    if key.depth == policy.maximum_depth {
        return;
    }
    let error = projected(key);
    let split = if old_splits.contains(&key) {
        error >= policy.merge_threshold_millipixels
    } else {
        error > policy.split_threshold_millipixels
    };
    if split {
        candidates.insert((error, std::cmp::Reverse(key)));
    }
}
