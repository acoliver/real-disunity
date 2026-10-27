use super::{
    FrameError, PrepareContext, Rc, RefCell, RenderScene, Residents, SceneGpu, Streaming,
    TerrainChunk, upload_chunk,
};
use std::cell::Cell;
use worldrunner_render_plan::SceneTerrain;

#[derive(Debug)]
pub(super) struct Replacement {
    remaining: Rc<RefCell<Vec<TerrainChunk>>>,
    old: Rc<RefCell<Residents>>,
    source: RenderScene,
    frozen: Rc<Cell<bool>>,
}
impl Replacement {
    fn can_coarsen(&self, allocated: usize, next_limit: u32) -> bool {
        let previous_limit = self
            .source
            .terrain()
            .expect("previous source")
            .streaming_policy()
            .expect("previous policy")
            .maximum_resident_chunks();
        allocated < next_limit.max(previous_limit) as usize
    }
}
impl Drop for Replacement {
    fn drop(&mut self) {
        self.frozen.set(false);
    }
}
impl Streaming {
    pub(in crate::draw) fn replacement(scene: RenderScene, old: &Self) -> Option<Self> {
        let terrain = scene.terrain()?;
        let policy = terrain.streaming_policy()?;
        let source = old.scene.terrain().expect("streaming source");
        let residents = old.committed.borrow().clone();
        let compatible = same_grid(source, terrain);
        let all = source.elevation_bounds() != terrain.elevation_bounds();
        let remaining = if compatible {
            residents
                .iter()
                .filter_map(|(chunk, _)| {
                    (all || span_changed(source, terrain, *chunk)).then_some(*chunk)
                })
                .collect()
        } else {
            vec![TerrainChunk::root(terrain, policy)]
        };
        let reused = residents
            .iter()
            .filter(|(chunk, _)| compatible && !remaining.contains(chunk))
            .cloned()
            .collect();
        *old.drawing.borrow_mut() = None;
        old.frozen.set(true);
        Some(Self {
            scene,
            identity: Rc::new(()),
            committed: Rc::new(RefCell::new(reused)),
            drawing: RefCell::new(None),
            retired: old.retired.clone(),
            authority: old.authority.clone(),
            frozen: Rc::default(),
            replacement: RefCell::new(Some(Replacement {
                remaining: Rc::new(RefCell::new(remaining)),
                old: old.committed.clone(),
                source: old.scene.clone(),
                frozen: old.frozen.clone(),
            })),
        })
    }
    pub(super) fn prepare_replacement(
        &self,
        context: &mut PrepareContext<'_>,
    ) -> Result<bool, FrameError> {
        let replacement = self.replacement.borrow();
        let Some(replacement) = replacement.as_ref() else {
            return Ok(true);
        };
        let terrain = self.scene.terrain().expect("source");
        let policy = terrain.streaming_policy().expect("policy");
        let remaining = replacement.remaining.borrow();
        if remaining.is_empty() {
            return Ok(true);
        }
        let shared = self
            .committed
            .borrow()
            .iter()
            .filter(|(chunk, lod)| {
                replacement
                    .old
                    .borrow()
                    .iter()
                    .any(|(old, gpu)| chunk == old && lod.vertices == gpu.vertices)
            })
            .count();
        self.retired
            .borrow_mut()
            .retain(|(serial, _)| *serial > self.authority.completed_serial());
        let retired: usize = self
            .retired
            .borrow()
            .iter()
            .map(|(_, chunks)| chunks.len())
            .sum();
        let allocated =
            replacement.old.borrow().len() + self.committed.borrow().len() - shared + retired;
        if allocated + remaining.len() > policy.maximum_resident_chunks() as usize
            && replacement.old.borrow().len() > 1
        {
            if replacement.can_coarsen(allocated, policy.maximum_resident_chunks()) {
                self.coarsen_previous(context, replacement)?;
            }
            return Ok(false);
        }
        let count = remaining
            .len()
            .min(policy.uploads_per_frame() as usize)
            .min((policy.maximum_resident_chunks() as usize).saturating_sub(allocated));
        let mut residents = self.committed.borrow().clone();
        for chunk in remaining.iter().take(count) {
            residents.push((*chunk, upload_chunk(context, terrain, *chunk)?));
        }
        let pending = replacement.remaining.clone();
        let destination = self.committed.clone();
        crate::compositor::publication::stage(context, &self.committed, move || {
            pending.borrow_mut().drain(..count);
            *destination.borrow_mut() = residents;
        });
        Ok(false)
    }
    fn coarsen_previous(
        &self,
        context: &mut PrepareContext<'_>,
        replacement: &Replacement,
    ) -> Result<(), FrameError> {
        let terrain = replacement.source.terrain().expect("previous source");
        let chunk = TerrainChunk::root(terrain, terrain.streaming_policy().expect("policy"));
        let lod = upload_chunk(context, terrain, chunk)?;
        let previous = replacement.old.clone();
        let retired = self.retired.clone();
        let authority = self.authority.clone();
        crate::compositor::publication::stage(context, &replacement.old, move || {
            let removed = std::mem::replace(&mut *previous.borrow_mut(), vec![(chunk, lod)]);
            retired
                .borrow_mut()
                .push((authority.submitted_serial(), removed));
        });
        Ok(())
    }
    pub(super) fn replacement_ready(&self) -> bool {
        self.replacement
            .borrow()
            .as_ref()
            .is_none_or(|replacement| replacement.remaining.borrow().is_empty())
    }
    pub(super) fn finish_replacement(&self) {
        let Some(replacement) = self.replacement.borrow_mut().take() else {
            return;
        };
        assert!(replacement.remaining.borrow().is_empty());
        let residents = self.committed.borrow();
        let removed = replacement
            .old
            .borrow()
            .iter()
            .filter(|(_, old)| {
                !residents
                    .iter()
                    .any(|(_, next)| next.vertices == old.vertices)
            })
            .cloned()
            .collect::<Residents>();
        if !removed.is_empty() {
            self.retired
                .borrow_mut()
                .push((self.authority.submitted_serial(), removed));
        }
    }
}
fn same_grid(a: &SceneTerrain, b: &SceneTerrain) -> bool {
    a.columns() == b.columns()
        && a.rows() == b.rows()
        && a.streaming_policy() == b.streaming_policy()
        && a.lod_policy() == b.lod_policy()
        && a.origin_east_millimeters() == b.origin_east_millimeters()
        && a.origin_north_millimeters() == b.origin_north_millimeters()
        && a.sample_spacing_millimeters() == b.sample_spacing_millimeters()
}
fn span_changed(a: &SceneTerrain, b: &SceneTerrain, chunk: TerrainChunk) -> bool {
    let minimum = chunk.minimum.map(|v| v.saturating_sub(chunk.stride));
    let maximum = [
        chunk.maximum[0]
            .saturating_add(chunk.stride)
            .min(a.columns() - 1),
        chunk.maximum[1]
            .saturating_add(chunk.stride)
            .min(a.rows() - 1),
    ];
    (minimum[1]..=maximum[1]).any(|row| {
        let start = row as usize * a.columns() as usize + minimum[0] as usize;
        let end = row as usize * a.columns() as usize + maximum[0] as usize + 1;
        a.heights_millimeters()[start..end] != b.heights_millimeters()[start..end]
    })
}
impl SceneGpu {
    /// Advances bounded replacement uploads without publishing a partial terrain.
    pub fn prepare_retained_terrain(
        &self,
        context: &mut PrepareContext<'_>,
    ) -> Result<bool, FrameError> {
        self.terrain
            .as_ref()
            .and_then(|t| t.streaming.as_ref())
            .map_or(Ok(true), |s| s.prepare_replacement(context))
    }
    /// Whether all retained replacement chunks have been submitted.
    pub fn retained_terrain_ready(&self) -> bool {
        self.terrain
            .as_ref()
            .and_then(|t| t.streaming.as_ref())
            .is_none_or(Streaming::replacement_ready)
    }
    /// Releases the preceding source only after the complete generation is ready.
    pub fn finish_retained_terrain(&self) {
        if let Some(streaming) = self.terrain.as_ref().and_then(|t| t.streaming.as_ref()) {
            streaming.finish_replacement();
        }
    }
}
