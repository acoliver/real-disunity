use crate::{
    files::{self, Result, io},
    fixture,
    model::RunRecord,
};
use serde_json::{Value, json};
use std::fmt::Write;
use std::{
    fs,
    path::{Path, PathBuf},
};
fn valid_identity(root: &Path, path: &Path, record: &RunRecord) -> bool {
    record.schema_version == 1
        && path
            .file_name()
            .is_some_and(|name| name == record.run_id.as_str())
        && files::relative_path(&record.run_dir).is_ok()
        && root.join(&record.run_dir) == path
}

pub fn records(root: &Path) -> Result<(Vec<RunRecord>, Vec<String>)> {
    let runs = files::runs_root(root)?;
    let mut records = Vec::new();
    let mut errors = Vec::new();
    if !runs.exists() {
        return Ok((records, errors));
    }
    let mut report_bytes = 0_u64;
    for path in recent_paths(&runs)? {
        report_bytes =
            report_bytes.saturating_add(fs::metadata(path.join("run.json")).map_or(0, |m| m.len()));
        if report_bytes > 67_108_864 {
            return Err(
                "dashboard report byte cap reached; refusing an incomplete aggregate".into(),
            );
        }
        if fs::symlink_metadata(&path)
            .map_err(io)?
            .file_type()
            .is_symlink()
        {
            errors.push(format!("symlink report rejected: {}", path.display()));
            continue;
        }
        let report = path.join("run.json");
        if !report.exists() {
            errors.push(format!("unfinished run: {}", path.display()));
            continue;
        }
        match files::read_json::<RunRecord>(&report) {
            Ok(record) if valid_identity(root, &path, &record) => records.push(record),
            Ok(_) => errors.push(format!("invalid report identity: {}", report.display())),
            Err(error) => errors.push(format!("corrupt report {}: {error}", report.display())),
        }
    }
    records.sort_by_key(|record| std::cmp::Reverse(record.started_ms));
    Ok((records, errors))
}
fn recent_paths(runs: &Path) -> Result<Vec<PathBuf>> {
    let mut paths = fs::read_dir(runs)
        .map_err(io)?
        .take(10_001)
        .map(|e| e.map(|e| e.path()).map_err(io))
        .collect::<Result<Vec<_>>>()?;
    if paths.len() > 10_000 {
        return Err("dashboard report entry cap reached; refusing an incomplete aggregate".into());
    }
    paths.sort_by_key(|p| {
        std::cmp::Reverse(
            fs::metadata(p.join("run.json"))
                .and_then(|m| m.modified())
                .ok(),
        )
    });
    Ok(paths)
}
pub fn state(root: &Path) -> Result<Value> {
    let (runs, errors) = records(root)?;
    Ok(
        json!({"fixtures":fixture::catalog(root)?,"groups":crate::reporting::groups(&runs),"runs":runs,"errors":errors}),
    )
}
pub fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
fn capture_source(
    root: &Path,
    record: &RunRecord,
    capture: &crate::model::ValidatedCapture,
) -> Result<PathBuf> {
    files::relative_path(Path::new(&record.run_id))?;
    if Path::new(&record.run_id).components().count() != 1 {
        return Err("invalid run id".into());
    }
    let run = files::runs_root(root)?.join(&record.run_id);
    let path = files::safe_file(&run, Path::new(&capture.capture.path))?;
    if files::hash(&path)? != capture.sha256 {
        return Err("capture changed since validation".into());
    }
    Ok(path)
}
pub fn page(root: &Path, output: Option<&Path>) -> Result<String> {
    render_page(root, output, None)
}
pub(crate) fn live_page(root: &Path, token: &str) -> Result<String> {
    render_page(root, None, Some(token))
}
fn render_page(root: &Path, output: Option<&Path>, token: Option<&str>) -> Result<String> {
    let fixtures = fixture::catalog(root)?;
    let (runs, errors) = records(root)?;
    render_records(root, output, token, &fixtures, &runs, &errors)
}
fn render_records(
    root: &Path,
    output: Option<&Path>,
    token: Option<&str>,
    fixtures: &[crate::Fixture],
    runs: &[RunRecord],
    errors: &[String],
) -> Result<String> {
    let mut html = String::from("<!doctype html><html lang=\"en\"><meta charset=\"utf-8\">");
    if let Some(token) = token {
        write!(
            html,
            "<meta name=\"compat-token\" content=\"{}\">",
            escape(token)
        )
        .map_err(io)?;
    }
    html.push_str("<meta name=\"viewport\" content=\"width=device-width\"><title>Compatibility runs</title><style>body{font:16px system-ui;background:#171b22;color:#eef;margin:2em}table{border-collapse:collapse}td,th{padding:.6em;border:1px solid #667}a{color:#9cf}img{max-width:640px;max-height:360px}pre{white-space:pre-wrap;overflow-wrap:anywhere}button,select{padding:.4em;margin:.2em}.pass{color:#9e9}.fail,.error{color:#faa}.blocked,.inconclusive{color:#fd9}</style><h1>Compatibility runs</h1><p>Results contain measured worker evidence. Harness-only runs exercise the protocol infrastructure. Unattempted profiles show no record. Unity 6.3 (6000.3) and legacy Unity 2017.1 are distinct source/API targets. Profiles execute this runtime facade; no official Unity player/compiler is used.</p><div id=\"controls\">");
    html.push_str(&matrix(fixtures)?);
    html.push_str(&crate::report_view::matrix(fixtures, runs)?);
    html.push_str(&history(root, runs, output)?);
    html.push_str("<h2>Report diagnostics</h2>");
    for error in errors {
        write!(html, "<p class=\"error\">{}</p>", escape(error)).map_err(io)?;
    }
    html.push_str("<script>");
    html.push_str(include_str!("dashboard.js"));
    html.push_str("</script></html>");
    Ok(html)
}
fn matrix(fixtures: &[crate::model::Fixture]) -> Result<String> {
    let mut html = String::from("<select id=\"fixture\">");
    for fixture in fixtures {
        write!(html, "<option>{}</option>", escape(&fixture.name)).map_err(io)?;
    }
    html.push_str("</select><select id=\"profile\"><option value=\"\">Default fixture profile</option></select><label><input id=\"desktop\" type=\"checkbox\">Desktop</label><button onclick=\"runFixture()\">Run</button><button onclick=\"refreshFixture()\">Refresh source</button><label><input id=\"accept\" type=\"checkbox\">Accept qualified candidate</label><p id=\"action\"></p><div id=\"tasks\"></div></div>");
    Ok(html)
}

