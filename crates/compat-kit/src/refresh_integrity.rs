use crate::{
    files::{self, Result, io},
    model::{CommandSpec, Fixture},
    process::{self, Execution, RunControl},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Selection {
    schema_version: u32,
    mode: String,
}
pub(crate) struct ClosureCheck<'a> {
    root: &'a Path,
    fixture: &'a Fixture,
    output: &'a Path,
    watched: BTreeMap<PathBuf, String>,
    proof: Option<Value>,
}
impl<'a> ClosureCheck<'a> {
    pub(crate) fn new(
        root: &'a Path,
        fixture: &'a Fixture,
        output: &'a Path,
    ) -> Result<Option<Self>> {
        let Some(value) = fixture.metadata.get("refresh_integrity") else {
            return Ok(None);
        };
        let selection: Selection = serde_json::from_value(value.clone()).map_err(io)?;
        if selection.schema_version != 1 {
            return Err("unsupported refresh integrity schema".into());
        }
        match selection.mode.as_str() {
            "strict-closure" => Ok(Some(Self { root, fixture, output, watched: BTreeMap::new(), proof: None })),
            "dependency-admission" => Err("unsupported dependency-admission: trusted host/provider AdmissionProof recomputation is not integrated; adoption blocked".into()),
            _ => Err("unsupported refresh integrity mode".into()),
        }
    }
    fn proof_path(&self) -> PathBuf {
        self.output.join("candidate-assembly.json")
    }
    pub(crate) fn observe(&mut self, candidate: &Path) -> Result<()> {
        self.verify_watched()?;
        if self.proof.is_some() || !self.proof_path().try_exists().map_err(io)? {
            return Ok(());
        }
        let proof_path = files::safe_file(self.output, Path::new("candidate-assembly.json"))?;
        let proof: Value = files::read_json(&proof_path)?;
        if proof["source_root"].as_str() != candidate.to_str() {
            return Err("compile proof is not bound to this candidate source root".into());
        }
        let assembly = PathBuf::from(
            proof["assembly"]
                .as_str()
                .ok_or("compile proof assembly missing")?,
        );
        let relative = assembly
            .strip_prefix(self.output)
            .map_err(|_| "qualified assembly must be inside gate output")?;
        let assembly = files::safe_file(self.output, relative)?;
        if proof["assembly_sha256"].as_str() != Some(files::hash(&assembly)?.as_str()) {
            return Err("compile proof assembly hash mismatch".into());
        }
        for path in [proof_path, assembly] {
            self.watch(path)?;
        }
        for relative in [
            "tools/compat-refresh/check.py",
            "fixtures/gates/candidate.py",
            "fixtures/gates/qualification.py",
        ] {
            self.watch(files::safe_file(self.root, Path::new(relative))?)?;
        }
        self.watch(self.receipt()?)?;
        self.proof = Some(proof);
        Ok(())
    }
    fn receipt(&self) -> Result<PathBuf> {
        let path = std::env::var_os("COMPAT_CONTENT_RECEIPT").map_or_else(
            || self.root.join(".work/target/debug/examples/receipt"),
            PathBuf::from,
        );
        if !path.is_absolute() {
            return Err(
                "COMPAT_CONTENT_RECEIPT must be an absolute trusted executable path".into(),
            );
        }
        files::safe_file(Path::new("/"), path.strip_prefix("/").map_err(io)?)
    }
    fn watch(&mut self, path: PathBuf) -> Result<()> {
        let hash = files::hash(&path)?;
        self.watched.insert(path, hash);
        Ok(())
    }
    pub(crate) fn verify_watched(&self) -> Result<()> {
        for (path, expected) in &self.watched {
            if !fs::symlink_metadata(path).map_err(io)?.is_file() || &files::hash(path)? != expected
            {
                return Err(format!(
                    "qualification proof/assembly/tool bytes changed before adoption: {}",
                    path.display()
                ));
            }
        }
        Ok(())
    }
    pub(crate) fn check(&mut self, candidate: &Path, control: &RunControl) -> Result<Value> {
        self.observe(candidate)?;
        let proof = self.proof.clone().ok_or("strict closure proof requested but candidate-assembly.json is missing; adoption blocked")?;
        let selected = self.output.join("adoption-fixture.json");
        files::write_json(&selected, self.fixture)?;
        self.watch(selected.clone())?;
        let logs = self.output.join("pre-adoption");
        let environment = BTreeMap::from([(
            "COMPAT_ROOT".into(),
            self.root.to_string_lossy().into_owned(),
        )]);
        let command = self.command(&selected, &logs)?;
        let options = Execution {
            cwd: self.root,
            env: &environment,
            logs: &logs,
            artifacts: Some(self.output),
            limits: &self.fixture.limits,
        };
        let outcome = process::execute(&command, &options, control)?;
        if !outcome.success || outcome.interrupted.is_some() || outcome.truncated_logs {
            return Err(format!(
                "pre-adoption content checker rejected candidate: {} {:?}; evidence {}",
                outcome.exit,
                outcome.interrupted,
                logs.display()
            ));
        }
        self.verify_watched()?;
        let result = checker_result(&logs)?;
        if result["status"] != "pass"
            || result["closure_sha256"] != proof["content_receipt"]["closure_sha256"]
        {
            return Err("pre-adoption checker returned an unbound closure result".into());
        }
        Ok(
            json!({"status":"pass","mode":"strict-closure","engine_proof":false,"scope":"strict-content-and-assembly","checker":result,"proof":self.proof,"observed_sha256":self.watched}),
        )
    }
    fn command(&self, fixture: &Path, logs: &Path) -> Result<CommandSpec> {
        Ok(CommandSpec {
            program: "python3".into(),
            args: vec![
                "-I".into(),
                "-B".into(),
                files::safe_file(self.root, Path::new("tools/compat-refresh/check.py"))?
                    .to_string_lossy()
                    .into_owned(),
                fixture.to_string_lossy().into_owned(),
                self.proof_path().to_string_lossy().into_owned(),
                "--receipt".into(),
                self.receipt()?.to_string_lossy().into_owned(),
                "--output".into(),
                logs.to_string_lossy().into_owned(),
            ],
        })
    }
}
fn checker_result(logs: &Path) -> Result<Value> {
    let text = fs::read_to_string(logs.join("stdout.log")).map_err(io)?;
    serde_json::from_str(
        text.lines()
            .last()
            .ok_or("pre-adoption checker result missing")?,
    )
    .map_err(io)
}
