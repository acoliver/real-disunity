//! Culling results and renderer lists.
//!
//! A culling record names a camera's layer mask and view volume; the host keeps the renderers of
//! the snapshot that it admits. A renderer list is made from one culling result under filters,
//! and the draw record that names it resolves it at its place in the frame: the host takes the
//! culled renderers the filters admit, orders them, and makes one [`DrawList`] record into the
//! targets bound at that place, with the view the script set up last.
//!
//! What the host can and cannot filter on follows what the world records. A renderer has a layer,
//! a matrix, a mesh and a material for each slot, so a list filters on the layer mask, the render
//! queue and the passes the material's shader has. It has no rendering layer mask, sorting layer,
//! static flag or light data, so a list's rendering layer mask, objects filter, light index and
//! per-object data are read and not applied. Planes and a split's sphere are tested against the
//! box of the renderer's mesh, moved by its matrix; a mesh with no box is tested as the point its
//! matrix places it at.

use std::cmp::Ordering;
use std::sync::Arc;

use glam::Vec3;
use worldrunner_render_program::ids::RendererId;
use worldrunner_render_program::layers::PropertyLayer;
use worldrunner_render_program::unity::offscreen::frame::{DrawList, ListDraw, Record};
use worldrunner_render_program::unity::shader::PassSelector;
use worldrunner_world_simulation::graphics::{RendererListDesc, RendererView};

use super::bounds::{enclosing_sphere, outside_planes, touches_sphere};
use super::builtins::view_layer;
use super::frame::{Mapper, PASS_TAG, RendererSkip, Skip, fills_target};

/// The `LightMode` value of the passes that draw a shadow caster into a shadow map. The names of
/// the passes of the Old World player's shaders come from the decoded pass tags, which the report
/// lists as `shader pass tag` in its census; this is the one of them that the shadow lists name.
pub const SHADOW_CASTER: &str = "ShadowCaster";

const LAYER_MASK: &str = "layer mask";
const QUEUE_RANGE: &str = "render queue range";
const NO_TAGGED_PASS: &str = "no pass with the list's tags";
const MATERIAL_UNAVAILABLE: &str = "material or shader unavailable";
const OVERRIDE_UNAVAILABLE: &str = "override material unavailable";
const OUTSIDE_SPHERE: &str = "outside the split's sphere";
const OUTSIDE_PLANES: &str = "outside the split's planes";
const NO_CASTER_PASS: &str = "no shadow caster pass";

/// One draw a list is going to make, before it is ordered.
struct Candidate {
    renderer: usize,
    slot: usize,
    queue: i32,
    distance: f32,
    tag: String,
}

impl<'a> Mapper<'a> {
    /// Keeps the renderers a culling record admits: those whose layer is in `mask` and that are
    /// not wholly outside one of `planes`.
    pub(super) fn cull(&mut self, id: u64, mask: u32, planes: &[[f32; 4]]) {
        let snapshot = self.snapshot;
        let (mut by_layer, mut by_planes) = (0, 0);
        let mut kept = Vec::new();
        for (index, view) in snapshot.renderers.iter().enumerate() {
            if view.layer >= 32 || (mask >> view.layer) & 1 == 0 {
                by_layer += 1;
            } else if !planes.is_empty() && outside_planes(planes, &self.corners_of(view)) {
                by_planes += 1;
            } else {
                kept.push(index);
            }
        }
        let report = &mut self.report.lists;
        report.culls += 1;
        report.kept += kept.len();
        report.out_by_layer += by_layer;
        report.out_by_planes += by_planes;
        self.script.culls.insert(id, kept);
    }

    /// The corners of the box of the renderer's mesh in the world, or the point its matrix places
    /// it at.
    fn corners_of(&self, view: &RendererView) -> Vec<Vec3> {
        match self.available.bounds.get(&view.mesh) {
            Some(bounds) => bounds.corners(&view.local_to_world).to_vec(),
            None => vec![origin_of(view)],
        }
    }

    /// Makes renderer `index` of the snapshot part of the frame when no camera did.
    pub(super) fn list_renderer(&mut self, index: usize) -> Result<(), RendererSkip> {
        if let Some(known) = self.script.renderers.get(&index) {
            return *known;
        }
        let snapshot = self.snapshot;
        let result = match self.renderer(&snapshot.renderers[index]) {
            Ok(renderer) => {
                self.frame
                    .renderers
                    .insert(RendererId(index as u64), renderer);
                Ok(())
            }
            Err(skip) => {
                *self.report.renderers_skipped.entry(skip).or_default() += 1;
                Err(skip)
            }
        };
        self.script.renderers.insert(index, result);
        result
    }

