//! Opt-in uniform-grid index over the world positions of active objects.
//!
//! The index never walks the world on its own. Every mutation path the change journal observes
//! also tells the index which objects may have moved, and an object that may have moved leaves
//! the grid until the next refresh. A query combines the grid, which only holds current
//! positions, with a direct evaluation of the few objects waiting for a refresh, so answers are
//! exact at all times. A refresh re-places only those waiting objects.

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

use glam::{Mat4, Vec3};

use crate::journal::Change;
use crate::{ObjectHandle, World, WorldError};

/// Waiting objects beyond this count trigger a refresh from the mutation that exceeded it, which
/// bounds the per-query cost of evaluating them directly.
const AUTO_REFRESH: usize = 4096;
/// Widens the cell range of a radius query so f32 rounding in the distance test can never put a
/// matching object outside the cells that are visited. Extra cells only cost time.
const RADIUS_MARGIN: f64 = 1e-6;

/// The grid coordinate of a cell on each axis, stored as the bits of the floored quotient.
type Cell = [u64; 3];

#[derive(Clone, Copy)]
struct Entry {
    handle: ObjectHandle,
    position: Vec3,
}

#[derive(Clone, Copy)]
enum Shape {
    Sphere { center: Vec3, radius: f32 },
    Box { min: Vec3, max: Vec3 },
}

impl Shape {
    fn contains(self, point: Vec3) -> bool {
        match self {
            Self::Sphere { center, radius } => (point - center).length_squared() <= radius * radius,
            Self::Box { min, max } => point.cmpge(min).all() && point.cmple(max).all(),
        }
    }

    /// Conservative axis-aligned bounds of the shape.
    fn bounds(self) -> ([f64; 3], [f64; 3]) {
        match self {
            Self::Sphere { center, radius } => {
                let reach = f64::from(radius) * (1.0 + RADIUS_MARGIN);
                let c = center.to_array().map(f64::from);
                (c.map(|v| v - reach), c.map(|v| v + reach))
            }
            Self::Box { min, max } => {
                (min.to_array().map(f64::from), max.to_array().map(f64::from))
            }
        }
    }
}

pub(crate) struct SpatialIndex {
    cell_size: f32,
    cells: HashMap<Cell, Vec<Entry>>,
    placed: HashMap<ObjectHandle, (Cell, usize)>,
    parents: HashMap<ObjectHandle, ObjectHandle>,
    children: HashMap<ObjectHandle, HashSet<ObjectHandle>>,
    /// Objects whose indexed position may differ from the grid. None of them is in the grid.
    /// An object here implies its descendants are here too.
    dirty: HashSet<ObjectHandle>,
}

impl SpatialIndex {
    fn new(cell_size: f32) -> Self {
        Self {
            cell_size,
            cells: HashMap::new(),
            placed: HashMap::new(),
            parents: HashMap::new(),
            children: HashMap::new(),
            dirty: HashSet::new(),
        }
    }

    fn axis(&self, value: f64) -> f64 {
        // Adding zero turns -0.0 into 0.0 so equal cells have equal keys.
        (value / f64::from(self.cell_size)).floor() + 0.0
    }

    fn cell_of(&self, point: Vec3) -> Cell {
        point.to_array().map(|v| self.axis(f64::from(v)).to_bits())
    }

    pub(crate) fn observe(&mut self, change: Change) {
        match change {
            Change::Created(object) | Change::Transform(object) | Change::Parent(object) => {
                self.mark(object);
            }
            Change::Destroyed(object) => self.forget(object),
            Change::Properties(_) | Change::Component(..) => {}
        }
    }

    pub(crate) fn mark(&mut self, object: ObjectHandle) {
        let mut pending = vec![object];
        while let Some(next) = pending.pop() {
            if !self.dirty.insert(next) {
                continue;
            }
            self.unplace(next);
            pending.extend(self.children.get(&next).into_iter().flatten().copied());
        }
    }

    fn forget(&mut self, object: ObjectHandle) {
        self.dirty.remove(&object);
        self.unplace(object);
        self.link(object, None);
        self.children.remove(&object);
    }

    fn link(&mut self, object: ObjectHandle, parent: Option<ObjectHandle>) {
        if self.parents.get(&object).copied() == parent {
            return;
        }
        if let Some(old) = self.parents.remove(&object) {
            let emptied = self.children.get_mut(&old).is_some_and(|kids| {
                kids.remove(&object);
                kids.is_empty()
            });
            if emptied {
                self.children.remove(&old);
            }
        }
        if let Some(parent) = parent {
            self.parents.insert(object, parent);
            self.children.entry(parent).or_default().insert(object);
        }
    }

