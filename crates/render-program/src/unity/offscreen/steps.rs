//! The steps of a frame, in call order, before the constants buffer exists: camera passes with
//! their draws, blits and copies.

use std::collections::BTreeMap;
use std::ops::Range;

use worldrunner_render_post::contracts::{
    AllocationKey, BufferUse, BufferUseKind, ColorAttachment, DepthAttachment, DrawGeometry,
    IndexBinding, RasterId, RasterPass, ResourceLifetime, ResourceUse, TextureCopy, TypedOperation,
    TypedWork,
};
use worldrunner_render_post::{AttachmentLoad, StageId};

use crate::ids::{CameraId, MaterialId, MeshId, ShaderId, TextureId};
use crate::layers::{Layers, PropertyLayer, TextureInput};
use crate::packet::TargetSignature;
use crate::properties::PropertyValue;
use crate::unity::shader::{PassSelector, ResolveError};

use super::error::OffscreenError;
use super::frame::{Camera, Material, OffscreenFrame, Record, SubMesh};
use super::plan::{DrawPlan, DrawSpec, Services, VertexPlan};
use super::probe::{DrawTriangles, MeshPositions, ProgramProbes};
use super::textures::TextureInfo;
use super::tracker::Tracker;
use super::{
    CameraCauses, CameraDraws, CameraPassReport, CameraProbe, PassSetup, ProgramProbe,
    SampledTexture, UnmatchedPasses,
};

/// One draw of a camera's pass.
pub(super) struct CameraDraw {
    pub(super) shader: ShaderId,
    pub(super) mesh: MeshId,
    pub(super) plan: DrawPlan,
    pub(super) vertices: VertexPlan,
    pub(super) geometry: DrawGeometry,
    pub(super) signature: TargetSignature,
    pub(super) front_face: wgpu::FrontFace,
    pub(super) index_use: Option<BufferUse>,
}

/// A blit with a material, whose bindings wait for the constants buffer.
pub(super) struct BlitStep {
    pub(super) id: RasterId,
    pub(super) colors: Vec<Option<ColorAttachment>>,
    pub(super) plan: DrawPlan,
    pub(super) vertices: VertexPlan,
    pub(super) count: u32,
}

pub(super) enum Step {
    Ready(TypedOperation),
    Camera {
        id: RasterId,
        pass: RasterPass,
        draws: Vec<CameraDraw>,
    },
    Blit(BlitStep),
}

fn raster_id(name: &'static str, index: usize) -> RasterId {
    RasterId::new(
        StageId::new(name, 0),
        u32::try_from(index).unwrap_or(u32::MAX),
    )
}

/// Plans each record of the frame into a step.
pub(super) fn plan_steps(
    svc: &mut Services<'_>,
    frame: &OffscreenFrame,
) -> Result<Vec<Step>, OffscreenError> {
    let mut steps = Vec::new();
    for (index, record) in frame.records.iter().enumerate() {
        steps.push(match record {
            Record::CameraRender { camera, target } => {
                camera_step(svc, frame, index, (*camera, *target))?
            }
            Record::Blit {
                source,
                target,
                material,
                pass,
            } => blit_step(svc, frame, index, (*source, *target), (*material, *pass))?,
            Record::CopyTexture { .. } => Step::Ready(copy_step(svc, index, record)?),
        });
    }
    Ok(steps)
}

