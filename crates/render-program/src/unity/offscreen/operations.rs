//! The typed operations a built frame hands to its consumer, and the counts and reports of what the
//! frame drew, left out and substituted.
use std::sync::Arc;

use worldrunner_render_gpu::submission::{FrameLease, SubmissionAuthority};
use worldrunner_render_post::contracts::{ResourceUse, TypedOperation};

use crate::budget::Reservation;
use crate::ids::{CameraId, TextureId};
use crate::offscreen::{AdmissionError, AdmittedOffscreen, Export, PendingOffscreen};

use super::counts::DrawCounts;
use super::error::OffscreenError;
use super::probe::{PassSetup, ProgramProbe};
use super::tracker::Tracker;
use super::views::ViewCounts;

/// The typed operations of a frame, ready to be checked.
pub struct OffscreenOperations {
    pub(super) device: wgpu::Device,
    pub(super) authority: SubmissionAuthority,
    pub(super) lease: FrameLease,
    pub(super) tracker: Tracker,
    pub(super) operations: Vec<TypedOperation>,
    pub(super) reads: Vec<ResourceUse>,
    pub(super) exports: Vec<Export>,
    pub(super) shares: Vec<Arc<Reservation>>,
    pub(super) missing: Vec<String>,
    pub(super) counts: DrawCounts,
    pub(super) constants_use: ConstantsUse,
    pub(super) planned_draws: usize,
    pub(super) taps: Vec<super::readback::TapRead>,
}

/// What a frame's constants took: the buffers they were uploaded into, their bytes, the largest
/// buffer's bytes, and the blocks packed against the blocks that took an earlier block's slice
/// because their bytes were equal.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ConstantsUse {
    pub buffers: usize,
    pub bytes: u64,
    pub largest: u64,
    pub blocks: usize,
    pub shared: usize,
}

/// Texture reads that bound only the levels written with the base level, counted for one
/// texture property, one set of levels and one pass.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PartialMipReads {
    /// The shader property the texture was bound under.
    pub texture: String,
    /// The levels bound and the levels the texture has.
    pub written: u32,
    pub total: u32,
    /// The tag key the drawing pass was selected by, or `index`.
    pub key: String,
    /// The value of the key on the pass drawn, or its index for `index`.
    pub tag: Option<String>,
    pub reads: usize,
}

/// What one camera render recorded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CameraDraws {
    pub camera: CameraId,
    pub target: TextureId,
    /// The draws recorded into the target.
    pub drawn: usize,
    /// The draws left out because the shader has no pass the camera selects.
    pub unmatched: usize,
    /// The draws left out because a texture the pass samples is named by no layer.
    pub missing_texture: usize,
    /// The draws left out because the pass samples a texture it draws into.
    pub feedback_loop: usize,
    /// The draws left out because the mesh lacks a vertex channel the pass reads.
    pub missing_channel: usize,
}

impl CameraDraws {
    /// The draws left out, for every reason.
    #[must_use]
    pub const fn left_out(&self) -> usize {
        self.unmatched + self.missing_texture + self.missing_channel + self.feedback_loop
    }
}

/// What one draw list recorded and left out. The draws left out are counted as a camera's are.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListDraws {
    /// The color texture the list drew into, or its depth texture when it has no color.
    pub target: TextureId,
    pub drawn: usize,
    pub unmatched: usize,
    pub missing_texture: usize,
    pub missing_channel: usize,
    pub feedback_loop: usize,
}

impl ListDraws {
    /// The draws left out, for every reason.
    #[must_use]
    pub const fn left_out(&self) -> usize {
        self.unmatched + self.missing_texture + self.missing_channel + self.feedback_loop
    }
}

/// Why one camera render left draws out: the texture properties no layer named and the
/// (source channel, shader input) pairs its meshes lacked, each with the draws it cost.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CameraCauses {
    pub textures: std::collections::BTreeMap<String, usize>,
    pub channels: std::collections::BTreeMap<(i8, i8), usize>,
}

/// Draws left out because their pass samples a texture it also draws into, counted for that
/// texture.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FeedbackLoopDraws {
    pub texture: TextureId,
    pub draws: usize,
}

/// Camera draws left out because the mesh lacks a vertex channel the pass reads, counted for one
/// source channel and the shader input it feeds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MissingChannelDraws {
    pub channel: i8,
    pub input: i8,
    pub draws: usize,
}

/// How one camera render drew: the formats of its target, the fixed-function state of the draws it
/// recorded and the constants and state properties its draws read that no layer gave a value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CameraPassReport {
    pub camera: CameraId,
    pub target: TextureId,
    pub color: Option<wgpu::TextureFormat>,
    pub depth: Option<wgpu::TextureFormat>,
    /// The state of the recorded draws in words, with the number of draws that had each.
    pub states: Vec<(String, usize)>,
    /// The names of the values no layer gave, which the draws read as zero or the pass's default.
    pub without_value: Vec<String>,
    /// How the target was set up: its viewport and what the pass clears.
    pub setup: PassSetup,
    /// What the draws did, by program, when the host asked about this render; else empty.
    pub programs: Vec<ProgramProbe>,
}

