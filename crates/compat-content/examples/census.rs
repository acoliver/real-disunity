use std::{env, path::Path};
use worldrunner_compat_content::{ImportProfile, script_census};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = env::args().collect();
    if args.len() != 3 {
        return Err("usage: census PROJECT_ROOT RELATIVE_SCRIPT_DIRECTORY".into());
    }
    let scripts = script_census(
        Path::new(&args[1]),
        Path::new(&args[2]),
        &ImportProfile::default(),
    )?;
    serde_json::to_writer_pretty(std::io::stdout().lock(), &scripts)?;
    Ok(())
}
