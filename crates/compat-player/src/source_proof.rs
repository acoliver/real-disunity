//! Actual PE/PDB signature inspection. Evidence grants no activation authority.
#[path = "source_proof/json.rs"]
mod json;
#[path = "source_proof/paths.rs"]
mod paths;
#[path = "source_proof/process.rs"]
mod process;
#[path = "source_proof/report.rs"]
mod report;

use std::path::{Path, PathBuf};

pub use paths::validate_document_path;

/// Immutable game images selected by the host's fixed inspection runner.
#[derive(Clone, Copy)]
pub enum SelectedGame {
    Snake,
    Pong,
    Breakout,
    KeyboardControl,
}

impl SelectedGame {
    fn name(self) -> &'static str {
        match self {
            Self::Snake => "Snake",
            Self::Pong => "Pong",
            Self::Breakout => "Breakout",
            Self::KeyboardControl => "Keyboard",
        }
    }
}

/// Process-produced inspection evidence, with current binary and source bindings.
///
/// There is deliberately no Deserialize implementation, accepted-proof conversion,
/// activation method, or public constructor from a report.
pub struct Inspection {
    evidence: serde_json::Value,
    bindings: Vec<(PathBuf, String)>,
    output_sha256: String,
}

impl Inspection {
    /// Run the fixed metadata inspector under the task-owned outer namespace.
    pub fn run(workspace: &Path, game: SelectedGame) -> Result<Self, String> {
        let workspace = workspace.canonicalize().map_err(|e| e.to_string())?;
        let work = workspace.join(".work/resumed-source-signature-proof");
        let config = work.join(format!("{}.config.json", game.name()));
        let bindings = report::bind_inputs(&workspace, &config)?;
        let (bytes, output_sha256) = process::run(&work, game.name())?;
        let evidence = report::validate(&bytes, &config, &bindings)?;
        let inspection = Self {
            evidence,
            bindings,
            output_sha256,
        };
        inspection.verify_current()?;
        Ok(inspection)
    }

    /// Read-only diagnostic output. Metadata matches remain unqualified boundaries.
    pub fn report(&self) -> &serde_json::Value {
        &self.evidence
    }

    /// SHA-256 of the exact process-produced output bytes.
    pub fn output_sha256(&self) -> &str {
        &self.output_sha256
    }

    /// Serialize diagnostics without creating reconstructible activation authority.
    pub fn write_report(&self, output: impl std::io::Write) -> Result<(), String> {
        serde_json::to_writer(output, &self.evidence).map_err(|e| e.to_string())
    }

    /// Detect changed sources, images, inspector implementation, or runner inputs.
    pub fn verify_current(&self) -> Result<(), String> {
        for (path, expected) in &self.bindings {
            if compat_kit::files::hash(path)? != *expected {
                return Err(format!("inspection input changed: {}", path.display()));
            }
        }
        Ok(())
    }
}