/// Draws that read a stand-in texture for one texture property, by the reason the host gave for
/// the stand-in. A draw that reads stand-ins for several properties is counted once for each.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StandInDraws {
    pub texture: String,
    pub reason: String,
    pub draws: usize,
}

/// Camera draws that read a shader input their mesh has no channel for and got a constant instead,
/// counted for one source channel, the shader input it feeds and the format the input was given.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DefaultedChannelDraws {
    pub channel: i8,
    pub input: i8,
    pub format: wgpu::VertexFormat,
    pub draws: usize,
}

/// Camera draws left out because no layer names a texture the pass samples, counted for one
/// texture property.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MissingTextureDraws {
    pub texture: String,
    pub draws: usize,
}

/// Draws left out because no layer names a structured buffer the pass reads, counted for one
/// buffer property.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MissingBufferDraws {
    pub buffer: String,
    pub draws: usize,
}

/// Camera draws left out for want of a pass, counted for the shaders whose passes carry the same
/// tag values.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnmatchedPasses {
    /// The tag key the camera selects by, or `index` when it selects a pass by position.
    pub key: String,
    /// The values the camera selects, in order of preference.
    pub selected: Vec<String>,
    /// The values of the key that the shader's passes have, in pass order.
    pub tags: Vec<String>,
    /// The passes of the shader's first sub-shader.
    pub passes: usize,
    pub draws: usize,
}

impl OffscreenOperations {
    /// The camera draws left out because the material's shader has no pass the camera selects.
    /// A renderer without a matching pass is not drawn by that camera; the count keeps a selector
    /// that matches nothing from passing as an empty image.
    #[must_use]
    pub fn unmatched_draws(&self) -> usize {
        self.counts.unmatched
    }

    /// The camera draws left out because no layer names a texture the pass samples, by the
    /// texture property, sorted by name. The rest of the camera's draws are recorded.
    #[must_use]
    pub fn missing_texture_draws(&self) -> Vec<MissingTextureDraws> {
        self.counts
            .missing_textures
            .iter()
            .map(|(texture, draws)| MissingTextureDraws {
                texture: texture.clone(),
                draws: *draws,
            })
            .collect()
    }

    /// The draws left out because no layer names a structured buffer the pass reads, by the
    /// buffer property, sorted by name. The rest of the pass's draws are recorded.
    #[must_use]
    pub fn missing_buffer_draws(&self) -> Vec<MissingBufferDraws> {
        self.counts
            .missing_buffers
            .iter()
            .map(|(buffer, draws)| MissingBufferDraws {
                buffer: buffer.clone(),
                draws: *draws,
            })
            .collect()
    }

    /// The draws that read a stand-in texture, by texture property and the reason the layer that
    /// supplied it gave, or `undeclared` for a slot nothing explains. Only recorded draws count.
    #[must_use]
    pub fn stand_in_draws(&self) -> Vec<StandInDraws> {
        self.counts
            .stand_ins
            .iter()
            .map(|((texture, reason), draws)| StandInDraws {
                texture: texture.clone(),
                reason: reason.clone(),
                draws: *draws,
            })
            .collect()
    }

    /// The texture property names the slots of the planned draws looked up in the layers, whether
    /// or not a layer held a texture under the name, sorted. A name that a global is set under and
    /// that is not here is a global no draw read.
    #[must_use]
    pub fn looked_up_textures(&self) -> &std::collections::BTreeSet<String> {
        &self.counts.texture_lookups
    }

    /// The recorded draws that read at least one stand-in texture.
    #[must_use]
    pub fn draws_with_stand_ins(&self) -> usize {
        self.counts.stand_in_draws
    }

    /// The camera draws left out because the mesh lacks a vertex channel the pass reads, by the
    /// channel and the input it feeds, sorted. The rest of the camera's draws are recorded.
    #[must_use]
    pub fn missing_channel_draws(&self) -> Vec<MissingChannelDraws> {
        self.counts
            .missing_channels
            .iter()
            .map(|(&(channel, input), &draws)| MissingChannelDraws {
                channel,
                input,
                draws,
            })
            .collect()
    }

    /// The draws left out because their pass samples a texture it draws into, at levels it
    /// writes, by that texture, sorted. A camera render, a list or a procedural draw keeps its
    /// other draws; the contract would refuse the whole frame for the one.
    #[must_use]
    pub fn feedback_loop_draws(&self) -> Vec<FeedbackLoopDraws> {
        self.counts
            .feedback_loops
            .iter()
            .map(|(&texture, &draws)| FeedbackLoopDraws { texture, draws })
            .collect()
    }

