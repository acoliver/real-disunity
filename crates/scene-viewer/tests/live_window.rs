//! Each desktop scenario runs on its own process's main thread and event loop.
#[cfg(target_os = "linux")]
#[path = "live_window/desktop.rs"]
mod desktop;
#[cfg(target_os = "linux")]
#[path = "live_window/lifecycle.rs"]
mod lifecycle;

#[cfg(target_os = "linux")]
#[path = "live_window/ingress.rs"]
mod ingress;

fn main() {
    #[cfg(target_os = "linux")]
    scenarios();
}

#[cfg(target_os = "linux")]
fn scenarios() {
    assert!(
        std::env::var_os("COMPAT_DISPLAY_CONTAINMENT").is_some(),
        "run desktop scenarios with tools/compat-display/run.py -- COMMAND"
    );
    if let Some(case) =
        std::env::args().find_map(|arg| arg.strip_prefix("--live-case=").map(str::to_owned))
    {
        if case == "desktop" {
            desktop::run();
        } else if case == "ingress" {
            ingress::run();
        } else if case == "ingress-acquisition" {
            ingress::acquisition();
        } else {
            lifecycle::run(&case);
        }
        return;
    }
    for case in [
        "desktop",
        "ingress",
        "ingress-acquisition",
        "ingress-error",
        "channel-budget",
        "scheduled-budget",
        "initialize-error",
        "update-error",
        "receipt-error",
        "invalid-clear",
        "invalid-view",
        "frame-exit",
        "synthetic-close",
        "measured",
    ] {
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .arg(format!("--live-case={case}"))
            .status()
            .unwrap();
        assert!(
            status.success(),
            "live window scenario {case} failed: {status}"
        );
        assert_process_windows_closed(case);
    }
}

#[cfg(target_os = "linux")]
fn assert_process_windows_closed(case: &str) {
    let title = match case {
        "desktop" => "worldrunner-live-contract-test".to_owned(),
        "ingress" | "ingress-acquisition" => "worldrunner-timestamped-ingress".to_owned(),
        _ => format!("live lifecycle {case}"),
    };
    let output = std::process::Command::new("xdotool")
        .args(["search", "--name", &format!("^{title}$")])
        .output()
        .unwrap();
    assert!(
        !output.status.success(),
        "scenario process left its window behind"
    );
}
