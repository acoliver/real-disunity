//! The steps of a frame, in call order, before the constants buffer exists: camera passes with
//! their draws, blits and copies.

use std::collections::BTreeMap;
use std::ops::Range;

use worldrunner_render_post::StageId;
use worldrunner_render_post::contracts::{
    AllocationKey, BufferUse, BufferUseKind, DrawGeometry, IndexBinding, RasterId, RasterPass,
    ResourceLifetime, ResourceUse, TypedOperation,
};

use crate::ids::{CameraId, MaterialId, MeshId, ShaderId, TextureId};
use crate::layers::{Layers, PropertyLayer};
use crate::packet::TargetSignature;
use crate::unity::shader::{PassSelector, ResolveError};

use super::blit_steps::{BlitStep, blit_step, copy_step};
use super::dots::{self, DotsDraw};
use super::error::{OffscreenError, Refusal};
use super::frame::{Attachment, Attachments, Camera, Material, OffscreenFrame, Record, SubMesh};
use super::list_steps::{self, Loads, Written};
use super::mips;
use super::pass_report::{pass_report, program_probes};
use super::plan::{DrawPlan, DrawSpec, Services, VertexPlan};
use super::raster::PassRaster;
use super::readback::TapRead;
use super::resample::ResampleStep;
use super::tap;
use super::tracker::Tracker;
use super::{CameraCauses, CameraDraws, UnmatchedPasses};

/// One draw of a camera's pass.
pub(super) struct CameraDraw {
    pub(super) shader: ShaderId,
    /// The mesh the draw reads, or `None` for a procedural draw, whose shader makes its vertices.
    pub(super) mesh: Option<MeshId>,
    pub(super) topology: wgpu::PrimitiveTopology,
    pub(super) plan: DrawPlan,
    pub(super) vertices: VertexPlan,
    pub(super) geometry: DrawGeometry,
    pub(super) signature: TargetSignature,
    pub(super) front_face: wgpu::FrontFace,
    pub(super) index_use: Option<BufferUse>,
    /// The scissor and depth bias the record that made the draw set.
    pub(super) raster: PassRaster,
}

pub(super) enum Step {
    Ready(TypedOperation),
    Camera {
        id: RasterId,
        pass: RasterPass,
        draws: Vec<CameraDraw>,
    },
    Blit(BlitStep),
    Resample(Box<ResampleStep>),
}

impl Step {
    /// The draws the step holds planned until it is finished.
    pub(super) fn planned_draws(&self) -> usize {
        match self {
            Self::Camera { draws, .. } => draws.len(),
            Self::Blit(_) => 1,
            Self::Resample(_) | Self::Ready(_) => 0,
        }
    }
}

/// The global values and keywords the draws read: the frame's own until a record replaces them.
#[derive(Clone, Copy)]
pub(super) struct Globals<'a> {
    pub(super) layer: &'a PropertyLayer,
    pub(super) keywords: &'a [String],
}

/// How the draws of one pass look: which pass of a shader draws, the view's values, the winding,
/// the globals and the target's formats.
#[derive(Clone, Copy)]
pub(super) struct Look<'a> {
    pub(super) selector: &'a PassSelector,
    pub(super) values: &'a PropertyLayer,
    /// The values of the one draw, above its renderer's own: a list item's.
    pub(super) own: Option<&'a PropertyLayer>,
    pub(super) front_face: wgpu::FrontFace,
    pub(super) globals: Globals<'a>,
    pub(super) signature: TargetSignature,
    /// What the pass writes, which its draws may not sample.
    pub(super) written: &'a [Written],
    /// The instances of a raw buffer the draw makes, when it makes them.
    pub(super) dots: Option<DotsDraw<'a>>,
}

pub(super) fn raster_id(name: &'static str, index: usize) -> RasterId {
    RasterId::new(
        StageId::new(name, 0),
        u32::try_from(index).unwrap_or(u32::MAX),
    )
}

