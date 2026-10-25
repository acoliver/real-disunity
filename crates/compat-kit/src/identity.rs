use crate::{
    model::{CommandSpec, Fixture, RunRecord},
    reporting::{Context, Selection, empty_outcomes},
};
use serde_json::json;
use std::{env, path::Path};

pub fn context(fixture: &Fixture, desktop: bool) -> Result<Context, String> {
    let selection = Selection::from_fixture(fixture)?;
    Ok(Context {
        case_id: selection.case_id,
        game: selection.game,
        mode: Some(
            if desktop {
                "desktop-replay"
            } else {
                "worker-default"
            }
            .into(),
        ),
        platform: json!({"os":env::consts::OS,"arch":env::consts::ARCH,
            "display":env::var("DISPLAY").ok(),"wayland_display":env::var("WAYLAND_DISPLAY").ok(),
            "adapter":null,"driver":null,"certification":null}),
        outcomes: empty_outcomes(),
        required_dimensions: selection.required_dimensions,
        ..Context::default()
    })
}
pub fn outcomes(fixture: &Fixture, record: &mut RunRecord) -> Result<(), String> {
    let selection = Selection::from_fixture(fixture)?;
    for (index, result) in record.assertions.iter_mut().enumerate() {
        result.unit = selection.units.get(&index).cloned();
    }
    for dimension in crate::reporting::DIMENSIONS {
        let statuses: Vec<_> = selection
            .assertion_dimensions
            .iter()
            .filter(|(_, d)| d.as_str() == dimension)
            .filter_map(|(i, _)| record.assertions.get(*i))
            .map(|a| {
                if a.passed {
                    crate::Status::Pass
                } else {
                    crate::Status::Fail
                }
            })
            .collect();
        // Conflicting assertions within one attempt are a failure, never a retry signal.
        let diagnostics = record
            .worker
            .iter()
            .flat_map(|w| {
                selection
                    .diagnostic_dimensions
                    .iter()
                    .filter(|(_, d)| d.as_str() == dimension)
                    .filter_map(|(i, _)| w.diagnostics.get(*i))
            })
            .map(|d| d.status)
            .filter(|s| *s != crate::Status::Pass);
        let outcome = statuses
            .into_iter()
            .chain(diagnostics)
            .reduce(crate::reporting::combine);
        record.reporting.outcomes.insert(dimension.into(), outcome);
    }
    if record.status == crate::Status::Pass
        && record
            .reporting
            .required_dimensions
            .iter()
            .any(|d| record.reporting.outcomes.get(d) != Some(&Some(crate::Status::Pass)))
    {
        record.status = crate::Status::Inconclusive;
        record
            .messages
            .push("required dimension lacks passing measured assertions".into());
    }
    Ok(())
}

pub fn available(root: &Path, command: &CommandSpec) -> bool {
    if command.program.contains('/') {
        return root.join(&command.program).is_file();
    }
    env::var_os("PATH").is_some_and(|path| {
        env::split_paths(&path).any(|directory| directory.join(&command.program).is_file())
    })
}
