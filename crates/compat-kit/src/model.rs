use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Status {
    Pass,
    Fail,
    Blocked,
    Unsupported,
    Error,
    Skipped,
    Inconclusive,
    NotImplemented,
    Flaky,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CommandSpec {
    pub program: String,
    #[serde(default)]
    pub args: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Limits {
    pub deadline_ms: u64,
    pub log_bytes: u64,
    pub artifact_bytes: u64,
    pub capture_count: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            deadline_ms: 60_000,
            log_bytes: 1_048_576,
            artifact_bytes: 268_435_456,
            capture_count: 32,
        }
    }
}
impl Limits {
    pub fn validate(&self) -> Result<(), String> {
        if self.deadline_ms == 0
            || self.deadline_ms > 3_600_000
            || self.log_bytes > 16_777_216
            || self.artifact_bytes > 1_073_741_824
            || self.capture_count > 128
        {
            return Err("limits exceed harness safety bounds".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Fixture {
    pub schema_version: u32,
    pub name: String,
    pub game: String,
    pub profiles: Vec<String>,
    pub default_profile: String,
    pub evidence_label: String,
    #[serde(default)]
    pub worker: Option<CommandSpec>,
    #[serde(default)]
    pub limits: Limits,
    #[serde(default)]
    pub seed: u64,
    #[serde(default)]
    pub timeline: Vec<Value>,
    #[serde(default)]
    pub assertions: Vec<Assertion>,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Assertion {
    Equal {
        selector: String,
        #[serde(default)]
        pointer: String,
        value: Value,
        frame: Option<u64>,
        phase: Option<String>,
    },
    Tolerance {
        selector: String,
        #[serde(default)]
        pointer: String,
        value: f64,
        tolerance: f64,
        frame: Option<u64>,
        phase: Option<String>,
    },
    Changed {
        selector: String,
        #[serde(default)]
        pointer: String,
        before: u64,
        after: u64,
        phase: Option<String>,
    },
    EventOrder {
        selector: String,
        #[serde(default)]
        pointer: String,
        events: Vec<Value>,
    },
    Capture {
        frame: u64,
        phase: String,
        origin: String,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunRequest {
    pub schema_version: u32,
    pub run_id: String,
    pub fixture: String,
    pub profile: String,
    pub desktop: bool,
    pub seed: u64,
    pub timeline: Vec<Value>,
    pub assertions: Vec<Assertion>,
    pub source_root: PathBuf,
    pub artifact_root: PathBuf,
    pub metadata: Value,
}

impl RunRequest {
    /// The request a worker receives for one run of `fixture`.
    pub fn for_fixture(fixture: &Fixture, run: RunTarget) -> Self {
        Self {
            schema_version: 1,
            run_id: run.run_id,
            fixture: fixture.name.clone(),
            profile: run.profile,
            desktop: run.desktop,
            seed: fixture.seed,
            timeline: fixture.timeline.clone(),
            assertions: fixture.assertions.clone(),
            source_root: run.source_root,
            artifact_root: run.artifact_root,
            metadata: fixture.metadata.clone(),
        }
    }
}

/// Where and how one fixture run executes.
#[derive(Clone, Debug)]
pub struct RunTarget {
    pub run_id: String,
    pub profile: String,
    pub desktop: bool,
    pub source_root: PathBuf,
    pub artifact_root: PathBuf,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Observation {
    pub selector: String,
    pub value: Value,
    pub frame: u64,
    pub phase: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Capture {
    pub path: String,
    pub frame: u64,
    pub phase: String,
    pub origin: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Teardown {
    pub completed: bool,
    pub native_worlds: u64,
    pub managed_roots: u64,
    pub pending_readbacks: u64,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Diagnostic {
    pub status: Status,
    pub message: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorkerReport {
    pub schema_version: u32,
    pub evidence_label: String,
    pub observations: Vec<Observation>,
    pub captures: Vec<Capture>,
    pub diagnostics: Vec<Diagnostic>,
    pub teardown: Teardown,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AssertionResult {
    pub assertion: Assertion,
    pub passed: bool,
    pub message: String,
    #[serde(default)]
    pub expected: Value,
    #[serde(default)]
    pub actual: Value,
    #[serde(default)]
    pub unit: Option<String>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ValidatedCapture {
    #[serde(flatten)]
    pub capture: Capture,
    pub width: u32,
    pub height: u32,
    pub sha256: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum TckRunSchema {
    #[serde(rename = "real-disunity.tck-run/1")]
    V1,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RunRecord {
    pub schema: TckRunSchema,
    pub schema_version: u32,
    pub run_id: String,
    pub fixture: String,
    pub profile: String,
    pub evidence_label: String,
    pub status: Status,
    pub run_dir: PathBuf,
    pub started_ms: u128,
    pub elapsed_ms: u128,
    pub messages: Vec<String>,
    pub assertions: Vec<AssertionResult>,
    pub captures: Vec<ValidatedCapture>,
    pub worker: Option<WorkerReport>,
    #[serde(default)]
    pub reporting: crate::reporting::Context,
}