/// Plans each record of the frame into a step, with the reads the frame's taps ask for placed
/// after the records they name.
///
/// # Errors
/// Fails with the first record the state refuses, which the refusal names.
pub(super) fn plan_steps(
    svc: &mut Services<'_>,
    frame: &OffscreenFrame,
) -> Result<(Vec<Step>, Vec<TapRead>), Refusal> {
    let mut steps = Vec::new();
    let mut taps = Vec::new();
    let mut globals = Globals {
        layer: &frame.globals,
        keywords: &frame.global_keywords,
    };
    for (index, record) in frame.records.iter().enumerate() {
        plan_record(svc, frame, (index, record), &mut globals, &mut steps)
            .map_err(|error| Refusal::at(index, error))?;
        taps.extend(tap::plan_taps(svc, frame, index, &mut steps));
    }
    taps.extend(tap::past_the_end(frame));
    Ok((steps, taps))
}

/// Plans one record into the steps it adds to `steps`: none for a record of globals or one that
/// does nothing, which replaces `globals` for the records after it.
fn plan_record<'a>(
    svc: &mut Services<'_>,
    frame: &'a OffscreenFrame,
    (index, record): (usize, &'a Record),
    globals: &mut Globals<'a>,
    steps: &mut Vec<Step>,
) -> Result<(), OffscreenError> {
    let in_force = *globals;
    let step = match record {
        Record::Globals(state) => {
            *globals = Globals {
                layer: &state.values,
                keywords: &state.keywords,
            };
            return Ok(());
        }
        Record::CameraRender { camera, target } => {
            camera_step(svc, frame, index, in_force, (*camera, *target))?
        }
        Record::Blit {
            source,
            target,
            material,
            pass,
        } => blit_step(
            svc,
            frame,
            index,
            in_force,
            (*source, *target),
            (*material, *pass),
        )?,
        Record::CopyTexture { .. } => Step::Ready(copy_step(svc, index, record)?),
        Record::GenerateMips { texture } => {
            steps.extend(mips::generate_steps(svc, steps.len(), *texture)?);
            return Ok(());
        }
        Record::Clear { target, clear } => {
            match list_steps::clear_step(svc, index, target.clone(), clear)? {
                Some(step) => step,
                None => return Ok(()),
            }
        }
        Record::Draws(list) => list_steps::list_step(svc, frame, index, in_force, list)?,
        Record::Procedural(draw) => {
            super::procedural::procedural_step(svc, frame, index, in_force, draw)?
        }
    };
    steps.push(step);
    Ok(())
}

fn camera_step(
    svc: &mut Services<'_>,
    frame: &OffscreenFrame,
    index: usize,
    globals: Globals<'_>,
    (camera_id, target): (CameraId, TextureId),
) -> Result<Step, OffscreenError> {
    let camera = frame
        .cameras
        .get(&camera_id)
        .ok_or_else(|| OffscreenError::Unknown(format!("camera {camera_id:?}")))?;
    require_render_texture(svc, target)?;
    let signature = svc.signature(target)?;
    let cells = list_steps::written(svc, &whole_target(target))?;
    let mut draws = Vec::new();
    let counted_before = frame
        .probe
        .test_targets
        .contains(&target)
        .then(|| svc.counts.clone());
    let unmatched_before = svc.counts.unmatched;
    let missing_before = svc.counts.missing_texture;
    let channel_before = svc.counts.missing_channel;
    let feedback_before = svc.counts.feedback_loop;
    svc.counts.current = CameraCauses::default();
    let outer_missing = std::mem::take(svc.missing);
    svc.probing = frame.probe.targets.contains(&target);
    let variant = frame.probe.variants.get(&target).copied();
    svc.constant_fragment = variant.is_some_and(|variant| variant.constant_fragment);
    let look = Look {
        selector: &camera.pass,
        values: &camera.values,
        own: None,
        front_face: camera.front_face,
        globals,
        signature,
        written: &cells,
        dots: None,
    };
    for visible in visible_draws(svc, frame, camera)? {
        if let Some(mut draw) = planned_draw(svc, frame, &look, visible)? {
            if let Some(variant) = variant {
                variant.apply(&mut draw.plan.state);
            }
            draws.push(draw);
        }
    }
    svc.constant_fragment = false;
    let probed = std::mem::take(&mut svc.probing);
    svc.counts.cameras.push(CameraDraws {
        camera: camera_id,
        target,
        drawn: draws.len(),
        unmatched: svc.counts.unmatched - unmatched_before,
        missing_texture: svc.counts.missing_texture - missing_before,
        missing_channel: svc.counts.missing_channel - channel_before,
        feedback_loop: svc.counts.feedback_loop - feedback_before,
    });
    let causes = std::mem::take(&mut svc.counts.current);
    svc.counts.camera_causes.push(causes);
    let without_value = std::mem::replace(svc.missing, outer_missing);
    let mut report = pass_report((camera_id, target), (&*svc, camera), &draws, &without_value);
    if probed {
        report.programs = program_probes(svc, &frame.probe, &draws);
    }
    svc.counts.passes.push(report);
    svc.missing.extend(without_value);
    if let Some(before) = counted_before {
        svc.counts.forget_since(before);
    }
    let pass = camera_pass(
        svc,
        raster_id("offscreen camera", index),
        camera,
        target,
        &draws,
    )?;
    Ok(Step::Camera {
        id: pass.id,
        pass,
        draws,
    })
}

