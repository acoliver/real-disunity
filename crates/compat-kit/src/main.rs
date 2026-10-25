use compat_kit::{
    RunControl, dashboard,
    files::{Result, io},
    fixture, runner, server,
};
use serde_json::{Value, json};
use std::{
    env,
    path::{Path, PathBuf},
    process::Command,
};

fn main() {
    match dispatch() {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("compat-kit: {error}");
            std::process::exit(2);
        }
    }
}
fn output(value: &impl serde::Serialize) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value).map_err(io)?);
    Ok(())
}
fn dispatch() -> Result<i32> {
    let root = env::var_os("COMPAT_ROOT")
        .map_or_else(|| env::current_dir().map_err(io), |p| Ok(PathBuf::from(p)))?
        .canonicalize()
        .map_err(io)?;
    let args: Vec<_> = env::args().skip(1).collect();
    let command = args.first().map_or("help", String::as_str);
    match command {
        "doctor" => {
            output(&doctor(&root)?)?;
        }
        "run" => return run_command(&root, &args),
        "fixture" => return fixture_command(&root, &args),
        "dashboard" => {
            if args.len() > 2 {
                return Err("dashboard accepts one output directory".into());
            }
            let path = args
                .get(1)
                .map_or_else(|| Path::new(".work/compat/dashboard"), Path::new);
            output(&json!({"dashboard":dashboard::generate(&root,path)?}))?;
        }
        "serve" => {
            let port = if args.len() == 1 {
                8787
            } else if args.len() == 3 && args[1] == "--port" {
                args[2].parse().map_err(io)?
            } else {
                return Err("serve [--port N]".into());
            };
            server::serve(&root, port)?;
        }
        "help" | "--help" | "-h" => println!(
            "compat-kit doctor\ncompat-kit fixture check [name]\ncompat-kit run <fixture> [--desktop] [--profile id]\ncompat-kit dashboard [output-directory-inside-.work]\ncompat-kit serve [--port 8787]"
        ),
        other => return Err(format!("unknown command: {other}")),
    }
    Ok(0)
}
fn doctor(root: &Path) -> Result<Value> {
    let fixtures = fixture::catalog(root)?;
    let mut checks = Vec::new();
    for fixture in fixtures {
        let source = fixture::check(&fixture).unwrap_or_else(|error| json!({"error":error}));
        let worker = fixture.worker.as_ref().map(|w| w.program.clone());
        let executable = worker.as_ref().is_some_and(|w| {
            if w.contains('/') {
                Path::new(w).is_file()
            } else {
                env::var_os("PATH")
                    .is_some_and(|path| env::split_paths(&path).any(|p| p.join(w).is_file()))
            }
        });
        checks.push(json!({"fixture":fixture.name,"source":source,"worker":worker,"worker_available":executable,"status":if executable {"inconclusive"} else {"blocked"}}));
    }
    let storage = Command::new("du")
        .args(["-sb"])
        .arg(root)
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| {
            s.split_whitespace()
                .next()
                .and_then(|n| n.parse::<u64>().ok())
        });
    Ok(
        json!({"schema_version":1,"root":root,"worktree_bytes":storage,"budget_bytes":500_000_000_000_u64,"within_budget":storage.map(|n|n<=500_000_000_000),"fixtures":checks,"note":"Tool/source checks establish prerequisites only. Game compatibility requires facade-backed worker observations and assertions."}),
    )
}

fn run_command(root: &Path, args: &[String]) -> Result<i32> {
    let name = args
        .get(1)
        .ok_or("run requires fixture name or JSON path")?;
    let mut profile = None;
    let mut desktop = false;
    let mut index = 2;
    while index < args.len() {
        match args[index].as_str() {
            "--desktop" => desktop = true,
            "--profile" => {
                index += 1;
                profile = Some(args.get(index).ok_or("--profile requires value")?.as_str());
            }
            other => return Err(format!("unknown run option: {other}")),
        }
        index += 1;
    }
    let fixture = fixture::load(root, name)?;
    let record = match runner::run(root, &fixture, profile, desktop, &RunControl::default()) {
        Ok(record) => record,
        Err(error) => {
            eprintln!("compat-kit infrastructure: {error}");
            return Ok(3);
        }
    };
    output(&record)?;
    Ok(compat_kit::reporting::exit_code(&record))
}

fn fixture_command(root: &Path, args: &[String]) -> Result<i32> {
    let action = args.get(1).ok_or("fixture requires check")?;
    if action != "check" {
        return Err("fixture requires check".into());
    }
    let names: Vec<_> = args.iter().skip(2).collect();
    if names.len() > 1 {
        return Err("fixture accepts at most one name".into());
    }
    let fixtures = if let Some(name) = names.first() {
        vec![fixture::load(root, name)?]
    } else {
        fixture::catalog(root)?
    };
    let mut results = Vec::new();
    for fixture in fixtures {
        let result = fixture::check(&fixture);
        results.push(result.unwrap_or_else(
            |error| json!({"fixture":fixture.name,"status":"blocked","message":error}),
        ));
    }
    let failed = results.iter().any(|result| {
        result["status"]
            .as_str()
            .is_some_and(|status| status != "pass")
    });
    if results.len() == 1 {
        output(&results[0])?;
    } else {
        output(&results)?;
    }
    Ok(if failed { 2 } else { 0 })
}
