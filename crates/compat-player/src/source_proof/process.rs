use std::{
    fs::File,
    io::Read,
    path::Path,
    process::{Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

static REQUEST: AtomicU64 = AtomicU64::new(0);
const MAX_REPORT: u64 = 16 * 1024 * 1024;

pub(super) fn run(work: &Path, game: &str) -> Result<(Vec<u8>, String), String> {
    let requests = work.join("requests");
    std::fs::create_dir_all(&requests).map_err(|e| e.to_string())?;
    super::paths::anchored(work, &requests)?;
    let sequence = REQUEST.fetch_add(1, Ordering::Relaxed);
    let output = requests.join(format!("{}-{sequence}.json", std::process::id()));
    let result = invoke(work, game, &output)
        .and_then(|()| Ok((read_bounded(&output)?, compat_kit::files::hash(&output)?)));
    let cleanup = std::fs::remove_file(&output);
    if result.is_ok() {
        cleanup.map_err(|e| e.to_string())?;
    }
    result
}

fn invoke(work: &Path, game: &str, output: &Path) -> Result<(), String> {
    let mut child = Command::new("/usr/bin/python3")
        .arg(work.join("contained.py"))
        .args(["--", "/usr/bin/python3"])
        .arg(work.join("host.py"))
        .arg(game)
        .arg(output)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| e.to_string())?;
    let deadline = Instant::now() + Duration::from_mins(1);
    loop {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            return if status.success() {
                Ok(())
            } else {
                Err(format!("metadata inspector failed: {status}"))
            };
        }
        if Instant::now() >= deadline {
            Command::new("/bin/kill")
                .args(["-TERM", &child.id().to_string()])
                .status()
                .map_err(|e| e.to_string())?;
            let grace = Instant::now() + Duration::from_secs(5);
            while child.try_wait().map_err(|e| e.to_string())?.is_none() {
                if Instant::now() >= grace {
                    child.kill().map_err(|e| e.to_string())?;
                    child.wait().map_err(|e| e.to_string())?;
                    break;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            return Err("metadata inspector exceeded 60 seconds".into());
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

pub(super) fn read_bounded(path: &Path) -> Result<Vec<u8>, String> {
    let file = File::open(path).map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    file.take(MAX_REPORT + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.is_empty() || bytes.len() as u64 > MAX_REPORT {
        return Err("empty or oversized inspector report".into());
    }
    Ok(bytes)
}