/// The layers of a camera draw's values, highest first: the names of the places in an
/// [`Origin::Layer`](super::Origin).
pub const DRAW_LAYERS: [&str; 8] = [
    "per-draw",
    "camera",
    "property block",
    "material",
    "material fallback",
    "globals",
    "declared default below globals",
    "size of the bound texture",
];

/// The depth a camera that clears depth clears to.
pub(super) const DEPTH_CLEAR: f32 = 1.0;

/// Plans one sub-mesh the camera sees as (material, renderer, slot). A sub-mesh with no
/// indices, a draw whose shader has no pass the camera selects, a draw whose pass samples a
/// texture no layer names and a draw whose mesh lacks a channel the pass reads give none; the
/// last three are counted.
pub(super) fn planned_draw(
    svc: &mut Services<'_>,
    frame: &OffscreenFrame,
    look: &Look<'_>,
    (material_id, renderer_id, slot): (MaterialId, crate::ids::RendererId, usize),
) -> Result<Option<CameraDraw>, OffscreenError> {
    let renderer = &frame.renderers[&renderer_id];
    let material = &frame.materials[&material_id];
    let shader = material
        .shader
        .ok_or(OffscreenError::NoShader(material_id))?;
    let layers: Vec<&PropertyLayer> = [
        look.own,
        Some(&renderer.per_draw),
        Some(look.values),
        renderer.slot_blocks.get(&slot),
        Some(&renderer.block),
        Some(&material.properties),
        Some(&material.fallback),
        Some(look.globals.layer),
        Some(&material.below_globals),
    ]
    .into_iter()
    .flatten()
    .collect();
    let mut keywords = keywords(material, look.globals);
    if look.dots.is_some() {
        dots::select_variant(&mut keywords);
    }
    let spec = DrawSpec {
        shader,
        keywords,
        selector: look.selector.clone(),
        layers: Layers::new(&layers),
        dots: look.dots,
    };
    let Some(sub) = svc.meshes.get(renderer.mesh)?.sub_meshes.get(slot).copied() else {
        return Ok(None);
    };
    if sub.count == 0 {
        return Ok(None);
    }
    // A draw left out after its constants were packed leaves none of them in the frame.
    let constants_mark = svc.constants.mark();
    let planned = camera_draw(
        svc,
        &spec,
        renderer.mesh,
        sub,
        (look.signature, look.front_face),
    )
    .and_then(|draw| list_steps::refuse_feedback(draw, look.written));
    if planned.is_err() {
        svc.constants.rollback(constants_mark);
    }
    counted(svc, &spec, planned)
}

/// The draw `planned` made, or `None` when it was left out for a cause the report counts: no
/// pass the selector picks, a texture or structured buffer no layer names, a mesh channel the
/// pass reads or a texture the pass samples and draws into. Other errors fail the frame.
pub(super) fn counted(
    svc: &mut Services<'_>,
    spec: &DrawSpec<'_>,
    planned: Result<CameraDraw, OffscreenError>,
) -> Result<Option<CameraDraw>, OffscreenError> {
    match planned {
        Ok(draw) => {
            svc.counts.note_defaulted(&draw.vertices.defaulted);
            svc.counts.note_stand_ins(&draw.plan.stand_ins);
            svc.keep_unset(&draw.plan);
            Ok(Some(draw))
        }
        Err(OffscreenError::Resolve(ResolveError::NoPass { .. })) => {
            svc.counts.unmatched += 1;
            left_out(svc, spec.shader, &spec.selector);
            Ok(None)
        }
        Err(OffscreenError::MissingTexture { name }) => {
            svc.counts.missing_texture += 1;
            *svc.counts.current.textures.entry(name.clone()).or_default() += 1;
            *svc.counts.missing_textures.entry(name).or_default() += 1;
            Ok(None)
        }
        Err(OffscreenError::MissingBuffer { name }) => {
            *svc.counts.missing_buffers.entry(name).or_default() += 1;
            Ok(None)
        }
        Err(OffscreenError::MissingChannel { channel, input, .. }) => {
            svc.counts.missing_channel += 1;
            *svc.counts
                .current
                .channels
                .entry((channel, input))
                .or_default() += 1;
            *svc.counts
                .missing_channels
                .entry((channel, input))
                .or_default() += 1;
            Ok(None)
        }
        Err(OffscreenError::FeedbackLoop(texture)) => {
            svc.counts.feedback_loop += 1;
            *svc.counts.feedback_loops.entry(texture).or_default() += 1;
            Ok(None)
        }
        Err(error) => Err(error),
    }
}

