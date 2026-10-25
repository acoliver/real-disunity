//! Bounded invocation of the .NET SDK.
use compat_kit::files;
use std::{
    fs,
    path::Path,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

const MAX_LOG_BYTES: u64 = 4 * 1024 * 1024;

/// A `dotnet` command. `DOTNET_HOST_PATH` selects a specific SDK host; otherwise `PATH` is used.
pub(crate) fn command(root: &Path) -> Command {
    let host = std::env::var_os("DOTNET_HOST_PATH").unwrap_or_else(|| "dotnet".into());
    let mut command = Command::new(host);
    command.current_dir(root);
    command
}

/// Runs `command` with output captured in `log`, killing it when it exceeds `timeout` or the
/// log budget.
pub(crate) fn run(command: &mut Command, log: &Path, timeout: Duration) -> Result<(), String> {
    let output = fs::File::create(log).map_err(files::io)?;
    let mut child = command
        .stdin(Stdio::null())
        .stdout(output.try_clone().map_err(files::io)?)
        .stderr(output)
        .spawn()
        .map_err(|error| format!("dotnet could not start: {error}"))?;
    let started = Instant::now();
    loop {
        let status = child.try_wait().map_err(files::io)?;
        let oversized = fs::metadata(log).map_err(files::io)?.len() > MAX_LOG_BYTES;
        if oversized || started.elapsed() > timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!(
                "dotnet exceeded its time or log budget: {}",
                log.display()
            ));
        }
        if let Some(status) = status {
            return if status.success() {
                Ok(())
            } else {
                Err(format!("dotnet failed ({status}); see {}", log.display()))
            };
        }
        thread::sleep(Duration::from_millis(20));
    }
}
