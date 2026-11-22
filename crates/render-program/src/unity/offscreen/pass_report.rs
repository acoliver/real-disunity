//! The report of a camera pass: how its draws were set up and what the probed camera's draws did.

use crate::ids::{CameraId, TextureId};

use super::depth_range;
use super::frame::Camera;
use super::plan::Services;
use super::probe::{DrawTriangles, MeshPositions, ProgramProbes};
use super::steps::{CameraDraw, DEPTH_CLEAR};
use super::{CameraPassReport, CameraProbe, PassSetup, ProgramProbe, SampledTexture};

/// How the camera's draws were set up: the target's formats, the states of the draws and the names
/// the draws read with no value.
pub(super) fn pass_report(
    (camera_id, target): (CameraId, TextureId),
    (svc, camera): (&Services<'_>, &Camera),
    draws: &[CameraDraw],
    without_value: &std::collections::BTreeSet<String>,
) -> CameraPassReport {
    let info = svc.textures.get(target).ok();
    let mut states = std::collections::BTreeMap::<String, usize>::new();
    for draw in draws {
        let state = &draw.plan.state;
        let stencil = state.stencil.as_ref().map_or_else(String::new, |stencil| {
            format!(
                ", stencil reference {} read mask {} write mask {} front {:?} back {:?}",
                stencil.reference,
                stencil.ops.read_mask,
                stencil.ops.write_mask,
                stencil.ops.front,
                stencil.ops.back
            )
        });
        let words = format!(
            "cull {:?}, depth test {:?} write {}, color mask {}, blend {}{stencil}",
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
                (Some(_), true) => format!("clear {:?}", depth_range::depth_clear(DEPTH_CLEAR)),
                (Some(_), false) => "load".to_owned(),
            },
        },
        programs: Vec::new(),
    }
}

/// What the probed camera's draws did, by program.
pub(super) fn program_probes(
    svc: &Services<'_>,
    names: &CameraProbe,
    draws: &[CameraDraw],
) -> Vec<ProgramProbe> {
    let mut probes = ProgramProbes::default();
    for draw in draws {
        let Some(facts) = &draw.plan.facts else {
            continue;
        };
        // The probe reads a mesh's CPU positions and indices; a procedural draw has none.
        let Some(Ok(mesh)) = draw.mesh.map(|mesh| svc.meshes.get(mesh)) else {
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