/// Counts a draw left out because `shader` has no pass `selector` selects.
fn left_out(svc: &mut Services<'_>, shader: ShaderId, selector: &PassSelector) {
    let (key, selected) = match selector {
        PassSelector::Tag { key, values } | PassSelector::TagWithUntagged { key, values, .. } => {
            (key.clone(), values.clone())
        }
        PassSelector::Index(index) => ("index".to_owned(), vec![index.to_string()]),
    };
    let (passes, tags) = svc
        .shaders
        .get(&shader)
        .map_or((0, Vec::new()), |shader| shader.pass_tags(&key));
    let found = svc.counts.unmatched_passes.iter_mut().find(|entry| {
        entry.key == key
            && entry.selected == selected
            && entry.tags == tags
            && entry.passes == passes
    });
    if let Some(entry) = found {
        entry.draws += 1;
    } else {
        svc.counts.unmatched_passes.push(UnmatchedPasses {
            key,
            selected,
            tags,
            passes,
            draws: 1,
        });
    }
}

/// The material and renderer of every sub-mesh the camera sees, as (material, renderer, slot), in
/// render queue order. Equal queues keep the renderers' order. Nothing is sorted front to back.
fn visible_draws(
    svc: &Services<'_>,
    frame: &OffscreenFrame,
    camera: &Camera,
) -> Result<Vec<(MaterialId, crate::ids::RendererId, usize)>, OffscreenError> {
    let mut draws = Vec::new();
    for (renderer_id, renderer) in &frame.renderers {
        if !camera.draws(*renderer_id, renderer.layer) {
            continue;
        }
        let mesh = svc.meshes.get(renderer.mesh)?;
        for slot in 0..mesh.sub_meshes.len().min(renderer.materials.len()) {
            let material_id = renderer.materials[slot];
            let material = frame
                .materials
                .get(&material_id)
                .ok_or(OffscreenError::UnknownMaterial(material_id))?;
            draws.push((material.render_queue, material_id, *renderer_id, slot));
        }
    }
    draws.sort_by_key(|draw| draw.0);
    Ok(draws
        .into_iter()
        .map(|(_, material, renderer, slot)| (material, renderer, slot))
        .collect())
}

pub(super) fn keywords<'a>(material: &'a Material, globals: Globals<'a>) -> Vec<&'a str> {
    material
        .keywords
        .iter()
        .chain(globals.keywords)
        .map(String::as_str)
        .collect()
}

pub(super) fn camera_draw(
    svc: &mut Services<'_>,
    spec: &DrawSpec<'_>,
    mesh: MeshId,
    sub: SubMesh,
    (signature, front_face): (TargetSignature, wgpu::FrontFace),
) -> Result<CameraDraw, OffscreenError> {
    let plan = svc.plan_program(spec)?;
    let vertices = svc.plan_vertices(mesh, &plan.entry)?;
    let info = svc.meshes.get(mesh)?;
    let buffer = svc.runtime.gpu_buffer(info.buffer)?;
    let key = svc
        .tracker
        .import_buffer(&buffer, ResourceLifetime::ImportedReadOnly, Some(0))?;
    let (index, index_use) = match &info.index {
        Some(index) => {
            let view = svc.tracker.buffer_view(key, index.range.clone(), 0)?;
            (
                Some(IndexBinding {
                    view: view.clone(),
                    format: index.format,
                }),
                Some(BufferUse::read(view, BufferUseKind::Index)),
            )
        }
        None => (None, None),
    };
    let geometry = DrawGeometry {
        vertex_buffers: vertices.views.clone(),
        index,
        elements: sub.first..sub.first + sub.count,
        base_vertex: sub.base_vertex,
        instances: 0..1,
    };
    Ok(CameraDraw {
        shader: spec.shader,
        mesh: Some(mesh),
        topology: wgpu::PrimitiveTopology::TriangleList,
        plan,
        vertices,
        geometry,
        signature,
        front_face,
        index_use,
        raster: PassRaster::default(),
    })
}

