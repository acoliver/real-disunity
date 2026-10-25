mod security;
use crate::{
    dashboard,
    files::{self, Result, io},
    fixture,
    process::RunControl,
    runner,
};
use security::LaunchSecurity;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    net::{Shutdown, TcpListener, TcpStream},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

struct Task {
    control: RunControl,
    kind: String,
    fixture: String,
    profile: String,
    done: bool,
    result: Value,
}
type Tasks = Arc<Mutex<BTreeMap<String, Task>>>;

pub fn serve(root: &Path, port: u16) -> Result<()> {
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port)).map_err(io)?;
    let address = listener.local_addr().map_err(io)?.to_string();
    let security = LaunchSecurity::new(&address)?;
    println!(
        "{}",
        json!({"address":address,"url":format!("http://{address}/")})
    );
    std::io::stdout().flush().map_err(io)?;
    let tasks: Tasks = Arc::default();
    for stream in listener.incoming() {
        let mut stream = stream.map_err(io)?;
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .map_err(io)?;
        stream
            .set_write_timeout(Some(Duration::from_secs(5)))
            .map_err(io)?;
        // Requests are bounded and handled serially; long-running work always runs in a controlled job.
        let response = handle(&mut stream, root, &security, &tasks);
        let (status, mime, bytes) = match response {
            Ok(response) => response,
            Err(error) => (
                400,
                "application/json",
                serde_json::to_vec(&json!({"error":error})).map_err(io)?,
            ),
        };
        let _ = respond(&mut stream, status, mime, &bytes);
        finish_connection(&mut stream);
    }
    Ok(())
}
fn respond(stream: &mut TcpStream, status: u16, mime: &str, bytes: &[u8]) -> Result<()> {
    let reason = match status {
        200 => "OK",
        202 => "Accepted",
        403 => "Forbidden",
        404 => "Not Found",
        _ => "Bad Request",
    };
    write!(
        stream,
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {mime}\r\nContent-Length: {}\r\nConnection: close\r\nX-Content-Type-Options: nosniff\r\nCache-Control: no-store\r\nX-Frame-Options: DENY\r\nReferrer-Policy: no-referrer\r\nContent-Security-Policy: default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src 'self'; connect-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'none'; object-src 'none'\r\n\r\n",
        bytes.len()
    ).map_err(io)?;
    stream.write_all(bytes).map_err(io)
}
fn finish_connection(stream: &mut TcpStream) {
    if stream.shutdown(Shutdown::Write).is_err() {
        return;
    }
    // Closing with unread rejected input can reset TCP and truncate the response.
    let deadline = std::time::Instant::now() + Duration::from_millis(50);
    let mut remaining = 1_048_576;
    let mut buffer = [0; 4096];
    while remaining > 0 {
        let timeout = deadline.saturating_duration_since(std::time::Instant::now());
        if timeout.is_zero() || stream.set_read_timeout(Some(timeout)).is_err() {
            break;
        }
        let length = remaining.min(buffer.len());
        match stream.read(&mut buffer[..length]) {
            Ok(0) | Err(_) => break,
            Ok(count) => remaining -= count,
        }
    }
}
struct Request {
    method: String,
    path: String,
    headers: BTreeMap<String, String>,
    body: Vec<u8>,
}
fn request(stream: &mut TcpStream) -> Result<Request> {
    let mut bytes = Vec::new();
    let mut buffer = [0; 4096];
    let end = loop {
        let count = stream.read(&mut buffer).map_err(io)?;
        if count == 0 {
            return Err("incomplete HTTP headers".into());
        }
        bytes.extend_from_slice(&buffer[..count]);
        if let Some(position) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
            break position + 4;
        }
        if bytes.len() > 16_384 {
            return Err("HTTP headers too large".into());
        }
    };
    if end > 16_384 {
        return Err("HTTP headers too large".into());
    }
    let header = std::str::from_utf8(&bytes[..end]).map_err(io)?;
    let mut lines = header.split("\r\n");
    let mut start = lines
        .next()
        .ok_or("missing HTTP request")?
        .split_whitespace();
    let method = start.next().ok_or("missing method")?.to_owned();
    let path = start.next().ok_or("missing path")?.to_owned();
    let mut headers = BTreeMap::new();
    for line in lines.filter(|line| !line.is_empty()) {
        let (key, value) = line.split_once(':').ok_or("invalid HTTP header")?;
        if headers
            .insert(key.to_ascii_lowercase(), value.trim().to_owned())
            .is_some()
        {
            return Err("duplicate HTTP header".into());
        }
    }
    if headers.contains_key("transfer-encoding") {
        return Err("chunked HTTP requests unsupported".into());
    }
    let length: usize = headers
        .get("content-length")
        .map_or(Ok(0), |v| v.parse().map_err(io))?;
    if length > 65_536 {
        return Err("HTTP body too large".into());
    }
    while bytes.len() < end + length {
        let count = stream.read(&mut buffer).map_err(io)?;
        if count == 0 {
            return Err("incomplete HTTP body".into());
        }
        bytes.extend_from_slice(&buffer[..count]);
    }
    Ok(Request {
        method,
        path,
        headers,
        body: bytes[end..end + length].to_vec(),
    })
}
type Response = (u16, &'static str, Vec<u8>);
fn handle(
    stream: &mut TcpStream,
    root: &Path,
    security: &LaunchSecurity,
    tasks: &Tasks,
) -> Result<Response> {
    let req = request(stream)?;
    if !security.trusts_peer(&req) {
        return json_response(403, &json!({"error":"Host or Origin rejected"}));
    }
    if req.path.contains('%') || req.path.contains('?') || req.path.contains('#') {
        return Err("encoded or parameterized paths unsupported".into());
    }
    if req.method == "GET" {
        return get(&req, root, security, tasks);
    }
    if !security.authorizes_mutation(&req) {
        return json_response(403, &json!({"error":"launch token required"}));
    }
    authorize_post(&req)?;
    let body: Value = serde_json::from_slice(&req.body).map_err(io)?;
    if req.path == "/api/stop" {
        let id = body["id"].as_str().ok_or("task id required")?;
        let tasks = tasks.lock().map_err(io)?;
        let task = tasks.get(id).ok_or("unknown task id")?;
        task.control.cancel();
        return json_response(202, &json!({"id":id,"cancel_requested":true}));
    }
    if req.path != "/api/run" {
        return json_response(404, &json!({"error":"unknown route"}));
    }
    start_job(root, &body, tasks)
}
fn authorize_post(req: &Request) -> Result<()> {
    if req.method != "POST" {
        return Err("method unsupported".into());
    }
    if req
        .headers
        .get("content-type")
        .is_none_or(|s| s != "application/json")
    {
        return Err("application/json required".into());
    }
    Ok(())
}
fn get(req: &Request, root: &Path, security: &LaunchSecurity, tasks: &Tasks) -> Result<Response> {
    if req.path == "/" {
        return Ok((
            200,
            "text/html; charset=utf-8",
            dashboard::live_page(root, security.token())?.into_bytes(),
        ));
    }
    if req.path == "/api/state" {
        let mut state = dashboard::state(root)?;
        let tasks = tasks.lock().map_err(io)?;
        state["tasks"] = json!(
            tasks
                .iter()
                .map(|(id, t)| json!({"id":id,"kind":t.kind,"fixture":t.fixture,"profile":t.profile,"done":t.done,"result":t.result}))
                .collect::<Vec<_>>()
        );
        return json_response(200, &state);
    }
    if let Some(relative) = req.path.strip_prefix("/files/") {
        let relative = Path::new(relative);
        files::relative_path(relative)?;
        let base = files::runs_root(root)?;
        let path = files::safe_file(&base, relative)?;
        if path.extension().is_none_or(|e| e != "png") {
            return Err("only validated PNG captures can be served".into());
        }
        let (runs, _) = dashboard::records(root)?;
        let authorized = runs.iter().any(|r| {
            r.captures.iter().any(|c| {
                PathBuf::from(&r.run_id).join(&c.capture.path) == relative
                    && files::hash(&path).is_ok_and(|hash| hash == c.sha256)
            })
        });
        if !authorized {
            return Err("capture absent from validated report".into());
        }
        if std::fs::metadata(&path).map_err(io)?.len() > 67_108_864 {
            return Err("capture too large".into());
        }
        return Ok((200, "image/png", std::fs::read(path).map_err(io)?));
    }
    json_response(404, &json!({"error":"unknown route"}))
}
fn start_job(root: &Path, body: &Value, tasks: &Tasks) -> Result<Response> {
    let fixture = fixture::load(root, body["fixture"].as_str().ok_or("fixture required")?)?;
    let profile = body["profile"].as_str().map(str::to_owned);
    let desktop = body["desktop"].as_bool().unwrap_or(false);
    let id = files::unique_id();
    let control = RunControl::default();
    let kind = "run";
    {
        let mut map = tasks.lock().map_err(io)?;
        if map.values().filter(|t| !t.done).count() >= 4 {
            return Err("four jobs already active".into());
        }
        if map.len() >= 1000 {
            map.retain(|_, t| !t.done);
        }
        map.insert(
            id.clone(),
            Task {
                control: control.clone(),
                kind: kind.into(),
                fixture: fixture.name.clone(),
                profile: profile
                    .clone()
                    .unwrap_or_else(|| fixture.default_profile.clone()),
                done: false,
                result: Value::Null,
            },
        );
    }
    let task_id = id.clone();
    let tasks = tasks.clone();
    let root = root.to_path_buf();
    thread::spawn(move || {
        let result = runner::run(&root, &fixture, profile.as_deref(), desktop, &control)
            .and_then(|r| serde_json::to_value(r).map_err(io));
        let value = result.unwrap_or_else(|e| json!({"status":"error","message":e}));
        if let Ok(mut tasks) = tasks.lock()
            && let Some(task) = tasks.get_mut(&task_id)
        {
            task.done = true;
            task.result = value;
        }
    });
    json_response(202, &json!({"id":id,"kind":kind}))
}
fn json_response(status: u16, value: &Value) -> Result<(u16, &'static str, Vec<u8>)> {
    Ok((
        status,
        "application/json",
        serde_json::to_vec(value).map_err(io)?,
    ))
}
