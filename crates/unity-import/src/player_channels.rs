//! Counts which vertex source channels the variants of a player's shaders read. It reports counts
//! only, by the numeric code of the channel in the variant's vertex input table, so no name or
//! string of the player leaves the decoders.

use std::{collections::BTreeMap, fmt, path::PathBuf};

use worldrunner_unity_serialized::shader_program::BindChannels;

use crate::{
    PlayerError,
    player_variants::{Event, walk_variants},
};

/// How many variants read each source channel, and how the tables look.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ChannelReport {
    pub shaders: usize,
    pub variants: usize,
    /// Variants that did not decode, which have no table to count.
    pub unreadable: usize,
    /// Variants that read each source channel at least once, by the channel's code.
    pub readers: BTreeMap<i8, usize>,
    /// Variants that feed each shader input, by the input's code.
    pub inputs: BTreeMap<i8, usize>,
    /// Variants by how many (source, input) pairs their table holds.
    pub pair_counts: BTreeMap<usize, usize>,
    /// Variants that read a source channel more than once.
    pub repeated_sources: usize,
    /// Variants that feed a shader input more than once.
    pub repeated_inputs: usize,
    /// Variants whose source mask has exactly the bit of each pair's source channel set.
    pub mask_matches: usize,
}

impl ChannelReport {
    pub fn add_table(&mut self, table: &BindChannels) {
        self.variants += 1;
        *self.pair_counts.entry(table.channels.len()).or_default() += 1;
        let mut sources = Vec::new();
        let mut inputs = Vec::new();
        for &[source, input] in &table.channels {
            sources.push(source);
            inputs.push(input);
        }
        sources.sort_unstable();
        inputs.sort_unstable();
        self.repeated_sources += usize::from(sources.windows(2).any(|pair| pair[0] == pair[1]));
        self.repeated_inputs += usize::from(inputs.windows(2).any(|pair| pair[0] == pair[1]));
        sources.dedup();
        inputs.dedup();
        for source in &sources {
            *self.readers.entry(*source).or_default() += 1;
        }
        for input in &inputs {
            *self.inputs.entry(*input).or_default() += 1;
        }
        let mask = sources
            .iter()
            .try_fold(0u64, |mask, &source| {
                u32::try_from(source)
                    .ok()
                    .and_then(|bit| 1u64.checked_shl(bit))
                    .map(|bit| mask | bit)
            })
            .and_then(|mask| i32::try_from(mask).ok());
        self.mask_matches += usize::from(mask == Some(table.source_map));
    }
}

impl fmt::Display for ChannelReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "{} shaders with a Vulkan platform, {} variants with a table, {} unreadable",
            self.shaders, self.variants, self.unreadable
        )?;
        writeln!(f, "variants that read each source channel:")?;
        for (code, count) in &self.readers {
            writeln!(f, "  source {code}: {count} of {}", self.variants)?;
        }
        writeln!(f, "variants that feed each shader input:")?;
        for (code, count) in &self.inputs {
            writeln!(f, "  input {code}: {count} of {}", self.variants)?;
        }
        writeln!(f, "variants by number of pairs:")?;
        for (pairs, count) in &self.pair_counts {
            writeln!(f, "  {pairs} pairs: {count}")?;
        }
        writeln!(
            f,
            "{} variants read a source channel more than once, {} feed an input more than once, \
             {} have a source mask that is the bits of their sources",
            self.repeated_sources, self.repeated_inputs, self.mask_matches
        )
    }
}

/// Counts the vertex input tables of every Vulkan variant of the shaders in `paths`.
///
/// # Errors
/// Fails if a file cannot be read or a shader does not load; a variant that does not decode is
/// counted, not an error.
pub fn channel_report(paths: &[PathBuf]) -> Result<ChannelReport, PlayerError> {
    let mut report = ChannelReport::default();
    walk_variants(paths, &mut |event| match event {
        Event::Shader(_) => report.shaders += 1,
        Event::Unreadable(_) => report.unreadable += 1,
        Event::Variant { channels, .. } => report.add_table(channels),
    })?;
    Ok(report)
}