pub(super) fn require_render_texture(
    svc: &Services<'_>,
    id: TextureId,
) -> Result<(), OffscreenError> {
    if svc.textures.get(id)?.spec.is_none() {
        return Err(OffscreenError::NotRenderTexture(id));
    }
    Ok(())
}

/// The raster pass of a camera: clears or loads its target and depth, and reads every texture its
/// draws sample.
fn camera_pass(
    svc: &mut Services<'_>,
    id: RasterId,
    camera: &Camera,
    target: TextureId,
    draws: &[CameraDraw],
) -> Result<RasterPass, OffscreenError> {
    let loads = Loads {
        color: camera.clear_color,
        depth: camera.clear_depth.then_some(DEPTH_CLEAR),
    };
    list_steps::attachments_pass(svc, id, &whole_target(target), &loads, draws)
}

/// The color and depth of a render texture at its base level: what a camera renders into.
fn whole_target(texture: TextureId) -> Attachments {
    let whole = Some(Attachment { texture, mip: 0 });
    Attachments::single(whole, whole)
}

pub(super) fn push_use(uses: &mut Vec<ResourceUse>, usage: ResourceUse) {
    let same = |other: &ResourceUse| {
        other.view.allocation() == usage.view.allocation()
            && other.view.range() == usage.view.range()
            && other.view.version() == usage.view.version()
    };
    if !uses.iter().any(same) {
        uses.push(usage);
    }
}

pub(super) fn push_buffer_use(uses: &mut Vec<BufferUse>, usage: BufferUse) {
    let same = |other: &BufferUse| {
        other.view.allocation() == usage.view.allocation()
            && other.view.range() == usage.view.range()
            && other.view.version() == usage.view.version()
            && other.kind == usage.kind
    };
    if !uses.iter().any(same) {
        uses.push(usage);
    }
}

/// The buffer reads of one operation, with the reads of one buffer at one version in one kind
/// merged into a single use over the span they cover.
///
/// The contract checks every use against every other and every binding against the uses, so an
/// operation of many draws that declared each draw's slice of a shared buffer would cost the
/// square of its draws. A read is a declaration that the pass may read the span, and reads do not
/// conflict, so the span stands for each of the reads in it.
#[derive(Default)]
pub(super) struct BufferReads {
    places: BTreeMap<(AllocationKey, u32, u8), usize>,
    spans: Vec<(BufferUse, Range<u64>)>,
}

impl BufferReads {
    pub(super) fn add(&mut self, usage: &BufferUse) {
        let view = &usage.view;
        let key = (view.allocation(), view.version(), kind_rank(usage.kind));
        let range = view.range();
        if let Some(&place) = self.places.get(&key) {
            let span = &mut self.spans[place].1;
            span.start = span.start.min(range.start);
            span.end = span.end.max(range.end);
        } else {
            self.places.insert(key, self.spans.len());
            self.spans.push((usage.clone(), range));
        }
    }

    /// The merged uses, in the order their buffers were first read.
    pub(super) fn finish(self, tracker: &Tracker) -> Result<Vec<BufferUse>, OffscreenError> {
        self.spans
            .into_iter()
            .map(|(usage, span)| {
                let view =
                    tracker.buffer_view(usage.view.allocation(), span, usage.view.version())?;
                Ok(BufferUse { view, ..usage })
            })
            .collect()
    }
}

fn kind_rank(kind: BufferUseKind) -> u8 {
    match kind {
        BufferUseKind::Uniform => 0,
        BufferUseKind::Storage => 1,
        BufferUseKind::Vertex => 2,
        BufferUseKind::Index => 3,
        BufferUseKind::Copy => 4,
    }
}
