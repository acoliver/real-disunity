use std::{env, fs, path::Path};
use worldrunner_compat_content::{
    AdmissionPolicy, ImportProfile, ImportSidecar, import_receipt, materialization_plan,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = env::args().collect();
    if args.len() != 6 {
        return Err(
            "usage: receipt PROJECT_ROOT SCENE PROFILE_JSON SIDECAR_JSON ADMISSION_JSON".into(),
        );
    }
    let profile: ImportProfile = serde_json::from_slice(&fs::read(&args[3])?)?;
    let sidecar: ImportSidecar = serde_json::from_slice(&fs::read(&args[4])?)?;
    let policy: AdmissionPolicy = serde_json::from_slice(&fs::read(&args[5])?)?;
    let receipt = import_receipt(Path::new(&args[1]), Path::new(&args[2]), &profile, &sidecar)?;
    materialization_plan(receipt.graph(), &policy)?;
    receipt.verify_current(Path::new(&args[1]))?;
    serde_json::to_writer(std::io::stdout().lock(), &receipt.proof())?;
    Ok(())
}
