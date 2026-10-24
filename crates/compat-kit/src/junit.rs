use crate::{
    files::{Result, io},
    model::{RunRecord, Status},
};
use std::fmt::Write;

fn escaped(value: &str) -> String {
    crate::dashboard::escape(
        &value
            .chars()
            .filter(|c| {
                matches!(c, '\t' | '\n' | '\r')
                    || (*c >= ' ' && *c != '\u{fffe}' && *c != '\u{ffff}')
            })
            .collect::<String>(),
    )
}
pub fn report(records: &[RunRecord]) -> Result<String> {
    let count = |statuses: &[Status]| {
        records
            .iter()
            .filter(|r| statuses.contains(&r.status))
            .count()
    };
    let mut xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><testsuite name=\"compat-kit attempts\" tests=\"{}\" failures=\"{}\" errors=\"{}\" skipped=\"{}\">",
        records.len(),
        count(&[Status::Fail, Status::Flaky]),
        count(&[Status::Error]),
        count(&[
            Status::Unsupported,
            Status::NotImplemented,
            Status::Blocked,
            Status::Skipped,
            Status::Inconclusive
        ])
    );
    for record in records {
        let status = serde_json::to_value(record.status).map_err(io)?;
        write!(xml, "<testcase name=\"{}\" classname=\"{}\"><properties><property name=\"status\" value=\"{}\"/><property name=\"profile\" value=\"{}\"/><property name=\"exit_code\" value=\"{}\"/></properties>",
            escaped(&record.run_id), escaped(&record.fixture), escaped(status.as_str().unwrap_or("error")),
            escaped(&record.profile), crate::reporting::exit_code(record)).map_err(io)?;
        let tag = match record.status {
            Status::Pass => None,
            Status::Fail | Status::Flaky => Some("failure"),
            Status::Error => Some("error"),
            _ => Some("skipped"),
        };
        if let Some(tag) = tag {
            write!(
                xml,
                "<{tag} message=\"{}\">{}</{tag}>",
                escaped(status.as_str().unwrap_or("error")),
                escaped(&serde_json::to_string(record).map_err(io)?)
            )
            .map_err(io)?;
        }
        xml.push_str("</testcase>");
    }
    xml.push_str("</testsuite>");
    Ok(xml)
}
