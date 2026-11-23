//! Procedural draws: a material's passes over vertices the vertex stage makes from the vertex and
//! instance index, with no mesh.
//!
//! The draw plans its program, constants, textures and render state as a list item does, with
//! the draw's own values in place of a renderer's and an empty property block. It binds no
//! vertex buffer and no index: the geometry is the vertex range `0..vertices` over the instance
//! range `0..instances`, in the draw's topology. A pass whose vertex stage declares vertex inputs
//! is refused by name, since nothing feeds them; there is no constant stand-in.

use worldrunner_render_post::contracts::DrawGeometry;

use crate::layers::{Layers, PropertyLayer};
use crate::packet::TargetSignature;

use super::error::OffscreenError;
use super::frame::{OffscreenFrame, ProceduralDraw};
use super::list_steps::{self, Loads};
use super::plan::{DrawSpec, Services, VertexPlan};
use super::raster::{self, PassRaster};
use super::steps::{CameraDraw, Globals, Step, counted, keywords, raster_id};

/// The raster step of a procedural draw: one draw for each pass that plans, into the draw's
/// attachments, loading what they hold.
pub(super) fn procedural_step(
    svc: &mut Services<'_>,
    frame: &OffscreenFrame,
    index: usize,
    globals: Globals<'_>,
    draw: &ProceduralDraw,
) -> Result<Step, OffscreenError> {
    let target = list_steps::usable(svc, draw.target.clone())?;
    let signature = list_steps::signature(svc, &target)?;
    let size = list_steps::target_size(svc, &target)?;
    let raster = raster::resolve(&draw.raster, size);
    let material = frame
        .materials
        .get(&draw.material)
        .ok_or(OffscreenError::UnknownMaterial(draw.material))?;
    let shader = material
        .shader
        .ok_or(OffscreenError::NoShader(draw.material))?;
    // The same seven places a list item's values come from; a procedural draw has no property block.
    let block = PropertyLayer::default();
    let layers = [
        &draw.per_draw,
        &draw.values,
        &block,
        &material.properties,
        &material.fallback,
        globals.layer,
        &material.below_globals,
    ];
    let cells = list_steps::written(svc, &target)?;
    let mut draws = Vec::new();
    // A scissor with no texel in it leaves nothing to draw.
    let passes = raster.map_or(&[][..], |_| draw.passes.as_slice());
    for selector in passes {
        let spec = DrawSpec {
            shader,
            keywords: keywords(material, globals),
            selector: selector.clone(),
            layers: Layers::new(&layers),
            dots: None,
        };
        let mark = svc.constants.mark();
        let planned = generated(svc, &spec, draw, signature)
            .and_then(|planned| list_steps::refuse_feedback(planned, &cells));
        if planned.is_err() {
            svc.constants.rollback(mark);
        }
        if let Some(mut planned) = counted(svc, &spec, planned)? {
            planned.raster = raster.unwrap_or_default();
            draws.push(planned);
        }
    }
    let loads = Loads {
        color: None,
        depth: None,
    };
    let pass = list_steps::attachments_pass(
        svc,
        raster_id("offscreen procedural", index),
        &target,
        &loads,
        &draws,
    )?;
    Ok(Step::Camera {
        id: pass.id,
        pass,
        draws,
    })
}

/// The draw of one pass over generated vertices.
fn generated(
    svc: &mut Services<'_>,
    spec: &DrawSpec<'_>,
    draw: &ProceduralDraw,
    signature: TargetSignature,
) -> Result<CameraDraw, OffscreenError> {
    // The vertex stage is checked before any constant or texture is planned, so a pass that reads
    // vertex inputs is refused by that cause even when it also lacks a texture.
    let chosen = svc.choose(spec)?;
    let inputs = chosen.entry.vertex_input_count();
    if inputs > 0 {
        return Err(OffscreenError::ProceduralVertexInputs {
            shader: spec.shader,
            pass: chosen.entry.pass,
            inputs,
        });
    }
    let plan = svc.plan_chosen(spec, chosen)?;
    Ok(CameraDraw {
        shader: spec.shader,
        mesh: None,
        topology: draw.topology,
        plan,
        vertices: VertexPlan {
            inputs: Vec::new(),
            views: Vec::new(),
            uses: Vec::new(),
            defaulted: Vec::new(),
        },
        geometry: DrawGeometry {
            vertex_buffers: Vec::new(),
            index: None,
            elements: 0..draw.vertices,
            base_vertex: 0,
            instances: 0..draw.instances,
        },
        signature,
        front_face: draw.front_face,
        index_use: None,
        raster: PassRaster::default(),
    })
}
