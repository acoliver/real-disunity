use super::{QualifiedProviders, binding::Binding};
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};
use worldrunner::compat_content::{
    ApiSurface, AssemblyApiUse, ContentReceipt, GameSourceQualification, ImportProfile,
    ScriptSource, TrustedGameSourceProof, script_census,
};

#[derive(Clone, Copy)]
pub enum GameSource {
    Snake,
    Pong,
    Breakout,
}
impl GameSource {
    fn name(self) -> &'static str {
        match self {
            Self::Snake => "Snake",
            Self::Pong => "Pong",
            Self::Breakout => "Breakout",
        }
    }
}
#[derive(Deserialize)]
struct Report {
    path: PathBuf,
    pe_sha256: String,
    pdb_sha256: String,
    documents: Vec<Document>,
    source_types: BTreeMap<String, PathBuf>,
    api_refs: Vec<ApiReference>,
    assembly_refs: Vec<String>,
    failures: Vec<String>,
}
#[derive(Deserialize)]
struct ApiReference {
    assembly: String,
    type_name: String,
    member: String,
}
#[derive(Deserialize)]
struct Document {
    path: PathBuf,
    expected: String,
    matched: bool,
}
struct Proof {
    profile: String,
    closure: String,
    sources: Vec<ScriptSource>,
    assembly: Vec<u8>,
    evidence: Vec<u8>,
    types: BTreeMap<String, String>,
    api: Vec<AssemblyApiUse>,
}
impl QualifiedProviders {
    /// Return source evidence for a sealed PE. Content policy still decides admission.
    pub fn game_source(
        &self,
        game: GameSource,
        receipt: &ContentReceipt,
        profile: &str,
    ) -> Result<GameSourceQualification, String> {
        self.verify_current()?;
        let proof = inspect(self.binding_for_source(), game, receipt, profile)?;
        GameSourceQualification::from_host(&proof)
    }
}
fn inspect(
    binding: &Binding,
    game: GameSource,
    receipt: &ContentReceipt,
    profile: &str,
) -> Result<Proof, String> {
    let bytes = std::fs::read(
        binding.path(".work/resumed-provider-qualification-v2/api-closure-recovered.json"),
    )
    .map_err(|e| e.to_string())?;
    let reports: Vec<Report> = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    let assembly = binding.path(&format!(
        ".work/resumed-provider-qualification-v2/managed/artifacts/bin/{0}/debug/{0}.dll",
        game.name()
    ));
    let report = reports
        .iter()
        .find(|r| r.path == assembly)
        .ok_or("no sealed game PE inspection")?;
    if !report.failures.is_empty()
        || report.assembly_refs.iter().any(|a| {
            a != "UnityCompat"
                && a != "System"
                && a != "mscorlib"
                && a != "netstandard"
                && !a.starts_with("System.")
        })
    {
        return Err("unqualified transitive game API closure".into());
    }
    if compat_kit::files::hash(&assembly)? != report.pe_sha256
        || compat_kit::files::hash(&assembly.with_extension("pdb"))? != report.pdb_sha256
    {
        return Err("actual PE/PDB inspection changed".into());
    }
    let root = binding.path(&format!(".work/games/{}", game.name().to_lowercase()));
    receipt.verify_current(&root).map_err(|e| e.to_string())?;
    let sources = script_census(
        &root,
        Path::new("Assets/Scripts"),
        &ImportProfile::default(),
    )
    .map_err(|e| e.to_string())?;
    verify_sources(&root, &sources, report)?;
    let types = report
        .source_types
        .iter()
        .map(|(name, path)| Ok((name.clone(), relative(&root, path)?)))
        .collect::<Result<_, String>>()?;
    let api = report.api_refs.iter().filter_map(api_use).collect();
    Ok(Proof {
        profile: profile.into(),
        closure: receipt.closure_sha256().into(),
        sources,
        assembly: std::fs::read(assembly).map_err(|e| e.to_string())?,
        evidence: bytes,
        types,
        api,
    })
}
fn relative(root: &Path, path: &Path) -> Result<String, String> {
    let relative = path.strip_prefix(root).map_err(|e| e.to_string())?;
    compat_kit::files::relative_path(relative)?;
    Ok(relative.to_string_lossy().into_owned())
}
fn verify_sources(root: &Path, sources: &[ScriptSource], report: &Report) -> Result<(), String> {
    let documents = report
        .documents
        .iter()
        .filter(|d| d.path.starts_with(root.join("Assets/Scripts")))
        .map(|d| {
            if !d.matched {
                return Err("unverified portable PDB source".into());
            }
            Ok((relative(root, &d.path)?, d.expected.clone()))
        })
        .collect::<Result<BTreeMap<_, _>, String>>()?;
    let current: BTreeMap<_, _> = sources
        .iter()
        .map(|s| (s.path.clone(), s.sha256.clone()))
        .collect();
    if documents.is_empty() || documents != current {
        return Err("complete compiler input/PDB/source census changed".into());
    }
    Ok(())
}
fn api_use(reference: &ApiReference) -> Option<AssemblyApiUse> {
    if reference.assembly != "UnityCompat" {
        return None;
    }
    let name = reference.type_name.as_str();
    let surface = if name == "UnityEngine.Input"
        || matches!(
            name,
            "UnityEngine.InputSystem.Keyboard" | "UnityEngine.InputSystem.KeyControl"
        ) {
        ApiSurface::KeyboardSnapshot
    } else if name.starts_with("UnityEngine.InputSystem.") {
        ApiSurface::InputActions
    } else {
        ApiSurface::CanonicalFacade
    };
    Some(AssemblyApiUse {
        assembly: reference.assembly.clone(),
        type_name: name.into(),
        member: reference.member.clone(),
        surface,
    })
}

impl TrustedGameSourceProof for Proof {
    fn profile_id(&self) -> &str {
        &self.profile
    }
    fn content_closure_sha256(&self) -> &str {
        &self.closure
    }
    fn source_directory(&self) -> &Path {
        Path::new("Assets/Scripts")
    }
    fn sources(&self) -> Vec<ScriptSource> {
        self.sources.clone()
    }
    fn assembly_bytes(&self) -> &[u8] {
        &self.assembly
    }
    fn verification_evidence_bytes(&self) -> &[u8] {
        &self.evidence
    }
    fn defined_types(&self) -> BTreeMap<String, String> {
        self.types.clone()
    }
    fn api_uses(&self) -> Vec<AssemblyApiUse> {
        self.api.clone()
    }
}
