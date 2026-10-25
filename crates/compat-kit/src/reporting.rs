use crate::model::{RunRecord, Status};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

pub const DIMENSIONS: [&str; 6] = [
    "compile",
    "content",
    "semantic",
    "desktop",
    "exploratory",
    "fidelity",
];

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Selection {
    pub case_id: Option<String>,
    pub game: Option<String>,
    pub assertion_dimensions: BTreeMap<usize, String>,
    pub diagnostic_dimensions: BTreeMap<usize, String>,
    pub units: BTreeMap<usize, String>,
    pub required_dimensions: Vec<String>,
}
impl Selection {
    pub fn from_fixture(fixture: &crate::Fixture) -> Result<Self, String> {
        let value = fixture
            .metadata
            .get("reporting")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}));
        let selection: Self = serde_json::from_value(value).map_err(crate::files::io)?;
        if selection
            .assertion_dimensions
            .iter()
            .any(|(i, d)| *i >= fixture.assertions.len() || !DIMENSIONS.contains(&d.as_str()))
            || selection
                .diagnostic_dimensions
                .values()
                .any(|d| !DIMENSIONS.contains(&d.as_str()))
            || selection
                .units
                .keys()
                .any(|i| *i >= fixture.assertions.len())
            || selection
                .required_dimensions
                .iter()
                .any(|d| !DIMENSIONS.contains(&d.as_str()))
        {
            return Err("invalid reporting assertion index or dimension".into());
        }
        Ok(selection)
    }
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum FailureKind {
    Cancelled,
    Deadline,
    Infrastructure,
    InvalidSpec,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FailureCause {
    pub kind: FailureKind,
    pub stage: String,
    pub message: String,
}
impl FailureCause {
    pub fn new(kind: FailureKind, stage: &str, message: impl Into<String>) -> Self {
        Self {
            kind,
            stage: stage.into(),
            message: message.into(),
        }
    }
    pub fn code(&self) -> i32 {
        match self.kind {
            FailureKind::Cancelled | FailureKind::Deadline => 4,
            FailureKind::Infrastructure => 3,
            FailureKind::InvalidSpec => 2,
        }
    }
}
impl From<FailureCause> for String {
    fn from(cause: FailureCause) -> Self {
        cause.message
    }
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Prerequisite {
    pub id: String,
    pub status: Status,
    pub message: String,
    #[serde(default)]
    pub dependencies: Vec<String>,
}
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct Context {
    pub case_id: Option<String>,
    pub game: Option<String>,
    pub mode: Option<String>,
    pub platform: Value,
    pub prerequisites: Vec<Prerequisite>,
    pub outcomes: BTreeMap<String, Option<Status>>,
    pub failure_causes: Vec<FailureCause>,
    pub required_dimensions: Vec<String>,
    pub exit_code: Option<i32>,
}
pub fn empty_outcomes() -> BTreeMap<String, Option<Status>> {
    DIMENSIONS.into_iter().map(|d| (d.into(), None)).collect()
}
pub fn combine(current: Status, next: Status) -> Status {
    fn rank(status: Status) -> u8 {
        match status {
            Status::Error => 9,
            Status::Fail => 8,
            Status::Flaky => 7,
            Status::Blocked => 6,
            Status::Unsupported => 5,
            Status::NotImplemented => 4,
            Status::Skipped => 3,
            Status::Inconclusive => 2,
            Status::Pass => 1,
        }
    }
    if rank(next) > rank(current) {
        next
    } else {
        current
    }
}
pub fn reduce(statuses: &[Status]) -> Option<Status> {
    for pending in [
        Status::Error,
        Status::Blocked,
        Status::Unsupported,
        Status::NotImplemented,
        Status::Skipped,
        Status::Inconclusive,
    ] {
        if statuses.contains(&pending) {
            return Some(pending);
        }
    }
    let mut status = statuses.iter().copied().reduce(combine)?;
    if matches!(status, Status::Pass | Status::Fail | Status::Flaky)
        && statuses.len() > 1
        && statuses.contains(&Status::Pass)
        && statuses.contains(&Status::Fail)
    {
        status = Status::Flaky;
    }
    Some(status)
}
pub fn exit_code(record: &RunRecord) -> i32 {
    let required_failed = record
        .reporting
        .required_dimensions
        .iter()
        .any(|d| record.reporting.outcomes.get(d) != Some(&Some(Status::Pass)));
    let default = i32::from(record.status != Status::Pass || required_failed);
    record
        .reporting
        .failure_causes
        .iter()
        .map(FailureCause::code)
        .chain((record.status == Status::Error).then_some(3))
        .fold(default, i32::max)
}
#[derive(Clone, Debug, Serialize)]
pub struct Group {
    pub identity: Value,
    pub status: Status,
    pub outcomes: BTreeMap<String, Option<Status>>,
    pub attempts: Vec<String>,
    pub exit_code: i32,
}
fn identity(record: &RunRecord) -> Value {
    let context = &record.reporting;
    let incomplete = context.mode.is_none() || context.platform.is_null();
    serde_json::json!({"fixture":record.fixture,"profile":record.profile,"evidence_label":record.evidence_label,
        "case_id":context.case_id,"game":context.game,"mode":context.mode,"platform":context.platform,
        "unknown_attempt":incomplete.then_some(&record.run_id)})
}
pub fn groups(records: &[RunRecord]) -> Vec<Group> {
    let mut buckets: BTreeMap<String, Vec<&RunRecord>> = BTreeMap::new();
    for record in records {
        buckets
            .entry(identity(record).to_string())
            .or_default()
            .push(record);
    }
    buckets
        .into_values()
        .map(|attempts| {
            let statuses: Vec<_> = attempts.iter().map(|r| r.status).collect();
            let outcomes = DIMENSIONS
                .into_iter()
                .map(|d| {
                    let statuses: Vec<_> = attempts
                        .iter()
                        .filter_map(|r| r.reporting.outcomes.get(d).copied().flatten())
                        .collect();
                    (d.into(), reduce(&statuses))
                })
                .collect();
            Group {
                identity: identity(attempts[0]),
                status: reduce(&statuses).unwrap_or(Status::Inconclusive),
                outcomes,
                attempts: attempts.iter().map(|r| r.run_id.clone()).collect(),
                exit_code: attempts.iter().map(|r| exit_code(r)).max().unwrap_or(1),
            }
        })
        .collect()
}
