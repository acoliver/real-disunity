use crate::{
    files::{self, Result},
    model::{Assertion, AssertionResult, Observation, ValidatedCapture, WorkerReport},
};
use std::{fs, io::BufReader, path::Path};

fn values<'a>(
    observations: &'a [Observation],
    selector: &str,
    pointer: &str,
    frame: Option<u64>,
    phase: Option<&str>,
) -> Vec<Option<&'a serde_json::Value>> {
    observations
        .iter()
        .filter(|o| {
            o.selector == selector
                && frame.is_none_or(|f| o.frame == f)
                && phase.is_none_or(|p| o.phase == p)
        })
        .map(|o| o.value.pointer(pointer))
        .collect()
}
pub fn evaluate(
    assertions: &[Assertion],
    worker: &WorkerReport,
    captures: &[ValidatedCapture],
) -> Vec<AssertionResult> {
    assertions
        .iter()
        .map(|assertion| {
            let passed = scalar(assertion, &worker.observations)
                .unwrap_or_else(|| sequence(assertion, &worker.observations, captures));
            AssertionResult {
                assertion: assertion.clone(),
                passed,
                expected: serde_json::to_value(assertion).unwrap_or_default(),
                actual: crate::detail::actual(assertion, worker, captures),
                unit: None,
                message: crate::detail::assertion(assertion, worker, captures),
            }
        })
        .collect()
}
fn scalar(assertion: &Assertion, observations: &[Observation]) -> Option<bool> {
    match assertion {
        Assertion::Equal {
            selector,
            pointer,
            value,
            frame,
            phase,
        } => Some(
            values(observations, selector, pointer, *frame, phase.as_deref())
                .last()
                .and_then(|actual| *actual)
                .is_some_and(|actual| actual == value),
        ),
        Assertion::Tolerance {
            selector,
            pointer,
            value,
            tolerance,
            frame,
            phase,
        } => Some(
            value.is_finite()
                && tolerance.is_finite()
                && *tolerance >= 0.0
                && values(observations, selector, pointer, *frame, phase.as_deref())
                    .last()
                    .and_then(|v| v.and_then(serde_json::Value::as_f64))
                    .is_some_and(|actual| (actual - value).abs() <= *tolerance),
        ),
        _ => None,
    }
}
fn sequence(
    assertion: &Assertion,
    observations: &[Observation],
    captures: &[ValidatedCapture],
) -> bool {
    match assertion {
        Assertion::Changed {
            selector,
            pointer,
            before,
            after,
            phase,
        } => {
            let a = values(
                observations,
                selector,
                pointer,
                Some(*before),
                phase.as_deref(),
            );
            let b = values(
                observations,
                selector,
                pointer,
                Some(*after),
                phase.as_deref(),
            );
            before < after
                && a.last()
                    .copied()
                    .flatten()
                    .zip(b.last().copied().flatten())
                    .is_some_and(|(a, b)| a != b)
        }
        Assertion::EventOrder {
            selector,
            pointer,
            events,
        } => {
            let actual = values(observations, selector, pointer, None, None);
            !events.is_empty()
                && actual.len() == events.len()
                && actual
                    .iter()
                    .zip(events)
                    .all(|(a, b)| a.is_some_and(|a| a == b))
        }
        Assertion::Capture {
            frame,
            phase,
            origin,
        } => captures.iter().any(|c| {
            c.capture.frame == *frame && c.capture.phase == *phase && c.capture.origin == *origin
        }),
        _ => false,
    }
}

pub fn validate(
    worker: &WorkerReport,
    artifact_root: &Path,
    capture_limit: usize,
) -> Result<Vec<ValidatedCapture>> {
    if worker.schema_version != 1 {
        return Err("unsupported worker response schema".into());
    }
    if worker.observations.len() > 100_000 || worker.captures.len() > capture_limit {
        return Err("worker evidence count cap exceeded".into());
    }
    if worker
        .observations
        .windows(2)
        .any(|w| w[0].frame > w[1].frame)
    {
        return Err("observation frame order is invalid".into());
    }
    worker
        .captures
        .iter()
        .map(|capture| {
            let path = files::safe_file(artifact_root, Path::new(&capture.path))
                .map_err(|e| format!("capture: {e}"))?;
            if fs::metadata(&path).map_err(files::io)?.len() > 67_108_864 {
                return Err("PNG compressed byte cap exceeded".into());
            }
            let input = BufReader::new(fs::File::open(&path).map_err(files::io)?);
            let decoder = png::Decoder::new_with_limits(input, png::Limits { bytes: 67_108_864 });
            let mut reader = decoder
                .read_info()
                .map_err(|e| format!("PNG decode: {e}"))?;
            let size = reader
                .output_buffer_size()
                .ok_or("PNG decode size overflow")?;
            if size > 67_108_864 || reader.info().width > 8192 || reader.info().height > 8192 {
                return Err("PNG decoded size cap exceeded".into());
            }
            let mut pixels = vec![0; size];
            let info = reader
                .next_frame(&mut pixels)
                .map_err(|e| format!("PNG decode: {e}"))?;
            reader
                .finish()
                .map_err(|e| format!("PNG validation: {e}"))?;
            Ok(ValidatedCapture {
                capture: capture.clone(),
                width: info.width,
                height: info.height,
                sha256: files::hash(&path)?,
            })
        })
        .collect()
}

pub fn validate_teardown(worker: &WorkerReport) -> Result<()> {
    let teardown = &worker.teardown;
    if !teardown.completed
        || teardown.native_worlds != 0
        || teardown.managed_roots != 0
        || teardown.pending_readbacks != 0
    {
        return Err("teardown incomplete or leaked native worlds/managed roots/readbacks".into());
    }
    Ok(())
}
