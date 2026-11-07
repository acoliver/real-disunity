//! A probed camera render in words: how its target was set up and what each program's draws did.
//! Counts and names of constants only.

use std::collections::BTreeSet;

use worldrunner_render_program::unity::offscreen::{CameraPassReport, ClipCounts, ProgramProbe};

use super::compute::SampledSource;

/// The most uploaded textures a dispatch's trace reads back from the GPU.
const MOST_SOURCES: usize = 16;

fn clip(counts: ClipCounts) -> String {
    format!(
        "inside {}, outside {}, behind {}",
        counts.inside, counts.outside, counts.behind
    )
}

fn program(probe: &ProgramProbe) -> String {
    let textures: Vec<String> = probe
        .textures
        .iter()
        .map(
            |texture| match (texture.uploaded, texture.texels, texture.nonzero) {
                (true, Some(texels), Some(nonzero)) => {
                    format!("{}: uploaded, {nonzero} of {texels} non-zero", texture.name)
                }
                (true, _, _) => format!("{}: uploaded", texture.name),
                (false, _, _) => format!("{}: render texture", texture.name),
            },
        )
        .collect();
    format!(
        "program shader {} pass {}: {} draws, {} vertices, clip positions {}, as transposes {}, \
         without positions {}, without matrices {}; vertex constants without a value [{}], zero \
         [{}]; fragment constants without a value [{}], zero [{}]; textures [{}]",
        probe.shader.0,
        probe.pass,
        probe.draws,
        probe.vertices,
        clip(probe.clip),
        clip(probe.clip_transposed),
        probe.without_positions,
        probe.without_matrices,
        probe.vertex_without_value.join(", "),
        probe.vertex_zero.join(", "),
        probe.fragment_without_value.join(", "),
        probe.fragment_zero.join(", "),
        textures.join(", ")
    )
}

/// The lines for one camera render: its setup, then a line for each program the host asked about.
pub(super) fn notes(pass: &CameraPassReport) -> Vec<String> {
    let mut lines = vec![format!(
        "setup: viewport {:?}, color {} ({:?}), depth {} ({:?})",
        pass.setup.viewport, pass.setup.color, pass.color, pass.setup.depth, pass.depth
    )];
    lines.extend(pass.programs.iter().map(program));
    lines
}

/// The uploaded textures the programs of the render sampled, once each, up to the most a trace
/// reads back.
pub(super) fn sampled(pass: &CameraPassReport) -> Vec<SampledSource> {
    let mut seen = BTreeSet::new();
    let mut sources = Vec::new();
    for texture in pass.programs.iter().flat_map(|probe| &probe.textures) {
        if !texture.uploaded || !seen.insert(texture.texture) || sources.len() >= MOST_SOURCES {
            continue;
        }
        sources.push(SampledSource {
            name: texture.name.clone(),
            texture: texture.texture,
            uploaded: format!(
                "{} of {} non-zero",
                texture.nonzero.unwrap_or(0),
                texture.texels.unwrap_or(0)
            ),
            on_the_gpu: None,
        });
    }
    sources
}