fn history(root: &Path, runs: &[RunRecord], output: Option<&Path>) -> Result<String> {
    let mut html = String::from("<h2>History and evidence</h2>");
    for record in runs {
        write!(
            html,
            "<details {}><summary>{} / {} / {} / {:?}</summary><pre>{}</pre>",
            crate::report_view::attributes(record, "attempt"),
            escape(&record.fixture),
            escape(&record.profile),
            escape(&record.run_id),
            record.status,
            escape(&serde_json::to_string_pretty(record).map_err(io)?)
        )
        .map_err(io)?;
        match request_detail(root, record) {
            Ok(detail) => html.push_str(&detail),
            Err(error) => write!(
                html,
                r#"<p class="error">damaged request: {}</p>"#,
                escape(&error)
            )
            .map_err(io)?,
        }
        for (index, capture) in record.captures.iter().enumerate() {
            match capture_image(root, record, capture, index, output) {
                Ok(image) => html.push_str(&image),
                Err(error) => {
                    write!(
                        html,
                        "<p class=\"error\">missing capture or damaged evidence: {}</p>",
                        escape(&error)
                    )
                    .map_err(io)?;
                }
            }
        }
        html.push_str("</details>");
    }
    Ok(html)
}
fn request_detail(root: &Path, record: &RunRecord) -> Result<String> {
    let directory = files::runs_root(root)?.join(&record.run_id);
    let path = directory.join("request.json");
    if !path.exists() {
        return Ok("<p>Input timeline: worker was not started.</p>".into());
    }
    let path = files::safe_file(&directory, Path::new("request.json"))?;
    let request: crate::model::RunRequest = files::read_json(&path)?;
    Ok(format!(
        "<h3>Input timeline and source request</h3><pre>{}</pre>",
        escape(&serde_json::to_string_pretty(&request).map_err(io)?)
    ))
}

fn capture_image(
    root: &Path,
    record: &RunRecord,
    capture: &crate::model::ValidatedCapture,
    index: usize,
    output: Option<&Path>,
) -> Result<String> {
    let source = capture_source(root, record, capture)?;
    let url = if let Some(output) = output {
        let relative = format!("images/{}-{index}.png", record.run_id);
        let target = files::work_path(root, &output.join(&relative))?;
        fs::create_dir_all(target.parent().ok_or("image parent missing")?).map_err(io)?;
        fs::copy(source, target).map_err(io)?;
        relative
    } else {
        format!("/files/{}/{}", record.run_id, capture.capture.path)
    };
    Ok(format!(
        "<figure><img src=\"{}\" alt=\"{} frame {}\"><figcaption>{} {}x{}, frame {}, {}</figcaption></figure>",
        escape(&url),
        escape(&capture.capture.origin),
        capture.capture.frame,
        escape(&capture.capture.origin),
        capture.width,
        capture.height,
        capture.capture.frame,
        escape(&capture.capture.phase)
    ))
}
pub fn generate(root: &Path, requested: &Path) -> Result<PathBuf> {
    let output = files::work_path(root, requested)?;
    fs::create_dir_all(&output).map_err(io)?;
    let fixtures = fixture::catalog(root)?;
    let (runs, errors) = records(root)?;
    let html = render_records(root, Some(&output), None, &fixtures, &runs, &errors)?;
    files::atomic_write(&output.join("index.html"), html.as_bytes())?;
    files::write_json(
        &output.join("state.json"),
        &json!({"fixtures":fixtures,"groups":crate::reporting::groups(&runs),"runs":runs,"errors":errors}),
    )?;
    files::atomic_write(
        &output.join("junit.xml"),
        crate::junit::report(&runs)?.as_bytes(),
    )?;
    Ok(output.join("index.html"))
}
