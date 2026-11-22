//! What planning a frame's draws left out, narrowed or substituted, for the host to report.

use crate::ids::TextureId;

/// What planning a frame's draws left out or narrowed, for the host to report.
#[derive(Clone, Default)]
pub(super) struct DrawCounts {
    /// Camera draws left out because the shader has no pass the camera selects.
    pub(super) unmatched: usize,
    /// Camera draws left out because no layer names a texture the pass samples, in all and by the
    /// texture property.
    pub(super) missing_texture: usize,
    pub(super) missing_textures: std::collections::BTreeMap<String, usize>,
    /// Draws left out because no layer names a structured buffer the pass reads, by the buffer
    /// property.
    pub(super) missing_buffers: std::collections::BTreeMap<String, usize>,
    /// The texture property names the slots of the planned draws looked up in the layers, found or
    /// not.
    pub(super) texture_lookups: std::collections::BTreeSet<String>,
    /// Draws that read a stand-in texture, in all and by (texture property, reason).
    pub(super) stand_in_draws: usize,
    pub(super) stand_ins: std::collections::BTreeMap<(String, String), usize>,
    /// Camera draws left out because the mesh lacks a vertex channel the pass reads, in all and
    /// by (source channel, shader input).
    pub(super) missing_channel: usize,
    pub(super) missing_channels: std::collections::BTreeMap<(i8, i8), usize>,
    /// Draws left out because the pass samples a texture it draws into, in all and by that
    /// texture.
    pub(super) feedback_loop: usize,
    pub(super) feedback_loops: std::collections::BTreeMap<TextureId, usize>,
    /// Draws that read a constant for a channel their mesh lacks, by channel, input and format.
    pub(super) defaulted: Vec<super::DefaultedChannelDraws>,
    /// Texture reads that bound fewer levels than the texture has.
    pub(super) partial_mips: usize,
    /// What each camera render recorded, in call order.
    pub(super) cameras: Vec<super::CameraDraws>,
    /// What each draw list recorded, in call order.
    pub(super) lists: Vec<super::ListDraws>,
    /// The draws left out, by what the shader's passes have against what the camera selected.
    pub(super) unmatched_passes: Vec<super::UnmatchedPasses>,
    /// The partial texture reads, by what read them.
    pub(super) partial_reads: Vec<super::PartialMipReads>,
    /// Why each camera render left draws out, in the order of `cameras`, and the camera now being
    /// planned.
    pub(super) camera_causes: Vec<super::CameraCauses>,
    pub(super) current: super::CameraCauses,
    /// How each camera render drew, in the order of `cameras`.
    pub(super) passes: Vec<super::CameraPassReport>,
    /// Programs built because the state had none, and the time draws spent choosing programs.
    pub(super) programs_built: usize,
    pub(super) program_time: std::time::Duration,
}

impl DrawCounts {
    /// Takes the counts back to `before`, as if the draws planned since had not been, except for
    /// the programs they built and the time they took, which the state keeps.
    pub(super) fn forget_since(&mut self, before: Self) {
        let (programs_built, program_time) = (self.programs_built, self.program_time);
        *self = Self {
            programs_built,
            program_time,
            ..before
        };
    }

    /// Notes that a draw's slot looked up the texture property `name`.
    pub(super) fn note_texture_lookup(&mut self, name: &str) {
        if !self.texture_lookups.contains(name) {
            self.texture_lookups.insert(name.to_owned());
        }
    }

    /// Counts one draw that read a stand-in texture for each of `stand_ins`, as one draw with
    /// stand-ins however many it read.
    pub(super) fn note_stand_ins(&mut self, stand_ins: &[(String, String)]) {
        if stand_ins.is_empty() {
            return;
        }
        self.stand_in_draws += 1;
        for entry in stand_ins {
            *self.stand_ins.entry(entry.clone()).or_default() += 1;
        }
    }

    /// Counts one draw that read a constant for each of `defaulted`.
    pub(super) fn note_defaulted(&mut self, defaulted: &[(i8, i8, wgpu::VertexFormat)]) {
        for &(channel, input, format) in defaulted {
            let found = self.defaulted.iter_mut().find(|entry| {
                (entry.channel, entry.input, entry.format) == (channel, input, format)
            });
            if let Some(entry) = found {
                entry.draws += 1;
            } else {
                self.defaulted.push(super::DefaultedChannelDraws {
                    channel,
                    input,
                    format,
                    draws: 1,
                });
            }
        }
    }
}