    /// The draws that read an input their mesh has no channel for, which they got a constant for.
    #[must_use]
    pub fn defaulted_channel_draws(&self) -> Vec<DefaultedChannelDraws> {
        self.counts.defaulted.clone()
    }

    /// How each camera render drew, in the order of [`camera_draws`](Self::camera_draws).
    #[must_use]
    pub fn camera_passes(&self) -> Vec<CameraPassReport> {
        self.counts.passes.clone()
    }

    /// What each camera render recorded and left out, in call order.
    #[must_use]
    pub fn camera_draws(&self) -> Vec<CameraDraws> {
        self.counts.cameras.clone()
    }

    /// What each draw list recorded and left out, in call order.
    #[must_use]
    pub fn list_draws(&self) -> Vec<ListDraws> {
        self.counts.lists.clone()
    }

    /// Why each camera render left draws out, in the order of [`camera_draws`](Self::camera_draws).
    #[must_use]
    pub fn camera_causes(&self) -> Vec<CameraCauses> {
        self.counts.camera_causes.clone()
    }

    /// The bytes of the frame's constants uploads: the constants of the draws that were kept, each
    /// block starting at the device's uniform offset alignment, and blocks of equal bytes shared.
    /// Zero when no draw has constants.
    #[must_use]
    pub fn constants_bytes(&self) -> u64 {
        self.constants_use.bytes
    }

    /// The buffers the constants are uploaded into: one unless the frame's constants exceed what a
    /// buffer may hold.
    #[must_use]
    pub fn constants_buffers(&self) -> usize {
        self.constants_use.buffers
    }

    /// The buffers, bytes and blocks of the frame's constants.
    #[must_use]
    pub fn constants_use(&self) -> ConstantsUse {
        self.constants_use
    }

    /// The draws the build planned and held at once before it made these operations.
    #[must_use]
    pub fn planned_draws(&self) -> usize {
        self.planned_draws
    }

    /// The texture views the build made and the ones its draws took from the views already made.
    #[must_use]
    pub fn view_counts(&self) -> ViewCounts {
        self.tracker.view_counts()
    }

    /// The programs this frame's draws built because the state had none for their shader, pass and
    /// variant yet. A frame whose draws all find theirs built none.
    #[must_use]
    pub fn programs_built(&self) -> usize {
        self.counts.programs_built
    }

    /// The time this frame's draws spent choosing and building their programs.
    #[must_use]
    pub fn program_time(&self) -> std::time::Duration {
        self.counts.program_time
    }

    /// The camera draws left out for want of a pass, by the tags the shader's passes have
    /// against the ones the camera selected.
    #[must_use]
    pub fn unmatched_passes(&self) -> Vec<UnmatchedPasses> {
        self.counts.unmatched_passes.clone()
    }

    /// The reads counted by `partial_mip_reads`, by texture property, levels and drawing pass.
    #[must_use]
    pub fn partial_mip_reads_by_pass(&self) -> Vec<PartialMipReads> {
        self.counts.partial_reads.clone()
    }

    /// The texture reads that bound only the levels written at the same point as the base level.
    #[must_use]
    pub fn partial_mip_reads(&self) -> usize {
        self.counts.partial_mips
    }

    /// The operations, in call order. The constants upload comes first when any draw has constants.
    #[must_use]
    pub fn operations(&self) -> &[TypedOperation] {
        &self.operations
    }

    /// The names of constant buffer members and state properties no layer supplied. They keep the
    /// packer's value, zero, or the pass's fixed state.
    #[must_use]
    pub fn missing(&self) -> &[String] {
        &self.missing
    }

    /// Hands the operations and their exports to a consumer that admits them on its own frame.
    /// Finish adapter-side reporting, readbacks and exports before handing off the packet.
    #[must_use]
    pub fn into_pending(self) -> PendingOffscreen {
        PendingOffscreen {
            device: self.device,
            authority: self.authority,
            lease: self.lease,
            registry: self.tracker.registry,
            operations: self.operations,
            reads: self.reads,
            exports: self.exports,
            shares: self.shares,
        }
    }

    /// Has the contract check the whole list and the reads the scene will make after it, and has
    /// `encoder`'s frame retain the arena and the storage the operations use until the GPU has
    /// completed the frame. Nothing is retained when the check fails.
    ///
    /// # Errors
    /// Fails with the contract's error for the operation at fault: a read without a producer, a
    /// hazard, a missing usage; and when `encoder` is not a frame of the runtime's authority or
    /// the arena has retired.
    pub fn admit(
        self,
        encoder: &wgpu::CommandEncoder,
    ) -> Result<AdmittedOffscreen, OffscreenError> {
        self.into_pending()
            .admit(encoder)
            .map_err(|error| match error {
                AdmissionError::Contract(error) => error.into(),
                AdmissionError::Program(error) => error.into(),
            })
    }
}
