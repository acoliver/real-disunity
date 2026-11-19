//! The census of draws that read a stand-in texture instead of a real one.
//!
//! A stand-in is a texture the host supplies where a draw's slot has none of the material's, the
//! block's or the globals' own: a shader's declared default (white, black, grey, bump) and the grey
//! that a slot nothing sets and no shader declares reads. Each draw that binds one is counted by
//! the property and the reason, so a run that draws with stand-ins does not look like a run that
//! drew with the game's textures.

use std::collections::BTreeMap;

use crate::counts_line::top_counts_line;

/// Why a draw reads a stand-in for a texture property.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StandInReason {
    /// The material lists no slot for a property its shader declares a default for, or the
    /// property block or globals do not either: nothing names a texture.
    Unset,
    /// The slot names no texture. The world cannot say whether the material names none, or named
    /// an asset the importer did not build or the managed engine could not carry as a texture.
    UnsetOrNotLoaded,
    /// The slot names a texture the engine knows of, and the host holds no pixels of it: the
    /// engine stored none, the upload was refused or over the storage budget, or the texture is a
    /// render texture that no longer exists.
    ExistsNotLoaded,
    /// The slot names a stored texture whose format the host does not decode.
    UnsupportedFormat,
    /// The shader reflects a slot it does not declare and no layer sets it.
    Undeclared,
}

impl StandInReason {
    /// The reason as the report prints it and the frame's layers carry it.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Unset => "unset",
            Self::UnsetOrNotLoaded => "unset-or-not-loaded",
            Self::ExistsNotLoaded => "exists-not-loaded",
            Self::UnsupportedFormat => "unsupported-format",
            Self::Undeclared => "undeclared",
        }
    }
}

/// The draws that read stand-ins: all of them, and by `<property> <reason>`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StandInCounts {
    /// The draws that read at least one stand-in.
    pub draws: usize,
    /// The draws that read a stand-in for each property and reason, as `<property> <reason>`. A
    /// draw that reads stand-ins for two properties is counted under both.
    pub by_property: BTreeMap<String, usize>,
}

impl StandInCounts {
    /// Adds `other`'s counts to these.
    pub fn add(&mut self, other: &Self) {
        self.draws += other.draws;
        for (name, count) in &other.by_property {
            *self.by_property.entry(name.clone()).or_default() += count;
        }
    }

    /// The lines of the report, without the `[gpu] ` prefix: the draws and, when any, the top
    /// `top` properties with the reason and the count.
    #[must_use]
    pub fn lines(&self, top: usize) -> Vec<String> {
        let mut lines = vec![format!("draws with stand-ins {}", self.draws)];
        lines.extend(top_counts_line(
            "stand-ins by property",
            &self.by_property,
            top,
        ));
        lines
    }
}
