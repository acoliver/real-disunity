//! The mip levels of render textures, which a draw writes at level 0 only.
//!
//! A script fills them in one of two ways, and both become render-program's
//! [`Record::GenerateMips`]. The script's own `GenerateMips` command is mapped where it stands. A
//! render texture whose flag asks for automatic generation and that has more than one level gets
//! the record after the records that write its level 0 (a camera render, a blit or a copy into
//! it, a clear or a draw while it is the target), once for each run of consecutive writes: before
//! the next record that reads it, and at the end of the frame when nothing read it first.
//!
//! What a record reads decides where the generation goes. A copy and a blit with no material read
//! their source alone. A clear and a generation read nothing. Every other record (a camera render,
//! a blit with a material, a draw) reads whatever its materials and globals name, so it follows the
//! generation of every texture written before it, except those it writes itself: the texture stays
//! in its run of writes. A dispatch is placed after the generation of the textures it binds.
//!
//! A texture render-program refuses to generate (3D, depth-only, a format the device does not
//! filter) is left alone and counted once for the frame, so the refusal does not fail the build.

use std::collections::BTreeSet;

use worldrunner_render_program::ids::TextureId;
use worldrunner_render_program::unity::offscreen::RENDER_FORMATS;
use worldrunner_render_program::unity::offscreen::frame::{Attachments, Record, RenderDimension};
use worldrunner_world_simulation::graphics::{RenderTextureId, WorldRenderTexture};

use super::frame::{Mapper, Skip, render_dimension};
use super::ids;

/// The name a texture the device cannot generate mips for is counted under.
pub(super) const UNSUPPORTED: &str = "rendertexture:MipsUnsupported";

/// The `GraphicsFormat` codes of the 32-bit float formats, which a device filters only with the
/// feature that says so.
const FLOAT32_FORMATS: [u32; 2] = [49, 52];

/// The flagged textures a frame has written and not yet generated, in the order they were first
/// written, and those it has counted as refused.
#[derive(Default)]
pub(super) struct AutoMips {
    pending: Vec<TextureId>,
    refused: BTreeSet<TextureId>,
}

/// What the mips of a flagged texture come to.
enum Verdict {
    Generate,
    /// Render-program refuses them, for the reason given.
    Unsupported(String),
    /// The frame does not make the texture at all (its format is not one it draws into), which
    /// has its own count.
    Unmade,
}

/// What a record reads of the textures that wait for their mips.
enum Reads {
    Nothing,
    Only(TextureId),
    Anything,
}

fn reads_of(record: &Record) -> Reads {
    match record {
        Record::Clear { .. } | Record::GenerateMips { .. } | Record::Globals(_) => Reads::Nothing,
        Record::CopyTexture { source, .. }
        | Record::Blit {
            source,
            material: None,
            ..
        } => Reads::Only(*source),
        Record::Blit { .. }
        | Record::CameraRender { .. }
        | Record::Draws(_)
        | Record::Procedural(_) => Reads::Anything,
    }
}

/// The textures whose level 0 `record` writes.
fn writes_of(record: &Record) -> Vec<TextureId> {
    let base = |attachments: &Attachments| {
        attachments
            .colors
            .iter()
            .chain(&attachments.depth)
            .filter(|attachment| attachment.mip == 0)
            .map(|attachment| attachment.texture)
            .collect()
    };
    match record {
        Record::CameraRender { target, .. } | Record::Blit { target, .. } => vec![*target],
        Record::CopyTexture {
            target, target_mip, ..
        } => (*target_mip == 0).then_some(*target).into_iter().collect(),
        Record::Clear { target, .. } => base(target),
        Record::Draws(list) => base(&list.target),
        Record::Procedural(draw) => base(&draw.target),
        Record::GenerateMips { .. } | Record::Globals(_) => Vec::new(),
    }
}

