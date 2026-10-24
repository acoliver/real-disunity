use crate::model::{Assertion, Observation, ValidatedCapture, WorkerReport};
use serde_json::{Value, json};

fn selected(assertion: &Assertion, observation: &Observation) -> Option<String> {
    let (selector, pointer, frame, phase) = match assertion {
        Assertion::Equal {
            selector,
            pointer,
            frame,
            phase,
            ..
        }
        | Assertion::Tolerance {
            selector,
            pointer,
            frame,
            phase,
            ..
        } => (selector, pointer, *frame, phase.as_deref()),
        Assertion::Changed {
            selector,
            pointer,
            before,
            after,
            phase,
        } => {
            if observation.frame != *before && observation.frame != *after {
                return None;
            }
            (selector, pointer, None, phase.as_deref())
        }
        Assertion::EventOrder {
            selector, pointer, ..
        } => (selector, pointer, None, None),
        Assertion::Capture { .. } => return None,
    };
    (observation.selector == *selector
        && frame.is_none_or(|f| f == observation.frame)
        && phase.is_none_or(|p| p == observation.phase))
    .then(|| pointer.clone())
}
pub fn actual(
    assertion: &Assertion,
    worker: &WorkerReport,
    captures: &[ValidatedCapture],
) -> Value {
    if let Assertion::Capture {
        frame,
        phase,
        origin,
    } = assertion
    {
        return json!(
            captures
                .iter()
                .filter(|c| c.capture.frame == *frame
                    && c.capture.phase == *phase
                    && c.capture.origin == *origin)
                .collect::<Vec<_>>()
        );
    }
    let values: Vec<_> = worker.observations.iter().rev().filter_map(|o| {
        selected(assertion, o).map(|pointer| json!({"frame":o.frame,"phase":o.phase,
            "pointer":pointer,"present":o.value.pointer(&pointer).is_some(),"value":o.value.pointer(&pointer)}))
    }).take(17).collect();
    json!({"latest_first":values.iter().take(16).collect::<Vec<_>>(),"limit":16,"truncated":values.len()>16})
}
pub fn assertion(
    assertion: &Assertion,
    worker: &WorkerReport,
    captures: &[ValidatedCapture],
) -> String {
    format!(
        "expected {}; observed {}",
        serde_json::to_string(assertion).unwrap_or_default(),
        actual(assertion, worker, captures)
    )
}
