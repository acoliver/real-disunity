//! Presented CPU geometry. Accepted edits never mutate the displayed snapshot.
use super::{ObjectId, PickHit, RetainedEventHost, RetainedHost, RetainedUpdate, SceneObjectEdit};
use std::{collections::BTreeMap, sync::Arc};
use worldrunner_camera::ResolvedView;
use worldrunner_render_plan::{RenderScene, SceneObject};

/// A geometric hit against the last presented (or windowless captured) generation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RetainedPickHit {
    pub generation: u64,
    pub hit: PickHit,
}

/// What the displayed frame can be asked about.
struct Snapshot {
    scene: Arc<RenderScene>,
    objects: BTreeMap<ObjectId, SceneObject>,
    view: ResolvedView,
    generation: u64,
}
/// The newest published geometry plus the way to bring the displayed snapshot up to it.
/// The snapshot owns its own object table, and presenting replays the edits published since
/// the previous presentation into it, so a frame's cost follows its edits and not the scene.
#[derive(Default)]
pub(super) struct Picking {
    scene: Option<Arc<RenderScene>>,
    objects: BTreeMap<ObjectId, SceneObject>,
    /// Final state of every object edited since the presented snapshot was last brought up to
    /// date; `None` means removed. Coalescing bounds it by the object count.
    since_presented: BTreeMap<ObjectId, Option<SceneObject>>,
    presented: Option<Snapshot>,
    pub(super) windowless: bool,
}

impl Picking {
    pub(super) fn initialize(
        &mut self,
        scene: &RenderScene,
        objects: &BTreeMap<ObjectId, SceneObject>,
    ) {
        self.scene = Some(Arc::new(scene.clone()));
        self.objects = objects.clone();
        self.since_presented.clear();
        self.presented = None;
    }

    fn publish(&mut self, update: RetainedUpdate, scene: Option<Arc<RenderScene>>) {
        if scene.is_some() {
            self.scene = scene;
        }
        match update {
            RetainedUpdate::Add { id, object } | RetainedUpdate::Replace { id, object } => {
                self.apply_edit(SceneObjectEdit::Replace { id, object });
            }
            RetainedUpdate::Remove(id) => self.apply_edit(SceneObjectEdit::Remove(id)),
            RetainedUpdate::Objects(edits) => {
                for edit in edits {
                    self.apply_edit(edit);
                }
            }
            _ => {}
        }
    }

    fn apply_edit(&mut self, edit: SceneObjectEdit) {
        let (id, object) = match edit {
            SceneObjectEdit::Add { id, object } | SceneObjectEdit::Replace { id, object } => {
                (id, Some(object))
            }
            SceneObjectEdit::Remove(id) => (id, None),
        };
        match object {
            Some(object) => self.objects.insert(id, object),
            None => self.objects.remove(&id),
        };
        if self.presented.is_some() {
            self.since_presented.insert(id, object);
        }
    }

    fn present(&mut self, view: &ResolvedView, generation: u64) {
        let mut objects = match self.presented.take() {
            Some(previous) => previous.objects,
            None => self.objects.clone(),
        };
        for (id, object) in std::mem::take(&mut self.since_presented) {
            match object {
                Some(object) => objects.insert(id, object),
                None => objects.remove(&id),
            };
        }
        self.presented = Some(Snapshot {
            scene: self.scene.clone().expect("resolved initial scene"),
            objects,
            view: *view,
            generation,
        });
    }
}

impl RetainedHost {
    /// Camera of the last displayed/captured frame, matching retained picking.
    pub fn presented_view(&self) -> Option<&ResolvedView> {
        self.picking
            .presented
            .as_ref()
            .map(|snapshot| &snapshot.view)
    }

    /// Queries top-left physical window pixels against the last presented frame.
    /// Accepted or renderer-ready edits, resize and camera changes stay invisible
    /// until submission/presentation. Before that first frame, or outside its viewport,
    /// returns None. See render-plan's `PickHit` for geometric coverage limitations.
    pub fn pick(
        &self,
        point: [f64; 2],
        include: impl FnMut(ObjectId) -> bool,
    ) -> Option<RetainedPickHit> {
        let snapshot = self.picking.presented.as_ref()?;
        let viewport = snapshot.view.viewport();
        if !point.iter().all(|v| v.is_finite())
            || point[0] < 0.0
            || point[1] < 0.0
            || point[0] >= f64::from(viewport.width)
            || point[1] >= f64::from(viewport.height)
        {
            return None;
        }
        let ray = snapshot
            .view
            .screen_ray(glam::DVec2::from_array(point).as_vec2())
            .ok()?;
        let hit = snapshot.scene.pick(
            ray,
            snapshot.objects.iter().map(|(&id, &object)| (id, object)),
            include,
        )?;
        snapshot
            .view
            .contains_point(hit.point_meters)
            .then_some(RetainedPickHit {
                generation: snapshot.generation,
                hit,
            })
    }

    pub(crate) fn publish_pick_front(&mut self) {
        let scene = self
            .readiness
            .queue
            .front()
            .expect("published front")
            .pick_scene
            .clone();
        let update = self.readiness.publish_front();
        self.picking.publish(update, scene);
    }

    pub(crate) fn present_captured_pick(&mut self, view: &ResolvedView) {
        if self.picking.windowless {
            self.present_pick(view);
        }
    }

    pub(crate) fn present_pick(&mut self, view: &ResolvedView) {
        self.picking.present(view, self.readiness.published);
    }
}
impl RetainedEventHost<'_> {
    /// Immutable view used by the displayed frame, for annotation extraction.
    pub fn presented_view(&self) -> Option<&ResolvedView> {
        self.host.presented_view()
    }

    /// Event-thread query with the same physical-pixel and presented-frame contract.
    pub fn pick(
        &self,
        point: [f64; 2],
        include: impl FnMut(ObjectId) -> bool,
    ) -> Option<RetainedPickHit> {
        self.host.pick(point, include)
    }
}

#[cfg(test)]
#[path = "picking_tests.rs"]
mod tests;
