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
//! matrix places it at. A shadow list leaves out a renderer that casts no shadow, and a drawing
//! list leaves out one that only casts shadows (`ShadowsOnly`), which a shadow list keeps. A
//! shadow list tests the sphere only when the split has no planes: the planes of a directional
//! split bound the casters, which reach toward the light past the sphere that wraps the receivers.
//! A shadow list takes its casters from every renderer the culling record's layer mask keeps,
//! those outside the camera's planes included, since a caster out of the camera's view shadows
//! what the camera sees.
//!
//! A drawing list names the `LightMode` values of the passes it draws, in order of preference. A
//! pass with no `LightMode` counts as `SRPDefaultUnlit` for a list (a camera counts it as
//! `Always`), so the Old World shaders whose passes carry no tag are drawn by the lists that name
//! `SRPDefaultUnlit` and left out of the rest.

use std::cmp::Ordering;
use std::sync::Arc;

use glam::Vec3;
use worldrunner_render_program::ids::RendererId;
use worldrunner_render_program::layers::PropertyLayer;
use worldrunner_render_program::unity::offscreen::frame::DrawList;
use worldrunner_render_program::unity::offscreen::frame::Instances;
use worldrunner_render_program::unity::offscreen::frame::ListDraw;
use worldrunner_render_program::unity::offscreen::frame::Record;
use worldrunner_render_program::unity::shader::PassSelector;
use worldrunner_render_program::unity::shader::tag_values_match;
use worldrunner_world_simulation::graphics::RendererListDesc;
use worldrunner_world_simulation::graphics::RendererView;
use worldrunner_world_simulation::shadows::CASTS_NO_SHADOW;
use worldrunner_world_simulation::shadows::SHADOWS_ONLY;

use super::bounds::enclosing_sphere;
use super::bounds::outside_planes;
use super::bounds::touches_sphere;
use super::builtins::view_layer;
use super::frame::LIST_UNTAGGED_LIGHT_MODE;
use super::frame::Mapper;
use super::frame::PASS_TAG;
use super::frame::RendererSkip;
use super::frame::ShaderInfo;
use super::frame::Skip;
use super::frame::fills_target;
use super::light_data::MainLight;
use super::light_data::item_values;
use super::script::Culled;

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
const NO_SHADOW_CAST: &str = "casts no shadow";
const SHADOWS_ONLY_LEFT_OUT: &str = "casts shadows only";

/// The winding that counts as a front face for every draw the player plans: camera renders, lists,
/// mesh and procedural draws. It follows from the clip-space Y the stages write (see
/// `camera::clip_from_gl`).
pub(super) const IMAGE_FRONT_FACE: wgpu::FrontFace = wgpu::FrontFace::Ccw;

impl ShaderInfo {
    /// Whether a pass of the shader has the `LightMode` value `mode`, by the rule that selects
    /// the pass ([`tag_values_match`]).
    pub(super) fn has_light_mode(&self, mode: &str) -> bool {
        self.light_modes
            .iter()
            .any(|held| tag_values_match(held, mode))
    }

    /// The first of a list's `tags` that a pass of the shader has, which names the pass the list
    /// draws. A pass with no `LightMode` has [`LIST_UNTAGGED_LIGHT_MODE`]; a pass tagged with
    /// another value, `Always` included, keeps it. [`list_pass_selector`] picks the pass by the
    /// same rule.
    pub(super) fn list_tag<'t>(&self, tags: &'t [String]) -> Option<&'t String> {
        tags.iter().find(|tag| {
            self.has_light_mode(tag)
                || (self.has_untagged && tag_values_match(LIST_UNTAGGED_LIGHT_MODE, tag))
        })
    }
}

/// The passes a drawing list with `tags` draws: the first pass with the first of the tags that
/// any pass has, a pass with no `LightMode` counting as [`LIST_UNTAGGED_LIGHT_MODE`].
pub(super) fn list_pass_selector(tags: &[String]) -> PassSelector {
    PassSelector::TagWithUntagged {
        key: PASS_TAG.to_owned(),
        values: tags.to_vec(),
        untagged: LIST_UNTAGGED_LIGHT_MODE.to_owned(),
    }
}

