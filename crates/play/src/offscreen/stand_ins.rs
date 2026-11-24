//! The census of draws that read a stand-in texture instead of a real one.
//!
//! A stand-in is a texture the host supplies where a draw's slot has none of the material's, the
//! block's or the globals' own: a shader's declared default (white, black, grey, bump) and the grey
//! that a slot nothing sets and no shader declares reads. Each draw that binds one is counted by
//! the property and the reason, so a run that draws with stand-ins does not look like a run that
//! drew with the game's textures.

use std::collections::{BTreeMap, BTreeSet};

use crate::counts_line::{names_line, top_counts_line};

/// Why a draw reads a stand-in for a texture property.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StandInReason {
    /// The material lists no slot for a property its shader declares a default for, or the
    /// property block or globals do not either: nothing names a texture.
    Unset,
    /// The slot is null in the game's own data and the shader declares a default for it, which
    /// the shader reads, as the original does.
    DeclaredDefault,
    /// The slot names a texture the engine did not send: the managed engine could not carry it,
    /// the importer did not build it, or it is a render texture that was destroyed.
    NotLoaded,
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
            Self::DeclaredDefault => "declared-default",
            Self::NotLoaded => "not-loaded",
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

    /// The counts of the draws added since these counts were `before`.
    #[must_use]
    pub fn since(&self, before: &Self) -> Self {
        let by_property = self
            .by_property
            .iter()
            .filter_map(|(name, count)| {
                let earlier = before.by_property.get(name).copied().unwrap_or(0);
                let added = count.saturating_sub(earlier);
                (added > 0).then(|| (name.clone(), added))
            })
            .collect();
        Self {
            draws: self.draws.saturating_sub(before.draws),
            by_property,
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

/// The texture globals a run set and the texture names its draws looked up in a slot.
///
/// Names are exact: a global set as `Foo` is read by a slot that asks for `Foo`, and a slot that
/// asks for `_Foo` finds nothing. A global that is set and never looked up shows a game whose data
/// and shaders name a texture differently.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GlobalReads {
    /// The names of the texture globals the world held at a frame and the frame's commands set.
    pub set: BTreeSet<String>,
    /// The names the slots of the recorded draws looked up, whether or not a layer holds them.
    pub read: BTreeSet<String>,
}

impl GlobalReads {
    /// Adds `other`'s names to these.
    pub fn add(&mut self, other: &Self) {
        self.set.extend(other.set.iter().cloned());
        self.read.extend(other.read.iter().cloned());
    }

    /// The names set and never looked up, in name order.
    #[must_use]
    pub fn unread(&self) -> BTreeSet<String> {
        self.set.difference(&self.read).cloned().collect()
    }

    /// The line of the report, without the `[gpu] ` prefix: the first `top` unread names and the
    /// count of the rest. There is no line when every name set was looked up.
    #[must_use]
    pub fn line(&self, top: usize) -> Option<String> {
        names_line("globals set and never read by a draw", &self.unread(), top)
    }
}
