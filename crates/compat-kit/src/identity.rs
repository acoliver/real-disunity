use crate::{
    files,
    model::{CommandSpec, Fixture, RunRecord},
    reporting::{Context, Selection, empty_outcomes},
};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    env,
    path::{Path, PathBuf},
};

pub fn digest(value: &impl serde::Serialize) -> Option<String> {
    serde_json::to_vec(value)
        .ok()
        .map(|bytes| format!("{:x}", Sha256::digest(bytes)))
}
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
fn program_path(program: &str, root: &Path) -> Option<PathBuf> {
    if program.contains('/') {
        return Some(root.join(program));
    }
    env::var_os("PATH").and_then(|path| {
        env::split_paths(&path)
            .map(|p| p.join(program))
            .find(|p| p.is_file())
    })
}
pub fn provider(root: &Path, command: &CommandSpec, selection: &Selection) -> Option<String> {
    let program = program_path(&command.program, root)?;
    let mut inputs = BTreeMap::from([(
        program.to_string_lossy().into_owned(),
        files::hash(&program).ok()?,
    )]);
    for argument in &command.args {
        let path = root.join(argument);
        if path.is_file() {
            inputs.insert(
                path.to_string_lossy().into_owned(),
                files::hash(&path).ok()?,
            );
        }
    }
    for input in selection.provider_inputs.as_ref()? {
        let path = root.join(input);
        inputs.insert(
            path.to_string_lossy().into_owned(),
            files::hash(&path).ok()?,
        );
    }
    digest(&inputs)
}
pub fn qualify(
    fixture: &Fixture,
    command: &CommandSpec,
    root: &Path,
    record: &mut RunRecord,
) -> Result<(), String> {
    let selection = Selection::from_fixture(fixture)?;
    record.reporting.identities.source = record.source_before.as_ref().and_then(digest);
    record.reporting.identities.overlay = overlay(root, fixture, &selection);
    record.reporting.identities.provider = provider(root, command, &selection);
    if let Some(path) = program_path(&command.program, root)
        && let Ok(hash) = files::hash(&path)
    {
        record
            .reporting
            .provider_files
            .insert(path.to_string_lossy().into_owned(), hash);
    }
    record.reporting.identities.test = digest(
        &json!({"assertions":fixture.assertions,"timeline":fixture.timeline,
        "seed":fixture.seed,"worker":fixture.worker,"limits":fixture.limits,"selection":selection}),
    );
    Ok(())
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
    program_path(&command.program, root).is_some_and(|p| p.is_file())
}

pub fn finish(
    fixture: &Fixture,
    command: &CommandSpec,
    root: &Path,
    record: &mut RunRecord,
) -> Result<(), String> {
    let selection = Selection::from_fixture(fixture)?;
    let overlay_after = overlay(root, fixture, &selection);
    if overlay_after != record.reporting.identities.overlay {
        record.status = crate::reporting::combine(record.status, crate::Status::Fail);
        record
            .messages
            .push("overlay input changed during run".into());
        record.reporting.identities.overlay =
            digest(&(record.reporting.identities.overlay.clone(), overlay_after));
    }
    let after = provider(root, command, &selection);
    if after != record.reporting.identities.provider {
        record.status = crate::reporting::combine(record.status, crate::Status::Fail);
        record
            .messages
            .push("provider input changed during run".into());
        record.reporting.identities.provider =
            digest(&(record.reporting.identities.provider.clone(), after));
    }
    record.reporting.identities.source = digest(&(&record.source_before, &record.source_after));
    Ok(())
}

fn overlay(root: &Path, fixture: &Fixture, selection: &Selection) -> Option<String> {
    let mut inputs = BTreeMap::new();
    for input in selection.overlay_inputs.as_ref()? {
        let path = root.join(input);
        inputs.insert(input, files::hash(&path).ok()?);
    }
    digest(&(fixture.metadata.clone(), inputs))
}