    fn place(&mut self, handle: ObjectHandle, position: Vec3) {
        self.unplace(handle);
        let cell = self.cell_of(position);
        let entries = self.cells.entry(cell).or_default();
        self.placed.insert(handle, (cell, entries.len()));
        entries.push(Entry { handle, position });
    }

    fn unplace(&mut self, handle: ObjectHandle) {
        let Some((cell, slot)) = self.placed.remove(&handle) else {
            return;
        };
        let Some(entries) = self.cells.get_mut(&cell) else {
            return;
        };
        entries.swap_remove(slot);
        if let Some(moved) = entries.get(slot)
            && let Some(location) = self.placed.get_mut(&moved.handle)
        {
            location.1 = slot;
        }
        if entries.is_empty() {
            self.cells.remove(&cell);
        }
    }

    /// Entries of every cell the shape's bounds touch, or of all cells when that range spans
    /// more cells than exist. The flag is true when every cell was scanned.
    fn candidates(&self, shape: Shape) -> (Vec<Entry>, bool) {
        let (low, high) = shape.bounds();
        let low = low.map(|v| self.axis(v));
        let high = high.map(|v| self.axis(v));
        let span: f64 = (0..3).map(|i| high[i] - low[i] + 1.0).product();
        let occupied = u32::try_from(self.cells.len()).map_or(f64::MAX, f64::from);
        if span.partial_cmp(&occupied).is_none_or(Ordering::is_gt) {
            return (self.cells.values().flatten().copied().collect(), true);
        }
        let mut found = Vec::new();
        let mut x = low[0];
        while x <= high[0] {
            let mut y = low[1];
            while y <= high[1] {
                let mut z = low[2];
                while z <= high[2] {
                    let key = [x, y, z].map(f64::to_bits);
                    found.extend(self.cells.get(&key).into_iter().flatten().copied());
                    z += 1.0;
                }
                y += 1.0;
            }
            x += 1.0;
        }
        (found, false)
    }
}

fn require_finite(values: &[Vec3]) -> Result<(), WorldError> {
    if values.iter().all(|v| v.is_finite()) {
        Ok(())
    } else {
        Err(WorldError::InvalidSpatialArgument)
    }
}

impl World {
    /// Starts indexing the world positions of objects that are active in their hierarchy and
    /// published, for [`World::objects_in_radius`], [`World::objects_in_box`] and
    /// [`World::nearest`]. `cell_size` is the grid edge in world units and must be finite and
    /// positive; a size near the typical query radius works best. Costs one pass over the
    /// world now. After that the index follows creation, destruction, transform, parent and
    /// active changes incrementally, so no step rebuilds it.
    ///
    /// Calling it again with the same size does nothing; a different size rebuilds the grid.
    pub fn enable_spatial_index(&mut self, cell_size: f32) -> Result<(), WorldError> {
        if !cell_size.is_finite() || cell_size <= 0.0 {
            return Err(WorldError::InvalidSpatialArgument);
        }
        if self
            .spatial
            .as_ref()
            .is_some_and(|index| index.cell_size.to_bits() == cell_size.to_bits())
        {
            return Ok(());
        }
        let mut index = SpatialIndex::new(cell_size);
        index.dirty.extend(
            self.objects
                .iter(self.identity)
                .map(|(id, _)| ObjectHandle(id)),
        );
        self.spatial = Some(index);
        self.refresh_spatial_index();
        Ok(())
    }

    pub fn has_spatial_index(&self) -> bool {
        self.spatial.is_some()
    }

    /// Moves objects waiting for placement into the grid. Queries are exact without calling
    /// this; it only keeps their cost low. [`World::step_physics`] calls it after each step, and
    /// mutations call it when many objects are waiting.
    pub fn refresh_spatial_index(&mut self) {
        let Some(mut index) = self.spatial.take() else {
            return;
        };
        for object in std::mem::take(&mut index.dirty) {
            match self.live_object(object) {
                Err(_) => index.forget(object),
                Ok(live) => {
                    index.link(object, live.parent);
                    match self.indexed_position(object) {
                        Some(position) => index.place(object, position),
                        None => index.unplace(object),
                    }
                }
            }
        }
        self.spatial = Some(index);
    }

    /// Notes that `object` and its descendants may have moved or changed activity.
    pub(crate) fn spatial_touch(&mut self, object: ObjectHandle) {
        if let Some(index) = &mut self.spatial {
            index.mark(object);
        }
    }

    pub(crate) fn spatial_observe(&mut self, change: Change) {
        let Some(index) = &mut self.spatial else {
            return;
        };
        index.observe(change);
        if index.dirty.len() > AUTO_REFRESH {
            self.refresh_spatial_index();
        }
    }

