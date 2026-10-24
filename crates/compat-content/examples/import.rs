use std::{env, path::Path};
use worldrunner_compat_content::{ImportProfile, ImportSidecar, import_project};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = env::args().collect();
    if args.len() < 3 {
        return Err(
            "usage: import PROJECT_ROOT RELATIVE_SCENE [PROFILE_JSON] [SIDECAR_JSON]".into(),
        );
    }
    let profile = if let Some(path) = args.get(3) {
        serde_json::from_slice(&std::fs::read(path)?)?
    } else {
        ImportProfile::default()
    };
    let sidecar = if let Some(path) = args.get(4) {
        serde_json::from_slice(&std::fs::read(path)?)?
    } else {
        ImportSidecar::default()
    };
    let graph = import_project(Path::new(&args[1]), Path::new(&args[2]), &profile, &sidecar)?;
    serde_json::to_writer_pretty(std::io::stdout().lock(), &graph)?;
    Ok(())
}