impl Mapper<'_> {
    /// The record of the script's own `GenerateMips` command.
    pub(super) fn generate_mips(&self, texture: RenderTextureId) -> Result<Vec<Record>, Skip> {
        if !self.snapshot.render_textures.contains_key(&texture) {
            return Err(Skip::StaleTarget);
        }
        Ok(vec![Record::GenerateMips {
            texture: ids::render_texture(texture),
        }])
    }

    /// `records` with, in front of each, the generation of the flagged textures it must find
    /// whole, and the writes of each noted as waiting for theirs.
    pub(super) fn with_auto_mips(&mut self, records: Vec<Record>) -> Vec<Record> {
        let mut mapped = Vec::with_capacity(records.len());
        for record in records {
            let writes = writes_of(&record);
            match reads_of(&record) {
                Reads::Nothing => {}
                Reads::Only(texture) => mapped.extend(self.take_pending(|held| held == texture)),
                Reads::Anything => mapped.extend(self.take_pending(|held| !writes.contains(&held))),
            }
            if let Record::GenerateMips { texture } = record {
                self.auto_mips.pending.retain(|held| *held != texture);
            }
            for texture in writes {
                self.note_written(texture);
            }
            mapped.push(record);
        }
        mapped
    }

    /// The generation of every flagged texture a dispatch binds that is waiting for it.
    pub(super) fn mips_for_dispatch(&mut self, bound: &BTreeSet<TextureId>) -> Vec<Record> {
        self.take_pending(|held| bound.contains(&held))
    }

    /// The generation of every flagged texture still waiting at the end of the frame.
    pub(super) fn mips_at_end(&mut self) -> Vec<Record> {
        self.take_pending(|_| true)
    }

    /// The generation records of the waiting textures that `due` names, which no longer wait.
    fn take_pending(&mut self, due: impl Fn(TextureId) -> bool) -> Vec<Record> {
        let (now, later): (Vec<_>, Vec<_>) = std::mem::take(&mut self.auto_mips.pending)
            .into_iter()
            .partition(|texture| due(*texture));
        self.auto_mips.pending = later;
        if !now.is_empty() {
            *self
                .report
                .census
                .entry("generated mips of flagged render textures".to_owned())
                .or_default() += now.len();
        }
        now.into_iter()
            .map(|texture| Record::GenerateMips { texture })
            .collect()
    }

    /// Notes that the level 0 of `texture` was written, if its flag asks for its mips.
    fn note_written(&mut self, texture: TextureId) {
        if self.auto_mips.pending.contains(&texture) {
            return;
        }
        let Some(desc) = self.described(texture) else {
            return;
        };
        if !desc.auto_generate_mips || desc.mip_count < 2 {
            return;
        }
        match self.verdict(desc) {
            Verdict::Generate => self.auto_mips.pending.push(texture),
            Verdict::Unsupported(reason) => self.refuse(texture, &reason),
            Verdict::Unmade => {}
        }
    }

    /// Whether render-program can generate the mips of `desc`.
    fn verdict(&self, desc: &WorldRenderTexture) -> Verdict {
        if matches!(render_dimension(desc), RenderDimension::D3(_)) {
            return Verdict::Unsupported("3D texture".to_owned());
        }
        if desc.format == 0 {
            return Verdict::Unsupported("depth-only texture".to_owned());
        }
        if !RENDER_FORMATS.contains(&desc.format) {
            return Verdict::Unmade;
        }
        if FLOAT32_FORMATS.contains(&desc.format) && !self.available.float32_filterable {
            return Verdict::Unsupported(format!("format {} is not filtered", desc.format));
        }
        Verdict::Generate
    }

    /// Counts a texture whose mips are left ungenerated, once for the frame.
    fn refuse(&mut self, texture: TextureId, reason: &str) {
        if self.auto_mips.refused.insert(texture) {
            self.report.count(UNSUPPORTED);
            *self
                .report
                .census
                .entry(format!("auto mips left out: {reason}"))
                .or_default() += 1;
        }
    }
}