/// One draw a list is going to make, before it is ordered.
struct Candidate {
    renderer: usize,
    slot: usize,
    queue: i32,
    distance: f32,
    tag: String,
}

impl<'a> Mapper<'a> {
    /// Keeps the renderers the culling record at `record` admits: those of its set, which the world
    /// fixed when the script culled on the layers of `mask`, that are not wholly outside one of
    /// `planes`. The world makes a set for every cull; a record without one keeps nothing. The
    /// renderers of the mask that the planes drop are kept apart for the shadow lists, which cast
    /// from outside the camera's view; they come from the same set, so a caster that was not
    /// drawable when the record was made stays out.
    pub(super) fn cull(&mut self, record: usize, id: u64, mask: u32, planes: &[[f32; 4]]) {
        let snapshot = self.snapshot;
        let candidates = snapshot.record_renderers(record).unwrap_or_default();
        let mut by_planes = 0;
        let mut culled = Culled::default();
        for &index in candidates {
            let index = index as usize;
            let view = &snapshot.renderers[index];
            if !planes.is_empty() && outside_planes(planes, &self.corners_of(view)) {
                by_planes += 1;
                culled.outside_view.push(index);
            } else {
                culled.kept.push(index);
            }
        }
        let report = &mut self.report.lists;
        report.culls += 1;
        report.kept += culled.kept.len();
        report.out_by_planes += by_planes;
        self.script.culls.insert(id, Arc::new(culled));
        self.script.cull_masks.insert(id, mask);
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
            .map(Arc::clone)
            .ok_or(Skip::UnknownList)?;
        let (target, raster) = self.draw_target()?;
        let (values, eye) = self.current_view()?;
        let mask = self.script.cull_masks.get(cull).copied().unwrap_or(0);
        let main = MainLight::picked_by(&self.snapshot.lights, mask);
        let draws = match &desc {
            RendererListDesc::Draw { .. } => self.draw_items(&culled.kept, &desc, (eye, &main)),
            RendererListDesc::Shadow { .. } => self.shadow_items(&culled, &desc, &main),
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
            raster,
            target,
            values,
            front_face: IMAGE_FRONT_FACE,
            draws,
        }))])
    }

    /// The values of the view the draws at this place use, and where its eye is: the camera the
    /// script set up, with the view and projection it set last, or no values before any setup. A
    /// camera that draws to a part of its target is not drawn.
    pub(super) fn current_view(&self) -> Result<(PropertyLayer, Vec3), Skip> {
        match &self.script.camera {
            Some(setup) if !fills_target(setup.viewport) => Err(Skip::PartialViewport),
            Some(setup) => {
                let values = view_layer(
                    &setup.view,
                    &setup.projection,
                    self.target_size(),
                    setup.half_height,
                );
                Ok((values, setup.view.inverse().w_axis.truncate()))
            }
            None => Ok((PropertyLayer::default(), Vec3::ZERO)),
        }
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
        (eye, main): (Vec3, &MainLight),
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
            list_pass_selector(pass_tags)
        };
        let mut candidates = Vec::new();
        for &index in culled {
            let view = &self.snapshot.renderers[index];
            if view.layer >= 32 || (layer_mask >> view.layer) & 1 == 0 {
                self.left_out(LAYER_MASK, 1);
                continue;
            }
            if view.shadow_casting_mode == SHADOWS_ONLY {
                self.left_out(SHADOWS_ONLY_LEFT_OUT, 1);
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
                let Some((queue, info)) = self.queue_and_modes(view, material) else {
                    self.left_out(MATERIAL_UNAVAILABLE, 1);
                    continue;
                };
                if !(*queue_min..=*queue_max).contains(&queue) {
                    self.left_out(QUEUE_RANGE, 1);
                    continue;
                }
                let tag = if replacement.is_some() {
                    format!("override pass {override_pass}")
                } else if let Some(tag) = info.list_tag(pass_tags) {
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
        self.items(candidates, replacement, (&selector, main))
    }

    /// The draws of the renderers inside a shadow split that cast a shadow and have a pass to draw
    /// a caster: the renderers the culling kept, in the order they were culled, then those of its
    /// layer mask that the camera's planes dropped. A caster outside the camera's view can shadow
    /// what the camera sees, so the camera's planes do not bound a shadow list.
    fn shadow_items(
        &mut self,
        culled: &Culled,
        desc: &RendererListDesc,
        main: &MainLight,
    ) -> Vec<ListDraw> {
        let RendererListDesc::Shadow { planes, sphere, .. } = desc else {
            return Vec::new();
        };
        let selector = PassSelector::Tag {
            key: PASS_TAG.to_owned(),
            values: vec![SHADOW_CASTER.to_owned()],
        };
        let mut candidates = Vec::new();
        let unseen = culled.outside_view.iter().map(|&index| (index, true));
        for (index, outside_view) in culled
            .kept
            .iter()
            .map(|&index| (index, false))
            .chain(unseen)
        {
            if !self.caster_in_split(index, planes, *sphere) || !self.usable_renderer(index) {
                continue;
            }
            let before = candidates.len();
            self.caster_candidates(index, &mut candidates);
            if outside_view && candidates.len() > before {
                self.report.lists.shadow_outside_view += 1;
            }
        }
        self.items(candidates, None, (&selector, main))
    }

    /// The caster items of renderer `index`: one for each material slot whose shader has a pass
    /// that draws a caster. The slots that have none are counted by the reason.
    fn caster_candidates(&mut self, index: usize, candidates: &mut Vec<Candidate>) {
        let view = &self.snapshot.renderers[index];
        let slots = self.frame.renderers[&RendererId(index as u64)]
            .materials
            .len();
        for (slot, material) in view.materials.iter().enumerate().take(slots) {
            let Some(material) = *material else {
                continue;
            };
            let Some((queue, info)) = self.queue_and_modes(view, material) else {
                self.left_out(MATERIAL_UNAVAILABLE, 1);
                continue;
            };
            if !info.has_light_mode(SHADOW_CASTER) {
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

    /// Whether renderer `index` casts a shadow into a split, counting it by the reason when it
    /// does not. The planes bound the casters, so a split with planes tests only them; the sphere
    /// wraps the receivers of a directional split and so serves only a split without planes.
    fn caster_in_split(&mut self, index: usize, planes: &[[f32; 4]], sphere: [f32; 4]) -> bool {
        let view = &self.snapshot.renderers[index];
        if view.shadow_casting_mode == CASTS_NO_SHADOW {
            self.left_out(NO_SHADOW_CAST, 1);
            return false;
        }
        let corners = self.corners_of(view);
        let (centre, radius) = enclosing_sphere(&corners);
        if planes.is_empty() && !touches_sphere(sphere, centre, radius) {
            self.left_out(OUTSIDE_SPHERE, 1);
            return false;
        }
        if outside_planes(planes, &corners) {
            self.left_out(OUTSIDE_PLANES, 1);
            return false;
        }
        true
    }

    /// The list's items for the candidates, counting each by the tag of the pass it will draw.
    fn items(
        &mut self,
        candidates: Vec<Candidate>,
        replacement: Option<worldrunner_render_program::ids::MaterialId>,
        (selector, main): (&PassSelector, &MainLight),
    ) -> Vec<ListDraw> {
        candidates
            .into_iter()
            .map(|candidate| {
                self.drawn_with(&candidate.tag);
                let layer = self.snapshot.renderers[candidate.renderer].layer;
                ListDraw {
                    renderer: RendererId(candidate.renderer as u64),
                    slot: candidate.slot,
                    material: replacement,
                    pass: selector.clone(),
                    instances: Instances::Count(1),
                    per_draw: item_values(main, layer),
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

    /// The render queue a slot's material draws in and what its shader offers, or `None` when the
    /// material or its shader is not available.
    fn queue_and_modes(
        &self,
        view: &RendererView,
        material: worldrunner_world_simulation::graphics::MaterialId,
    ) -> Option<(i32, &'a ShaderInfo)> {
        let (snapshot, available) = (self.snapshot, self.available);
        let held = snapshot.materials.get(&material)?;
        let info = available.shaders.get(&held.shader)?;
        let own = if held.render_queue < 0 {
            info.queue
        } else {
            held.render_queue
        };
        Some((view.render_queue.unwrap_or(own), info))
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
