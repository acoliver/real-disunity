use std::{env, fs, path::Path};
use worldrunner_compat_content::{
    AdmissionPolicy, DependencyPolicy, ImportProfile, ImportSidecar, TrustedProviderInventory,
    import_receipt,
};

/// Reporting entry point. A blocked outcome is valid output, never qualification.
/// Executable providers must be composed by the host through the Rust API.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = env::args().collect();
    if args.len() != 7 {
        return Err("usage: dependency_receipt ROOT SCENE PROFILE_JSON SIDECAR_JSON COMPONENTS_JSON DEPENDENCIES_JSON".into());
    }
    let profile: ImportProfile = serde_json::from_slice(&fs::read(&args[3])?)?;
    let sidecar: ImportSidecar = serde_json::from_slice(&fs::read(&args[4])?)?;
    let components: AdmissionPolicy = serde_json::from_slice(&fs::read(&args[5])?)?;
    let dependencies = DependencyPolicy::from_json(&fs::read(&args[6])?)?;
    let receipt = import_receipt(Path::new(&args[1]), Path::new(&args[2]), &profile, &sidecar)?;
    let result = receipt.admit(&components, &dependencies, &TrustedProviderInventory::new());
    receipt.verify_current(Path::new(&args[1]))?;
    match result {
        Ok(admitted) => serde_json::to_writer_pretty(std::io::stdout().lock(), admitted.proof())?,
        Err(blocked) => serde_json::to_writer_pretty(std::io::stdout().lock(), &blocked.proof)?,
    }
    Ok(())
}
