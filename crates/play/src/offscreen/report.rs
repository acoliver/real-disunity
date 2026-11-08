//! What the mapping of a snapshot did, as counts: the records built and left out, what could not
//! be loaded or uploaded, how the cameras drew and what the storage held. Counts and names only.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{Debug, Display};

use worldrunner_render_program::unity::offscreen::{
    CameraDraws, CameraPassReport, OffscreenOperations, PartialMipReads, UnmatchedPasses,
};

use super::compute::DispatchTrace;
use super::frame::{RendererSkip, Skip};
use super::trace::Phases;

/// The uploaded textures and meshes given back to make room in the budget.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Reclaimed {
    pub textures: usize,
    pub meshes: usize,
    /// Their bytes, as the budget counted them.
    pub bytes: u64,
}

impl Reclaimed {
    /// Adds `other`'s counts to these.
    pub fn add(&mut self, other: Self) {
        self.textures += other.textures;
        self.meshes += other.meshes;
        self.bytes += other.bytes;
    }
}

/// What the mapping did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Report {
    /// The records the world listed.
    pub records: usize,
    /// The records of the frame, after a blit of every pass became one for each pass.
    pub built: usize,
    /// The dispatches among the records. They are not frame records; see [`Placed`].
    pub dispatches: usize,
    pub skipped: BTreeMap<Skip, usize>,
    pub renderers_skipped: BTreeMap<RendererSkip, usize>,
    /// The `GraphicsFormat` codes of the render textures the frame uses.
    pub formats: BTreeSet<u32>,
    /// What could not be loaded, uploaded or built, by the kind of thing and the name of the
    /// error, such as `shader:NoVulkanPlatform`.
    pub failures: BTreeMap<String, usize>,
    /// The same failures by the chain of variant names inside the error, such as
    /// `texture:Program/Layout/UnsupportedParameter`, and the first message of each chain.
    pub causes: BTreeMap<String, usize>,
    pub examples: BTreeMap<String, String>,
    /// How each dispatch was bound and what came of it, in call order.
    pub traces: Vec<DispatchTrace>,
    /// What each camera render recorded and left out.
    pub camera_draws: Vec<CameraDraws>,
    /// How each of those camera renders drew, in the same order.
    pub camera_passes: Vec<CameraPassReport>,
    /// The camera draws left out for want of a pass, and the texture reads that bound only the
    /// levels written with the base level, each counted by the tags of the pass concerned.
    pub unmatched_passes: Vec<UnmatchedPasses>,
    pub partial_reads: Vec<PartialMipReads>,
    /// The camera draws left out because no layer names a texture the pass samples, by the
    /// texture property.
    pub missing_textures: BTreeMap<String, usize>,
    /// The camera draws left out because the mesh lacks a vertex channel the pass reads, by
    /// `channel C for input I`.
    pub missing_channels: BTreeMap<String, usize>,
    /// The camera draws that read a constant for a vertex channel their mesh lacks, by
    /// `channel C for input I as FORMAT`. These draws are recorded; they are not failures.
    pub defaulted_channels: BTreeMap<String, usize>,
    /// The texture properties that named no texture the frame can read, by `name: reason`.
    pub unresolved_textures: BTreeMap<String, usize>,
    /// Where building the frame spent its time.
    pub phases: Phases,
    /// The script commands the adapter does not run, by what they are: `SetGlobal texture _Name
    /// (render)`, `SetRenderTarget`, and so on.
    pub unsupported_work: BTreeMap<String, usize>,
    /// The textures the world's globals hold when the frame is mapped, by `name: kind`.
    pub global_textures: BTreeMap<String, usize>,
    /// What the runtime's storage held when the budget first refused an allocation of this work,
    /// as lines; empty when it refused none.
    pub holdings: Vec<String>,
    /// What was given back to make room for this work.
    pub reclaimed: Reclaimed,
    /// Counts that describe what was mapped and are not failures, by description: the formats of
    /// the textures stored, the constants the adapter filled, and so on.
    pub census: BTreeMap<String, usize>,
    /// The pixel reads answered.
    pub pixel_reads: usize,
    /// The pixel reads left unanswered, by the reason.
    pub pixel_reads_refused: BTreeMap<String, usize>,
}

impl Report {
    /// Takes what `operations` counted of the draws it holds.
    pub fn absorb_draws(&mut self, operations: &OffscreenOperations) {
        self.camera_draws.extend(operations.camera_draws());
        self.camera_passes.extend(operations.camera_passes());
        for entry in operations.unmatched_passes() {
            self.add_unmatched(entry);
        }
        for entry in operations.partial_mip_reads_by_pass() {
            self.add_partial(entry);
        }
        for entry in operations.missing_texture_draws() {
            *self.missing_textures.entry(entry.texture).or_default() += entry.draws;
        }
        for entry in operations.missing_channel_draws() {
            let name = format!("channel {} for input {}", entry.channel, entry.input);
            *self.missing_channels.entry(name).or_default() += entry.draws;
        }
        for entry in operations.defaulted_channel_draws() {
            let name = format!(
                "channel {} for input {} as {:?}",
                entry.channel, entry.input, entry.format
            );
            *self.defaulted_channels.entry(name).or_default() += entry.draws;
        }
    }