    /// The world position of `object` when it belongs in the index: live, published, and
    /// active along its whole parent chain, with a finite position.
    fn indexed_position(&self, object: ObjectHandle) -> Option<Vec3> {
        let mut matrix = Mat4::IDENTITY;
        let mut current = Some(object);
        while let Some(handle) = current {
            let live = self.live_object(handle).ok()?;
            if !live.active || self.is_unpublished(handle) {
                return None;
            }
            matrix = self.local_transform(handle).ok()?.matrix() * matrix;
            current = live.parent;
        }
        let position = matrix.w_axis.truncate();
        position.is_finite().then_some(position)
    }

    fn matches(&self, shape: Shape) -> Result<(Vec<Entry>, bool), WorldError> {
        let index = self
            .spatial
            .as_ref()
            .ok_or(WorldError::SpatialIndexDisabled)?;
        let (mut found, scanned_all) = index.candidates(shape);
        found.retain(|entry| shape.contains(entry.position));
        for &object in &index.dirty {
            if let Some(position) = self.indexed_position(object)
                && shape.contains(position)
            {
                found.push(Entry {
                    handle: object,
                    position,
                });
            }
        }
        Ok((found, scanned_all))
    }

    /// Objects whose world position is within `radius` of `center`, in ascending handle order.
    ///
    /// The distance test is `(position - center).length_squared() <= radius * radius` in f32,
    /// so an object exactly `radius` away is included. A `radius` of zero matches objects at
    /// `center`. Only objects that are live, published and active in their hierarchy are
    /// reported; an object waiting for destruction at the end of the frame is still live.
    ///
    /// Fails with [`WorldError::SpatialIndexDisabled`] before
    /// [`World::enable_spatial_index`], and with [`WorldError::InvalidSpatialArgument`] for a
    /// non-finite `center` or a negative or non-finite `radius`.
    pub fn objects_in_radius(
        &self,
        center: Vec3,
        radius: f32,
    ) -> Result<Vec<ObjectHandle>, WorldError> {
        require_finite(&[center])?;
        if !radius.is_finite() || radius < 0.0 {
            return Err(WorldError::InvalidSpatialArgument);
        }
        Ok(sorted_handles(
            self.matches(Shape::Sphere { center, radius })?.0,
        ))
    }

    /// Objects whose world position lies in the box from `min` to `max`, in ascending handle
    /// order. Both corners are inclusive on every axis. A box with `min` above `max` on any
    /// axis is empty. Reports the same objects as [`World::objects_in_radius`].
    ///
    /// Fails with [`WorldError::SpatialIndexDisabled`] before [`World::enable_spatial_index`],
    /// and with [`WorldError::InvalidSpatialArgument`] for a non-finite corner.
    pub fn objects_in_box(&self, min: Vec3, max: Vec3) -> Result<Vec<ObjectHandle>, WorldError> {
        require_finite(&[min, max])?;
        Ok(sorted_handles(self.matches(Shape::Box { min, max })?.0))
    }

    /// The object nearest to `center` among those within `max_radius` (inclusive, tested as in
    /// [`World::objects_in_radius`]) for which `filter` returns true, or `None`. Squared f32
    /// distances that compare equal are ties, and the lower handle wins. Callers exclude a
    /// reference object by filtering it out.
    ///
    /// Fails like [`World::objects_in_radius`].
    pub fn nearest(
        &self,
        center: Vec3,
        max_radius: f32,
        filter: impl Fn(ObjectHandle) -> bool,
    ) -> Result<Option<ObjectHandle>, WorldError> {
        require_finite(&[center])?;
        if !max_radius.is_finite() || max_radius < 0.0 {
            return Err(WorldError::InvalidSpatialArgument);
        }
        let first = self
            .spatial
            .as_ref()
            .map_or(max_radius, |index| index.cell_size);
        // Growing the radius keeps the cost near the distance of the answer. Anything closer
        // than the best match inside the current radius would be inside it too.
        let mut radius = first.min(max_radius);
        loop {
            let (found, scanned_all) = self.matches(Shape::Sphere { center, radius })?;
            let best = found
                .into_iter()
                .filter(|entry| filter(entry.handle))
                .map(|entry| ((entry.position - center).length_squared(), entry.handle))
                .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
            if best.is_some() || radius >= max_radius {
                return Ok(best.map(|(_, handle)| handle));
            }
            radius = if scanned_all {
                max_radius
            } else {
                (radius * 2.0).min(max_radius)
            };
        }
    }
}

fn sorted_handles(mut entries: Vec<Entry>) -> Vec<ObjectHandle> {
    entries.sort_by_key(|entry| entry.handle);
    entries.into_iter().map(|entry| entry.handle).collect()
}