fn camera_step(
    svc: &mut Services<'_>,
    frame: &OffscreenFrame,
    index: usize,
    (camera_id, target): (CameraId, TextureId),
) -> Result<Step, OffscreenError> {
    let camera = frame
        .cameras
        .get(&camera_id)
        .ok_or_else(|| OffscreenError::Unknown(format!("camera {camera_id:?}")))?;
    require_render_texture(svc, target)?;
    let signature = svc.signature(target)?;
    let mut draws = Vec::new();
    let counted_before = frame
        .probe
        .test_targets
        .contains(&target)
        .then(|| svc.counts.clone());
    let unmatched_before = svc.counts.unmatched;
    let missing_before = svc.counts.missing_texture;
    let channel_before = svc.counts.missing_channel;
    svc.counts.current = CameraCauses::default();
    let outer_missing = std::mem::take(svc.missing);
    svc.probing = frame.probe.targets.contains(&target);
    let variant = frame.probe.variants.get(&target).copied();
    svc.constant_fragment = variant.is_some_and(|variant| variant.constant_fragment);
    for visible in visible_draws(svc, frame, camera)? {
        if let Some(mut draw) = planned_draw(svc, frame, (camera, signature), visible)? {
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

/// How the camera's draws were set up: the target's formats, the states of the draws and the names
/// the draws read with no value.
fn pass_report(
    (camera_id, target): (CameraId, TextureId),
    (svc, camera): (&Services<'_>, &Camera),
    draws: &[CameraDraw],
    without_value: &std::collections::BTreeSet<String>,
) -> CameraPassReport {
    let info = svc.textures.get(target).ok();
    let mut states = std::collections::BTreeMap::<String, usize>::new();
    for draw in draws {
        let state = &draw.plan.state;
        let words = format!(
            "cull {:?}, depth test {:?} write {}, color mask {}, blend {}",
            state.cull,
            state.depth_compare,
            state.depth_write,
            write_mask(state.color_write),
            blend_words(state.blend)
        );
        *states.entry(words).or_default() += 1;
    }
    CameraPassReport {
        camera: camera_id,
        target,
        color: info.and_then(|info| info.color.map(|_| info.format)),
        depth: info.and_then(|info| info.depth_format),
        states: states.into_iter().collect(),
        without_value: without_value.iter().cloned().collect(),
        setup: PassSetup {
            viewport: info.map_or([0, 0], |info| info.size),
            color: match (
                info.is_some_and(|info| info.color.is_some()),
                camera.clear_color,
            ) {
                (false, _) => "no color".to_owned(),
                (true, Some(c)) => {
                    format!("clear [{:?}, {:?}, {:?}, {:?}]", c[0], c[1], c[2], c[3])
                }
                (true, None) => "load".to_owned(),
            },
            depth: match (info.and_then(|info| info.depth), camera.clear_depth) {
                (None, _) => "no depth".to_owned(),
                (Some(_), true) => format!("clear {DEPTH_CLEAR:?}"),
                (Some(_), false) => "load".to_owned(),
            },
        },
        programs: Vec::new(),
    }
}

/// The layers of a camera draw's values, highest first: the names of the places in an
/// [`Origin::Layer`](super::Origin).
pub const DRAW_LAYERS: [&str; 6] = [
    "per-draw",
    "camera",
    "property block",
    "material",
    "material fallback",
    "globals",
];

/// The depth a camera that clears depth clears to.
const DEPTH_CLEAR: f32 = 1.0;

/// What the probed camera's draws did, by program.
fn program_probes(
    svc: &Services<'_>,
    names: &CameraProbe,
    draws: &[CameraDraw],
) -> Vec<ProgramProbe> {
    let mut probes = ProgramProbes::default();
    for draw in draws {
        let Some(facts) = &draw.plan.facts else {
            continue;
        };
        let Ok(mesh) = svc.meshes.get(draw.mesh) else {
            continue;
        };
        let positions = MeshPositions {
            vertices: u64::from(mesh.vertex_count),
            positions: mesh.positions.as_deref(),
        };
        let textures = facts.textures.iter().map(|(name, texture)| {
            let info = svc.textures.get(*texture).ok();
            SampledTexture {
                name: name.clone(),
                texture: *texture,
                uploaded: info.is_some_and(|info| info.spec.is_none()),
                texels: info.and_then(|info| info.counts).map(|(texels, _)| texels),
                nonzero: info
                    .and_then(|info| info.counts)
                    .map(|(_, nonzero)| nonzero),
            }
        });
        let triangles = DrawTriangles {
            indices: mesh.cpu_indices.as_deref(),
            elements: draw.geometry.elements.clone(),
            base_vertex: draw.geometry.base_vertex,
            cull: draw.plan.state.cull,
            front_face: draw.front_face,
        };
        probes.add(
            (draw.shader, draw.plan.entry.pass),
            names,
            facts,
            (&positions, &triangles),
            textures,
        );
    }
    probes.finish()
}

/// A blend state in words: `off`, or its color and alpha equations as `operation source
/// destination`.
fn blend_words(blend: Option<wgpu::BlendState>) -> String {
    let equation = |part: wgpu::BlendComponent| {
        format!(
            "{:?} {:?} {:?}",
            part.operation, part.src_factor, part.dst_factor
        )
    };
    blend.map_or_else(
        || "off".to_owned(),
        |blend| {
            format!(
                "on (color {}, alpha {})",
                equation(blend.color),
                equation(blend.alpha)
            )
        },
    )
}

/// The channels a color write mask lets through, as `RGBA` with the ones it blocks left out, or
/// `none`.
fn write_mask(mask: wgpu::ColorWrites) -> String {
    let channels = [
        (wgpu::ColorWrites::RED, 'R'),
        (wgpu::ColorWrites::GREEN, 'G'),
        (wgpu::ColorWrites::BLUE, 'B'),
        (wgpu::ColorWrites::ALPHA, 'A'),
    ];
    let letters: String = channels
        .iter()
        .filter(|(channel, _)| mask.contains(*channel))
        .map(|(_, letter)| *letter)
        .collect();
    if letters.is_empty() {
        "none".to_owned()
    } else {
        letters
    }
}

/// Plans one sub-mesh the camera sees as (material, renderer, slot). A sub-mesh with no
/// indices, a draw whose shader has no pass the camera selects, a draw whose pass samples a
/// texture no layer names and a draw whose mesh lacks a channel the pass reads give none; the
/// last three are counted.
fn planned_draw(
    svc: &mut Services<'_>,
    frame: &OffscreenFrame,
    (camera, signature): (&Camera, TargetSignature),
    (material_id, renderer_id, slot): (MaterialId, crate::ids::RendererId, usize),
) -> Result<Option<CameraDraw>, OffscreenError> {
    let renderer = &frame.renderers[&renderer_id];
    let material = &frame.materials[&material_id];
    let shader = material
        .shader
        .ok_or(OffscreenError::NoShader(material_id))?;
    let layers = [
        &renderer.per_draw,
        &camera.values,
        &renderer.block,
        &material.properties,
        &material.fallback,
        &frame.globals,
    ];
    let spec = DrawSpec {
        shader,
        keywords: keywords(material, frame),
        selector: camera.pass.clone(),
        layers: Layers::new(&layers),
    };
    let sub = svc.meshes.get(renderer.mesh)?.sub_meshes[slot];
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
        (signature, camera.front_face),
    );
    if planned.is_err() {
        svc.constants.rollback(constants_mark);
    }
    match planned {
        Ok(draw) => {
            svc.counts.note_defaulted(&draw.vertices.defaulted);
            Ok(Some(draw))
        }
        Err(OffscreenError::Resolve(ResolveError::NoPass { .. })) => {
            svc.counts.unmatched += 1;
            left_out(svc, spec.shader, &camera.pass);
            Ok(None)
        }
        Err(OffscreenError::MissingTexture { name }) => {
            svc.counts.missing_texture += 1;
            *svc.counts.current.textures.entry(name.clone()).or_default() += 1;
            *svc.counts.missing_textures.entry(name).or_default() += 1;
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
        if renderer.layer >= 32 || camera.culling_mask & (1 << renderer.layer) == 0 {
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

fn keywords<'a>(material: &'a Material, frame: &'a OffscreenFrame) -> Vec<&'a str> {
    material
        .keywords
        .iter()
        .chain(&frame.global_keywords)
        .map(String::as_str)
        .collect()
}

fn camera_draw(
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
        mesh,
        plan,
        vertices,
        geometry,
        signature,
        front_face,
        index_use,
    })
}

fn require_render_texture(svc: &Services<'_>, id: TextureId) -> Result<(), OffscreenError> {
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
    let (imported, _) = svc.import(target)?;
    let mut uses = Vec::new();
    for draw in draws {
        for usage in &draw.plan.texture_uses {
            push_use(&mut uses, usage.clone());
        }
    }
    let color = imported
        .color
        .map(|key| {
            let (before, after) = svc.tracker.write(key, 0, 0);
            let view = svc
                .tracker
                .cell_view(key, (0, 0), wgpu::TextureAspect::All, after)?;
            Ok::<_, OffscreenError>(ColorAttachment {
                view,
                before: camera.clear_color.is_none().then_some(before),
                load: camera.clear_color.map_or(AttachmentLoad::Load, |c| {
                    AttachmentLoad::ClearColor(wgpu::Color {
                        r: c[0],
                        g: c[1],
                        b: c[2],
                        a: c[3],
                    })
                }),
                store: wgpu::StoreOp::Store,
                resolve: None,
            })
        })
        .transpose()?;
    let depth = imported
        .depth
        .map(|key| {
            let (before, after) = svc.tracker.write(key, 0, 0);
            let view = svc
                .tracker
                .cell_view(key, (0, 0), wgpu::TextureAspect::All, after)?;
            Ok::<_, OffscreenError>(DepthAttachment {
                view,
                before: (!camera.clear_depth).then_some(before),
                load: if camera.clear_depth {
                    AttachmentLoad::ClearDepth(1.0)
                } else {
                    AttachmentLoad::Load
                },
                store: wgpu::StoreOp::Store,
                read_only: false,
            })
        })
        .transpose()?;
    Ok(RasterPass {
        id,
        colors: color.into_iter().map(Some).collect(),
        depth,
        uses,
        bindings: Vec::new(),
    })
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

fn blit_step(
    svc: &mut Services<'_>,
    frame: &OffscreenFrame,
    index: usize,
    (source, target): (TextureId, TextureId),
    (material, pass): (Option<MaterialId>, usize),
) -> Result<Step, OffscreenError> {
    require_render_texture(svc, target)?;
    let Some(material_id) = material else {
        return plain_blit(svc, index, source, target).map(Step::Ready);
    };
    let material = frame
        .materials
        .get(&material_id)
        .ok_or(OffscreenError::UnknownMaterial(material_id))?;
    let shader = material
        .shader
        .ok_or(OffscreenError::NoShader(material_id))?;
    let mesh = frame.blit_mesh.ok_or(OffscreenError::NoBlitMesh)?;
    let overlay = source_layer(svc, frame, source)?;
    let layers = [
        &overlay,
        &material.properties,
        &material.fallback,
        &frame.globals,
    ];
    let spec = DrawSpec {
        shader,
        keywords: keywords(material, frame),
        selector: PassSelector::Index(pass),
        layers: Layers::new(&layers),
    };
    let plan = svc.plan_program(&spec)?;
    let vertices = svc.plan_vertices(mesh, &plan.entry)?;
    svc.counts.note_defaulted(&vertices.defaulted);
    let count = blit_vertex_count(svc, mesh)?;
    let (imported, _) = svc.import(target)?;
    let color = imported
        .color
        .ok_or(OffscreenError::NoColorAttachment(target))?;
    let (before, after) = svc.tracker.write(color, 0, 0);
    let view = svc
        .tracker
        .cell_view(color, (0, 0), wgpu::TextureAspect::All, after)?;
    let colors = vec![Some(ColorAttachment {
        view,
        before: Some(before),
        load: AttachmentLoad::Load,
        store: wgpu::StoreOp::Store,
        resolve: None,
    })];
    Ok(Step::Blit(BlitStep {
        id: raster_id("offscreen blit", index),
        colors,
        plan,
        vertices,
        count,
    }))
}

/// The layer a blit adds above the material: the source under the main texture name, and its
/// texel size under `<name>_TexelSize` for a shader that declares it.
fn source_layer(
    svc: &Services<'_>,
    frame: &OffscreenFrame,
    source: TextureId,
) -> Result<PropertyLayer, OffscreenError> {
    let info = svc.textures.get(source)?;
    let [width, height] = info.size.map(texels);
    let mut layer = PropertyLayer::default();
    layer.textures.insert(
        frame.main_texture.clone(),
        TextureInput {
            texture: source,
            sampler: None,
        },
    );
    layer.values.set(
        &format!("{}_TexelSize", frame.main_texture),
        PropertyValue::Vector([1.0 / width, 1.0 / height, width, height]),
    );
    Ok(layer)
}

/// A side in texels as a float. Sides above 65535 saturate, which no render texture reaches.
fn texels(side: u32) -> f32 {
    f32::from(u16::try_from(side).unwrap_or(u16::MAX))
}

fn blit_vertex_count(svc: &Services<'_>, mesh: MeshId) -> Result<u32, OffscreenError> {
    let info = svc.meshes.get(mesh)?;
    if info.index.is_some() || info.vertex_count % 3 != 0 {
        return Err(OffscreenError::BadMesh {
            mesh,
            reason: "a blit mesh is triangles with no index buffer".to_owned(),
        });
    }
    Ok(info.vertex_count)
}

/// A blit with no material copies the texture when the sizes and formats agree.
fn plain_blit(
    svc: &mut Services<'_>,
    index: usize,
    source: TextureId,
    target: TextureId,
) -> Result<TypedOperation, OffscreenError> {
    let (from, to) = (svc.textures.get(source)?, svc.textures.get(target)?);
    let reason = if from.size != to.size {
        Some(format!("the sizes are {:?} and {:?}", from.size, to.size))
    } else if from.format != to.format {
        Some(format!(
            "the formats are {:?} and {:?}",
            from.format, to.format
        ))
    } else {
        None
    };
    if let Some(reason) = reason {
        return Err(OffscreenError::BlitNeedsMaterial { reason });
    }
    copy_cells(
        svc,
        raster_id("offscreen blit", index),
        (source, 0, 0),
        (target, 0, 0),
    )
}

fn copy_step(
    svc: &mut Services<'_>,
    index: usize,
    record: &Record,
) -> Result<TypedOperation, OffscreenError> {
    let Record::CopyTexture {
        source,
        source_element,
        source_mip,
        target,
        target_element,
        target_mip,
    } = *record
    else {
        unreachable!("only copies are planned here");
    };
    require_render_texture(svc, target)?;
    copy_cells(
        svc,
        raster_id("offscreen copy", index),
        (source, source_mip, source_element),
        (target, target_mip, target_element),
    )
}

/// Copies one mip of one element to another, after checking that both exist and agree.
fn copy_cells(
    svc: &mut Services<'_>,
    id: RasterId,
    (source, source_mip, source_element): (TextureId, u32, u32),
    (target, target_mip, target_element): (TextureId, u32, u32),
) -> Result<TypedOperation, OffscreenError> {
    let (from, to) = (svc.textures.get(source)?, svc.textures.get(target)?);
    for (texture, info, mip, element) in [
        (source, from, source_mip, source_element),
        (target, to, target_mip, target_element),
    ] {
        if mip >= info.mips || element >= info.layers {
            return Err(OffscreenError::NoSuchSubresource {
                texture,
                mip,
                element,
            });
        }
    }
    check_copy((source, from, source_mip), (target, to, target_mip))?;
    let (source_imported, _) = svc.import(source)?;
    let (target_imported, _) = svc.import(target)?;
    let read = svc
        .tracker
        .cell(source_imported.primary, source_mip, source_element);
    let (_, written) = svc
        .tracker
        .write(target_imported.primary, target_mip, target_element);
    let aspect = wgpu::TextureAspect::All;
    let source_view = svc.tracker.cell_view(
        source_imported.primary,
        (source_mip, source_element),
        aspect,
        read,
    )?;
    let target_view = svc.tracker.cell_view(
        target_imported.primary,
        (target_mip, target_element),
        aspect,
        written,
    )?;
    Ok(TypedOperation::new(
        id,
        TypedWork::CopyTexture {
            source: TextureCopy::whole(source_view),
            destination: TextureCopy::whole(target_view),
        },
    ))
}

/// Checks that the two mips have the same size and the formats agree up to sRGB.
fn check_copy(
    (source, from, source_mip): (TextureId, &TextureInfo, u32),
    (target, to, target_mip): (TextureId, &TextureInfo, u32),
) -> Result<(), OffscreenError> {
    let mismatch = |reason: String| OffscreenError::CopyMismatch {
        from: source,
        to: target,
        reason,
    };
    let at = |size: [u32; 2], mip: u32| size.map(|side| (side >> mip).max(1));
    let sizes = (at(from.size, source_mip), at(to.size, target_mip));
    if sizes.0 != sizes.1 {
        return Err(mismatch(format!(
            "the sizes are {:?} and {:?}",
            sizes.0, sizes.1
        )));
    }
    if from.format.remove_srgb_suffix() != to.format.remove_srgb_suffix() {
        return Err(mismatch(format!(
            "the formats are {:?} and {:?}",
            from.format, to.format
        )));
    }
    Ok(())
}