    fn add_unmatched(&mut self, entry: UnmatchedPasses) {
        let same = |held: &&mut UnmatchedPasses| {
            (&held.key, &held.selected, &held.tags, held.passes)
                == (&entry.key, &entry.selected, &entry.tags, entry.passes)
        };
        if let Some(held) = self.unmatched_passes.iter_mut().find(|held| same(held)) {
            held.draws += entry.draws;
        } else {
            self.unmatched_passes.push(entry);
        }
    }

    fn add_partial(&mut self, entry: PartialMipReads) {
        let same = |held: &&mut PartialMipReads| {
            (
                &held.texture,
                held.written,
                held.total,
                &held.key,
                &held.tag,
            ) == (
                &entry.texture,
                entry.written,
                entry.total,
                &entry.key,
                &entry.tag,
            )
        };
        if let Some(held) = self.partial_reads.iter_mut().find(|held| same(held)) {
            held.reads += entry.reads;
        } else {
            self.partial_reads.push(entry);
        }
    }

    /// Counts a failure of `kind` by the name of `error` and by the chain of names inside it.
    pub fn fail(&mut self, kind: &str, error: &(impl Debug + Display)) {
        let name = format!("{kind}:{}", variant_name(error));
        *self.failures.entry(name).or_default() += 1;
        let cause = format!("{kind}:{}", variant_chain(error));
        *self.causes.entry(cause.clone()).or_default() += 1;
        self.examples
            .entry(cause)
            .or_insert_with(|| error.to_string());
    }

    /// Counts a failure that has no error behind it, by the whole of its name.
    pub fn count(&mut self, name: &str) {
        self.count_many(name, 1);
    }

    /// Counts `times` failures of the same name. Nothing is counted for zero.
    pub fn count_many(&mut self, name: &str, times: usize) {
        if times > 0 {
            *self.failures.entry(name.to_owned()).or_default() += times;
            *self.causes.entry(name.to_owned()).or_default() += times;
        }
    }

    /// Adds the failures, causes and examples of `other` to this report's, keeping the first
    /// example of each cause.
    pub fn absorb_failures(&mut self, other: Self) {
        for (name, count) in other.failures {
            *self.failures.entry(name).or_default() += count;
        }
        for (cause, count) in other.causes {
            *self.causes.entry(cause).or_default() += count;
        }
        for (cause, example) in other.examples {
            self.examples.entry(cause).or_insert(example);
        }
        for (name, count) in other.unresolved_textures {
            *self.unresolved_textures.entry(name).or_default() += count;
        }
        for (name, count) in other.census {
            *self.census.entry(name).or_default() += count;
        }
        if self.holdings.is_empty() {
            self.holdings = other.holdings;
        }
        self.reclaimed.add(other.reclaimed);
        self.camera_draws.extend(other.camera_draws);
        self.camera_passes.extend(other.camera_passes);
        self.phases.add(&other.phases);
        for (name, count) in other.unsupported_work {
            *self.unsupported_work.entry(name).or_default() += count;
        }
        for (name, count) in other.global_textures {
            *self.global_textures.entry(name).or_default() += count;
        }
        for entry in other.unmatched_passes {
            self.add_unmatched(entry);
        }
        for entry in other.partial_reads {
            self.add_partial(entry);
        }
        for (name, count) in other.missing_textures {
            *self.missing_textures.entry(name).or_default() += count;
        }
        for (name, count) in other.missing_channels {
            *self.missing_channels.entry(name).or_default() += count;
        }
        for (name, count) in other.defaulted_channels {
            *self.defaulted_channels.entry(name).or_default() += count;
        }
    }
}

/// The leading identifier of an error's `Debug` form: the name of its variant.
fn variant_name(error: &dyn Debug) -> String {
    variant_chain(error)
        .split('/')
        .next()
        .unwrap_or_default()
        .to_owned()
}

/// The names of the variants nested in an error's `Debug` form, outermost first and joined by
/// `/`: `Program(Layout(UnsupportedParameter { kind: ".." }))` is `Program/Layout/UnsupportedParameter`.
/// The chain ends at a variant whose payload is not another variant.
fn variant_chain(error: &dyn Debug) -> String {
    let text = format!("{error:?}");
    let mut names = Vec::new();
    let mut rest = text.as_str();
    loop {
        let end = rest
            .find(|c: char| !(c.is_alphanumeric() || c == '_'))
            .unwrap_or(rest.len());
        if end == 0 {
            break;
        }
        names.push(&rest[..end]);
        match rest[end..].strip_prefix('(') {
            Some(inner) => rest = inner,
            None => break,
        }
    }
    names.join("/")
}
