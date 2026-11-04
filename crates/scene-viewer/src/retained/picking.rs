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

#[derive(Clone)]
struct State {
    scene: Arc<RenderScene>,
    objects: Arc<BTreeMap<ObjectId, SceneObject>>,
}
struct Snapshot {
    state: Arc<State>,
    view: ResolvedView,
    generation: u64,
}
#[derive(Default)]
pub(super) struct Picking {
    published: Option<Arc<State>>,
    presented: Option<Snapshot>,
    pub(super) windowless: bool,
}

impl Picking {
    pub(super) fn initialize(
        &mut self,
        scene: &RenderScene,
        objects: &BTreeMap<ObjectId, SceneObject>,
    ) {
        self.published = Some(Arc::new(State {
            scene: Arc::new(scene.clone()),
            objects: Arc::new(objects.clone()),
        }));
    }

    fn publish(&mut self, update: RetainedUpdate, scene: Option<Arc<RenderScene>>) {
        let state = Arc::make_mut(self.published.as_mut().expect("resolved initial scene"));
        if let Some(scene) = scene {
            state.scene = scene;
        }
        match update {
            RetainedUpdate::Add { id, object } | RetainedUpdate::Replace { id, object } => {
                apply_edit(
                    Arc::make_mut(&mut state.objects),
                    SceneObjectEdit::Replace { id, object },
                );
            }
            RetainedUpdate::Remove(id) => {
                apply_edit(
                    Arc::make_mut(&mut state.objects),
                    SceneObjectEdit::Remove(id),
                );
            }
            RetainedUpdate::Objects(edits) => {
                let objects = Arc::make_mut(&mut state.objects);
                for edit in edits {
                    apply_edit(objects, edit);
                }
            }
            _ => {}
        }
    }
}
fn apply_edit(objects: &mut BTreeMap<ObjectId, SceneObject>, edit: SceneObjectEdit) {
    match edit {
        SceneObjectEdit::Add { id, object } | SceneObjectEdit::Replace { id, object } => {
            objects.insert(id, object);
        }
        SceneObjectEdit::Remove(id) => {
            objects.remove(&id);
        }
    }
}

impl RetainedHost {
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
        let hit = snapshot.state.scene.pick(
            ray,
            snapshot
                .state
                .objects
                .iter()
                .map(|(&id, &object)| (id, object)),
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
        self.picking.presented = Some(Snapshot {
            state: self
                .picking
                .published
                .as_ref()
                .expect("resolved initial scene")
                .clone(),
            view: *view,
            generation: self.readiness.published,
        });
    }
}
impl RetainedEventHost<'_> {
    /// Event-thread query with the same physical-pixel and presented-frame contract.
    pub fn pick(
        &self,
        point: [f64; 2],
        include: impl FnMut(ObjectId) -> bool,
    ) -> Option<RetainedPickHit> {
        self.host.pick(point, include)
    }
}
