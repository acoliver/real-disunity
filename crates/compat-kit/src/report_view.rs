use crate::{
    dashboard::escape,
    files::{Result, io},
    model::{Fixture, RunRecord},
    reporting,
};
use std::{collections::BTreeSet, fmt::Write};

pub fn attributes(record: &RunRecord, view: &str) -> String {
    let context = &record.reporting;
    format!(
        "data-view=\"{}\" data-status=\"{}\" data-case=\"{}\" data-game=\"{}\" data-profile=\"{}\" data-platform=\"{}\"",
        escape(view),
        status(record.status),
        escape(context.case_id.as_deref().unwrap_or("unknown")),
        escape(context.game.as_deref().unwrap_or(&record.fixture)),
        escape(&record.profile),
        escape(context.platform["os"].as_str().unwrap_or("unknown"))
    )
}
fn status(status: crate::Status) -> String {
    serde_json::to_value(status)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_else(|| "error".into())
}
fn select(id: &str, label: &str, values: BTreeSet<String>) -> Result<String> {
    let mut html =
        format!("<label>{label}<select id=\"{id}-filter\"><option value=\"\">All {label}</option>");
    for value in values {
        write!(html, "<option>{}</option>", escape(&value)).map_err(io)?;
    }
    html.push_str("</select></label>");
    Ok(html)
}
fn filters(fixtures: &[Fixture], runs: &[RunRecord]) -> Result<String> {
    let mut html = String::from("<h2>Result filters</h2>");
    for (id, label) in [
        ("case", "cases"),
        ("game", "games"),
        ("profile", "profiles"),
        ("platform", "platforms"),
    ] {
        let mut values = BTreeSet::new();
        for record in runs {
            let value = match id {
                "case" => record.reporting.case_id.as_deref().unwrap_or("unknown"),
                "game" => record.reporting.game.as_deref().unwrap_or(&record.fixture),
                "profile" => &record.profile,
                _ => record.reporting.platform["os"]
                    .as_str()
                    .unwrap_or("unknown"),
            };
            values.insert(value.to_owned());
        }
        for fixture in fixtures {
            add_fixture_values(&mut values, fixture, id)?;
        }
        html.push_str(&select(id, label, values)?);
    }
    html.push_str(&status_filter()?);
    html.push_str("<label>View<select id=\"attempt-filter\"><option value=\"\">Groups and all attempts</option><option value=\"group\">Reduced groups</option><option value=\"attempt\">All attempts</option></select></label>");
    Ok(html)
}
fn add_fixture_values(values: &mut BTreeSet<String>, fixture: &Fixture, field: &str) -> Result<()> {
    let selection = reporting::Selection::from_fixture(fixture)?;
    match field {
        "case" => {
            values.insert(selection.case_id.unwrap_or_else(|| "unknown".into()));
        }
        "game" => {
            values.insert(selection.game.unwrap_or_else(|| fixture.name.clone()));
        }
        "profile" => values.extend(fixture.profiles.iter().cloned()),
        _ => {
            values.insert("unknown".into());
        }
    }
    Ok(())
}

pub fn matrix(fixtures: &[Fixture], runs: &[RunRecord]) -> Result<String> {
    let mut html = filters(fixtures, runs)?;
    html.push_str("<h2>Game and profile matrix</h2><p>Rows summarize attempts by fixture, profile, mode and platform. These summaries do not certify identical builds or inputs. Null dimension outcomes are unmeasured. All attempts remain in history.</p><table><tr><th>Fixture / profile</th><th>Reduced status</th><th>Dimensions and attempts</th></tr>");
    for group in reporting::groups(runs) {
        let record = runs
            .iter()
            .find(|r| group.attempts.contains(&r.run_id))
            .ok_or("group attempt missing")?;
        let mut display = record.clone();
        display.status = group.status;
        write!(
            html,
            "<tr {}><td>{} / {}</td><td>{}</td><td><pre>{}</pre></td></tr>",
            attributes(&display, "group"),
            escape(&record.fixture),
            escape(&record.profile),
            status(group.status),
            escape(&serde_json::to_string_pretty(&group).map_err(io)?)
        )
        .map_err(io)?;
    }
    for fixture in fixtures {
        for profile in &fixture.profiles {
            if runs
                .iter()
                .any(|r| r.fixture == fixture.name && r.profile == *profile)
            {
                continue;
            }
            let selection = reporting::Selection::from_fixture(fixture)?;
            write!(html, "<tr data-view=\"group\" data-status=\"no record\" data-case=\"{}\" data-game=\"{}\" data-profile=\"{}\" data-platform=\"unknown\"><td>{} / {}</td><td>no record</td><td>{}</td></tr>",
                escape(selection.case_id.as_deref().unwrap_or("unknown")), escape(selection.game.as_deref().unwrap_or(&fixture.name)), escape(profile), escape(&fixture.name), escape(profile), escape(&fixture.evidence_label)).map_err(io)?;
        }
    }
    html.push_str("</table>");
    Ok(html)
}

fn status_filter() -> Result<String> {
    select(
        "status",
        "statuses",
        [
            "pass",
            "fail",
            "unsupported",
            "not-implemented",
            "blocked",
            "skipped",
            "error",
            "flaky",
            "inconclusive",
            "no record",
        ]
        .map(str::to_owned)
        .into(),
    )
}