    /// Notes a list for the draw record that names it.
    pub(super) fn create_list(&mut self, id: u64, desc: &RendererListDesc) {
        self.report.lists.created += 1;
        self.script.lists.insert(id, desc.clone());
    }

    /// Resolves list `id` at this place in the frame into a draw record.
    pub(super) fn draw_list(&mut self, id: u64) -> Result<Vec<Record>, Skip> {
        let desc = self
            .script
            .lists
            .get(&id)
            .cloned()
            .ok_or(Skip::UnknownList)?;
        let (RendererListDesc::Draw { cull, .. } | RendererListDesc::Shadow { cull, .. }) = &desc;
        let culled = self
            .script
            .culls
            .get(cull)
            .cloned()
            .ok_or(Skip::UnknownList)?;
        let target = self.script.target.ok_or(Skip::UnboundTarget)?;
        let (values, eye) = match &self.script.camera {
            Some(setup) if !fills_target(setup.viewport) => return Err(Skip::PartialViewport),
            Some(setup) => (
                view_layer(&setup.view, &setup.projection),
                setup.view.inverse().w_axis.truncate(),
            ),
            None => (PropertyLayer::default(), Vec3::ZERO),
        };
        let draws = match &desc {
            RendererListDesc::Draw { .. } => self.draw_items(&culled, &desc, eye),
            RendererListDesc::Shadow { .. } => self.shadow_items(&culled, &desc),
        };
        let lists = &mut self.report.lists;
        if draws.is_empty() {
            lists.empty += 1;
            return Ok(Vec::new());
        }
        if matches!(desc, RendererListDesc::Shadow { .. }) {
            lists.shadow_drawn += 1;
        } else {
            lists.drawn += 1;
        }
        Ok(vec![Record::Draws(Arc::new(DrawList {
            target,
            values,
            // The image is the other way up (see `camera::clip_from_gl`), which reverses a
            // winding, as for a camera render.
            front_face: wgpu::FrontFace::Ccw,
            draws,
        }))])
    }

    fn left_out(&mut self, reason: &str, count: usize) {
        if count > 0 {
            *self
                .report
                .lists
                .left_out
                .entry(reason.to_owned())
                .or_default() += count;
        }
    }

    fn drawn_with(&mut self, tag: &str) {
        *self
            .report
            .lists
            .draws_by_tag
            .entry(tag.to_owned())
            .or_default() += 1;
    }

    /// The draws of the culled renderers that a drawing list admits, in the order it sorts them.
    fn draw_items(
        &mut self,
        culled: &[usize],
        desc: &RendererListDesc,
        eye: Vec3,
    ) -> Vec<ListDraw> {
        let RendererListDesc::Draw {
            pass_tags,
            queue_min,
            queue_max,
            layer_mask,
            sorting,
            override_material,
            override_pass,
            ..
        } = desc
        else {
            return Vec::new();
        };
        let replacement = match override_material.map(|id| self.material(id, None)) {
            None => None,
            Some(Ok(named)) => Some(named),
            Some(Err(_)) => {
                self.left_out(OVERRIDE_UNAVAILABLE, culled.len());
                return Vec::new();
            }
        };
        let selector = if replacement.is_some() {
            PassSelector::Index(usize::try_from(*override_pass).unwrap_or(0))
        } else {
            PassSelector::Tag {
                key: PASS_TAG.to_owned(),
                values: pass_tags.clone(),
            }
        };
        let mut candidates = Vec::new();
        for &index in culled {
            let view = &self.snapshot.renderers[index];
            if view.layer >= 32 || (layer_mask >> view.layer) & 1 == 0 {
                self.left_out(LAYER_MASK, 1);
                continue;
            }
            if !self.usable_renderer(index) {
                continue;
            }
            let slots = self.frame.renderers[&RendererId(index as u64)]
                .materials
                .len();
            for (slot, material) in view.materials.iter().enumerate().take(slots) {
                let Some(material) = *material else {
                    continue;
                };
                let Some((queue, modes)) = self.queue_and_modes(view, material) else {
                    self.left_out(MATERIAL_UNAVAILABLE, 1);
                    continue;
                };
                if !(*queue_min..=*queue_max).contains(&queue) {
                    self.left_out(QUEUE_RANGE, 1);
                    continue;
                }
                let tag = if replacement.is_some() {
                    format!("override pass {override_pass}")
                } else if let Some(tag) = pass_tags.iter().find(|tag| modes.contains(tag)) {
                    tag.clone()
                } else {
                    self.left_out(NO_TAGGED_PASS, 1);
                    continue;
                };
                candidates.push(Candidate {
                    renderer: index,
                    slot,
                    queue,
                    distance: (origin_of(view) - eye).length_squared(),
                    tag,
                });
            }
        }
        sort(&mut candidates, *sorting);
        self.items(candidates, replacement, &selector)
    }

