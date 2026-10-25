use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fmt::Write as _,
    fs,
    io::{BufRead, BufReader, Read, Write},
    net::TcpStream,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
};

pub struct Server {
    child: Child,
    pub address: String,
    pub announcement: String,
}
impl Server {
    pub fn launch(root: &Path) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_compat-kit"))
            .env("COMPAT_ROOT", root)
            .env_remove("COMPAT_RUNS")
            .args(["serve", "--port", "0"])
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let mut announcement = String::new();
        BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut announcement)
            .unwrap();
        let info: Value = serde_json::from_str(&announcement).unwrap();
        Self {
            child,
            address: info["address"].as_str().unwrap().into(),
            announcement,
        }
    }
    pub fn get(&self, path: &str) -> Response {
        exchange(&self.address, "GET", path, &Value::Null, &[])
    }
    pub fn token(&self) -> String {
        let page = self.get("/");
        assert_eq!(page.status, 200);
        let token = page
            .body
            .split("name=\"compat-token\" content=\"")
            .nth(1)
            .and_then(|s| s.split('"').next())
            .expect("page launch token");
        assert_eq!(token.len(), 64);
        assert!(
            token
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        );
        token.into()
    }
    pub fn post(&self, path: &str, body: &Value, token: &str) -> Response {
        exchange(
            &self.address,
            "POST",
            path,
            body,
            &[
                ("Origin", &format!("http://{}", self.address)),
                ("X-Compat-Token", token),
            ],
        )
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
pub struct Response {
    pub status: u16,
    pub headers: BTreeMap<String, String>,
    pub body: String,
}
pub fn exchange(
    address: &str,
    method: &str,
    path: &str,
    body: &Value,
    headers: &[(&str, &str)],
) -> Response {
    let mut stream = TcpStream::connect(address).unwrap();
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(5)))
        .unwrap();
    let body = body.to_string();
    let mut request = format!("{method} {path} HTTP/1.1\r\n");
    if !headers
        .iter()
        .any(|(key, _)| key.eq_ignore_ascii_case("host"))
    {
        write!(request, "Host: {address}\r\n").unwrap();
    }
    write!(
        request,
        "Content-Type: application/json\r\nContent-Length: {}\r\n",
        body.len()
    )
    .unwrap();
    for (key, value) in headers {
        write!(request, "{key}: {value}\r\n").unwrap();
    }
    write!(request, "Connection: close\r\n\r\n{body}").unwrap();
    stream.write_all(request.as_bytes()).unwrap();
    let mut response = String::new();
    if let Err(error) = stream.read_to_string(&mut response) {
        // Early header rejection may close with unread request bytes on Linux.
        assert_eq!(error.kind(), std::io::ErrorKind::ConnectionReset);
    }
    let (head, body) = response.split_once("\r\n\r\n").unwrap();
    let mut lines = head.split("\r\n");
    let status = lines
        .next()
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap()
        .parse()
        .unwrap();
    let headers: BTreeMap<String, String> = lines
        .map(|line| {
            let (key, value) = line.split_once(':').unwrap();
            (key.to_ascii_lowercase(), value.trim().into())
        })
        .collect();
    assert_eq!(
        body.len(),
        headers["content-length"].parse::<usize>().unwrap()
    );
    Response {
        status,
        headers,
        body: body.into(),
    }
}
pub fn root(mode: &str) -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir_all(root.path().join("fixtures")).unwrap();
    let fixture = json!({"schema_version":1,"name":"harness-only","game":"snake",
        "profiles":["test"],"default_profile":"test","evidence_label":"harness-only",
        "worker":{"program":env!("CARGO_BIN_EXE_compat-kit-protocol-test-worker"),"args":[mode]},
        "limits":{"deadline_ms":3000,"log_bytes":1024,"artifact_bytes":1_048_576,"capture_count":4},
        "assertions":[{"op":"equal","selector":"counter","pointer":"/count","value":7}]});
    fs::write(
        root.path().join("fixtures/harness-only.json"),
        fixture.to_string(),
    )
    .unwrap();
    root
}
pub fn snapshot(root: &Path) -> BTreeMap<PathBuf, Option<Vec<u8>>> {
    fn visit(root: &Path, path: &Path, files: &mut BTreeMap<PathBuf, Option<Vec<u8>>>) {
        for entry in fs::read_dir(path).unwrap() {
            let path = entry.unwrap().path();
            let directory = path.is_dir();
            files.insert(
                path.strip_prefix(root).unwrap().into(),
                if directory {
                    None
                } else {
                    Some(fs::read(&path).unwrap())
                },
            );
            if directory {
                visit(root, &path, files);
            }
        }
    }
    let mut files = BTreeMap::new();
    visit(root, root, &mut files);
    files
}