    /// The draws of the culled renderers inside a shadow split that have a pass to draw a
    /// caster, in the order the renderers were culled.
    fn shadow_items(&mut self, culled: &[usize], desc: &RendererListDesc) -> Vec<ListDraw> {
        let RendererListDesc::Shadow { planes, sphere, .. } = desc else {
            return Vec::new();
        };
        let selector = PassSelector::Tag {
            key: PASS_TAG.to_owned(),
            values: vec![SHADOW_CASTER.to_owned()],
        };
        let mut candidates = Vec::new();
        for &index in culled {
            let view = &self.snapshot.renderers[index];
            let corners = self.corners_of(view);
            let (centre, radius) = enclosing_sphere(&corners);
            if !touches_sphere(*sphere, centre, radius) {
                self.left_out(OUTSIDE_SPHERE, 1);
                continue;
            }
            if outside_planes(planes, &corners) {
                self.left_out(OUTSIDE_PLANES, 1);
                continue;
            }
            if !self.usable_renderer(index) {
                continue;
            }
            let slots = self.frame.renderers[&RendererId(index as u64)]
                .materials
                .len();
            for (slot, material) in view.materials.iter().enumerate().take(slots) {
                let Some(material) = *material else {
                    continue;
                };
                let Some((queue, modes)) = self.queue_and_modes(view, material) else {
                    self.left_out(MATERIAL_UNAVAILABLE, 1);
                    continue;
                };
                if !modes.iter().any(|mode| mode == SHADOW_CASTER) {
                    self.left_out(NO_CASTER_PASS, 1);
                    continue;
                }
                candidates.push(Candidate {
                    renderer: index,
                    slot,
                    queue,
                    distance: 0.0,
                    tag: SHADOW_CASTER.to_owned(),
                });
            }
        }
        self.items(candidates, None, &selector)
    }

    /// The list's items for the candidates, counting each by the tag of the pass it will draw.
    fn items(
        &mut self,
        candidates: Vec<Candidate>,
        replacement: Option<worldrunner_render_program::ids::MaterialId>,
        selector: &PassSelector,
    ) -> Vec<ListDraw> {
        candidates
            .into_iter()
            .map(|candidate| {
                self.drawn_with(&candidate.tag);
                ListDraw {
                    renderer: RendererId(candidate.renderer as u64),
                    slot: candidate.slot,
                    material: replacement,
                    pass: selector.clone(),
                }
            })
            .collect()
    }

    /// Whether renderer `index` is in the frame, making it there when no camera did. A renderer
    /// that cannot be made is counted by the reason.
    fn usable_renderer(&mut self, index: usize) -> bool {
        match self.list_renderer(index) {
            Ok(()) => true,
            Err(skip) => {
                self.left_out(&format!("renderer {skip:?}"), 1);
                false
            }
        }
    }

    /// The render queue a slot's material draws in and the `LightMode` values its shader's passes
    /// have, or `None` when the material or its shader is not available.
    fn queue_and_modes(
        &self,
        view: &RendererView,
        material: worldrunner_world_simulation::graphics::MaterialId,
    ) -> Option<(i32, &'a [String])> {
        let (snapshot, available) = (self.snapshot, self.available);
        let held = snapshot.materials.get(&material)?;
        let info = available.shaders.get(&held.shader)?;
        let own = if held.render_queue < 0 {
            info.queue
        } else {
            held.render_queue
        };
        Some((view.render_queue.unwrap_or(own), &info.light_modes))
    }
}

/// Where a renderer's matrix places the origin of its mesh.
fn origin_of(view: &RendererView) -> Vec3 {
    let matrix = &view.local_to_world;
    Vec3::new(matrix[12], matrix[13], matrix[14])
}

/// Orders the candidates by the sorting bits of a list: bit 2 by render queue and, within a
/// queue, bit 4 back to front or else bit 8 front to back by the distance of the renderer's
/// origin from the camera. The other bits (sorting layer, state grouping, canvas order and
/// priority) name keys a renderer here does not have. Equal keys keep the culling order.
fn sort(candidates: &mut [Candidate], sorting: u32) {
    let by_queue = sorting & 2 != 0;
    let back_to_front = sorting & 4 != 0;
    let front_to_back = sorting & 8 != 0 && !back_to_front;
    candidates.sort_by(|a, b| {
        let mut order = Ordering::Equal;
        if by_queue {
            order = a.queue.cmp(&b.queue);
        }
        if order == Ordering::Equal && back_to_front {
            order = b.distance.total_cmp(&a.distance);
        } else if order == Ordering::Equal && front_to_back {
            order = a.distance.total_cmp(&b.distance);
        }
        order
    });
}
